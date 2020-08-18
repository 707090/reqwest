use std::future::Future;

use crate::header::{CONTENT_LENGTH, CONTENT_TYPE};

use super::{Client, client::Pending, multipart, response::Response};

/// A request which can be executed with `Client::execute()`.
pub type Request = crate::core::request::Request<super::Body>;
/// A builder to construct the properties of a `Request`.
pub type RequestBuilder = crate::core::request::RequestBuilder<super::Body>;

impl RequestBuilder {
    /// Sends a multipart/form-data body.
    ///
    /// ```
    /// # use reqwest::Error;
    ///
    /// # async fn run() -> Result<(), Error> {
    /// let client = reqwest::Client::new();
    /// let form = reqwest::multipart::Form::new()
    ///     .text("key3", "value3")
    ///     .text("key4", "value4");
    ///
    ///
    /// let response = reqwest::RequestBuilder::post("your url")
    ///     .multipart(form)
    ///     .send(&client)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn multipart(self, mut multipart: multipart::Form) -> RequestBuilder {
        let mut builder = self.header(
            CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", multipart.boundary()).as_str(),
        );

        builder = match multipart.compute_length() {
            Some(length) => builder.header(CONTENT_LENGTH, length),
            None => builder,
        };

        if let Ok(ref mut req) = builder.request {
            *req.body_mut() = Some(multipart.stream())
        }
        builder
    }

    /// Constructs the Request and sends it to the target URL using the specified client and returns
    /// a future Response.
    ///
    /// # Errors
    ///
    /// This method fails if there was an error while building the request, sending the request,
    /// redirect loop was detected or redirect limit was exhausted.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use reqwest::Error;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// let response = reqwest::RequestBuilder::get("https://hyper.rs")
    ///     .send(&reqwest::Client::new())
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn send(self, client: &Client) -> impl Future<Output=Result<Response, crate::Error>> {
        match self.request {
            Ok(req) => client.execute_request(req),
            Err(err) => Pending::new_err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::convert::TryFrom;

    use fallible::TryClone;
    use http::Request as HttpRequest;
    use serde::Serialize;

    use crate::{Method, RequestBuilder};

    use super::Request;

    #[test]
    fn add_query_append() {
        let some_url = "https://google.com/";
        let req = RequestBuilder::get(some_url)
            .query(&[("foo", "bar")])
            .query(&[("qux", 3)])
            .build()
            .expect("request is valid");
        assert_eq!(req.url().query(), Some("foo=bar&qux=3"));
    }

    #[test]
    fn add_query_append_same() {
        let some_url = "https://google.com/";
        let req = RequestBuilder::get(some_url)
            .query(&[("foo", "a"), ("foo", "b")])
            .build()
            .expect("request is valid");
        assert_eq!(req.url().query(), Some("foo=a&foo=b"));
    }

    #[test]
    fn add_query_struct() {
        #[derive(Serialize)]
        struct Params {
            foo: String,
            qux: i32,
        }

        let some_url = "https://google.com/";
        let params = Params {
            foo: "bar".into(),
            qux: 3,
        };
        let req = RequestBuilder::get(some_url)
            .query(&params)
            .build()
            .expect("request is valid");
        assert_eq!(req.url().query(), Some("foo=bar&qux=3"));
    }

    #[test]
    fn add_query_map() {
        let mut params = BTreeMap::new();
        params.insert("foo", "bar");
        params.insert("qux", "three");

        let some_url = "https://google.com/";
        let req = RequestBuilder::get(some_url)
            .query(&params)
            .build()
            .expect("request is valid");
        assert_eq!(req.url().query(), Some("foo=bar&qux=three"));
    }

    #[test]
    fn test_replace_headers() {
        use http::HeaderMap;

        let mut headers = HeaderMap::new();
        headers.insert("foo", "bar".parse().unwrap());
        headers.append("foo", "baz".parse().unwrap());

        let req = RequestBuilder::get("https://hyper.rs")
            .header("im-a", "keeper")
            .header("foo", "pop me")
            .headers(headers)
            .build()
            .expect("request build");

        assert_eq!(req.headers()["im-a"], "keeper");

        let foo = req.headers().get_all("foo").iter().collect::<Vec<_>>();
        assert_eq!(foo.len(), 2);
        assert_eq!(foo[0], "bar");
        assert_eq!(foo[1], "baz");
    }

    #[test]
    fn normalize_empty_query() {
        let some_url = "https://google.com/";
        let empty_query: &[(&str, &str)] = &[];

        let req = RequestBuilder::get(some_url)
            .query(empty_query)
            .build()
            .expect("request build");

        assert_eq!(req.url().query(), None);
        assert_eq!(req.url().as_str(), "https://google.com/");
    }

    #[test]
    fn try_clone_reusable() {
        let builder = RequestBuilder::post("http://httpbin.org/post")
            .header("foo", "bar")
            .body("from a &str!");
        let req = builder
            .try_clone()
            .expect("clone successful")
            .build()
            .expect("request is valid");
        assert_eq!(req.url().as_str(), "http://httpbin.org/post");
        assert_eq!(req.method(), Method::POST);
        assert_eq!(req.headers()["foo"], "bar");
    }

    #[test]
    fn try_clone_no_body() {
        let req = RequestBuilder::get("http://httpbin.org/get")
            .try_clone()
            .expect("clone successful")
            .build()
            .expect("request is valid");
        assert_eq!(req.url().as_str(), "http://httpbin.org/get");
        assert_eq!(req.method(), Method::GET);
        assert!(req.body().is_none());
    }

    #[test]
    #[cfg(feature = "stream")]
    fn try_clone_stream() {
        let chunks: Vec<Result<_, ::std::io::Error>> = vec![
            Ok("hello"),
            Ok(" "),
            Ok("world"),
        ];
        let stream = futures_util::stream::iter(chunks);

        let builder = RequestBuilder::get("http://httpbin.org/get")
            .body(super::super::Body::wrap_stream(stream));
        let clone = builder.try_clone();
        assert!(clone.is_err());
    }

    #[test]
    fn convert_url_authority_into_basic_auth() {
        let some_url = "https://Aladdin:open sesame@localhost/";

        let req = RequestBuilder::get(some_url)
            .build()
            .expect("request build");

        assert_eq!(req.url().as_str(), "https://localhost/");
        assert_eq!(req.headers()["authorization"], "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==");
    }

    #[test]
    fn test_basic_auth_sensitive_header() {
        let some_url = "https://localhost/";

        let req = RequestBuilder::get(some_url)
            .basic_auth("Aladdin", Some("open sesame"))
            .build()
            .expect("request build");

        assert_eq!(req.url().as_str(), "https://localhost/");
        assert_eq!(req.headers()["authorization"], "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==");
        assert_eq!(req.headers()["authorization"].is_sensitive(), true);
    }

    #[test]
    fn test_bearer_auth_sensitive_header() {
        let some_url = "https://localhost/";

        let req = RequestBuilder::get(some_url)
            .bearer_auth("Hold my bear")
            .build()
            .expect("request build");

        assert_eq!(req.url().as_str(), "https://localhost/");
        assert_eq!(req.headers()["authorization"], "Bearer Hold my bear");
        assert_eq!(req.headers()["authorization"].is_sensitive(), true);
    }


    #[test]
    fn convert_from_http_request() {
        let http_request = HttpRequest::builder().method("GET")
            .uri("http://localhost/")
            .header("User-Agent", "my-awesome-agent/1.0")
            .body("test test test")
            .unwrap();
        let req: Request = Request::try_from(http_request).unwrap();
        assert_eq!(req.body().is_none(), false);
        let test_data = b"test test test";
        assert_eq!(req.body().unwrap().as_bytes(), Some(&test_data[..]));
        let headers = req.headers();
        assert_eq!(headers.get("User-Agent").unwrap(), "my-awesome-agent/1.0");
        assert_eq!(req.method(), Method::GET);
        assert_eq!(req.url().as_str(), "http://localhost/");
    }

    /*
    use {body, Method};
    use super::Client;
    use header::{Host, Headers, ContentType};
    use std::collections::HashMap;
    use serde_urlencoded;
    use serde_json;

    #[test]
    fn basic_get_request() {
        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let r = client.get(some_url).unwrap().build();

        assert_eq!(r.method, Method::Get);
        assert_eq!(r.url.as_str(), some_url);
    }

    #[test]
    fn basic_head_request() {
        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let r = client.head(some_url).unwrap().build();

        assert_eq!(r.method, Method::Head);
        assert_eq!(r.url.as_str(), some_url);
    }

    #[test]
    fn basic_post_request() {
        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let r = client.post(some_url).unwrap().build();

        assert_eq!(r.method, Method::Post);
        assert_eq!(r.url.as_str(), some_url);
    }

    #[test]
    fn basic_put_request() {
        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let r = client.put(some_url).unwrap().build();

        assert_eq!(r.method, Method::Put);
        assert_eq!(r.url.as_str(), some_url);
    }

    #[test]
    fn basic_patch_request() {
        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let r = client.patch(some_url).unwrap().build();

        assert_eq!(r.method, Method::Patch);
        assert_eq!(r.url.as_str(), some_url);
    }

    #[test]
    fn basic_delete_request() {
        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let r = client.delete(some_url).unwrap().build();

        assert_eq!(r.method, Method::Delete);
        assert_eq!(r.url.as_str(), some_url);
    }

    #[test]
    fn add_header() {
        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let mut r = client.post(some_url).unwrap();

        let header = Host {
            hostname: "google.com".to_string(),
            port: None,
        };

        // Add a copy of the header to the request builder
        let r = r.header(header.clone()).build();

        // then check it was actually added
        assert_eq!(r.headers.get::<Host>(), Some(&header));
    }

    #[test]
    fn add_headers() {
        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let mut r = client.post(some_url).unwrap();

        let header = Host {
            hostname: "google.com".to_string(),
            port: None,
        };

        let mut headers = Headers::new();
        headers.set(header);

        // Add a copy of the headers to the request builder
        let r = r.headers(headers.clone()).build();

        // then make sure they were added correctly
        assert_eq!(r.headers, headers);
    }

    #[test]
    fn add_headers_multi() {
        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let mut r = client.post(some_url).unwrap();

        let header = Host {
            hostname: "google.com".to_string(),
            port: None,
        };

        let mut headers = Headers::new();
        headers.set(header);

        // Add a copy of the headers to the request builder
        let r = r.headers(headers.clone()).build();

        // then make sure they were added correctly
        assert_eq!(r.headers, headers);
    }

    #[test]
    fn add_body() {
        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let mut r = client.post(some_url).unwrap();

        let body = "Some interesting content";

        let r = r.body(body).build();

        let buf = body::read_to_string(r.body.unwrap()).unwrap();

        assert_eq!(buf, body);
    }

    #[test]
    fn add_form() {
        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let mut r = client.post(some_url).unwrap();

        let mut form_data = HashMap::new();
        form_data.insert("foo", "bar");

        let r = r.form(&form_data).unwrap().build();

        // Make sure the content type was set
        assert_eq!(r.headers.get::<ContentType>(),
                   Some(&ContentType::form_url_encoded()));

        let buf = body::read_to_string(r.body.unwrap()).unwrap();

        let body_should_be = serde_urlencoded::to_string(&form_data).unwrap();
        assert_eq!(buf, body_should_be);
    }

    #[test]
    fn add_json() {
        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let mut r = client.post(some_url).unwrap();

        let mut json_data = HashMap::new();
        json_data.insert("foo", "bar");

        let r = r.json(&json_data).unwrap().build();

        // Make sure the content type was set
        assert_eq!(r.headers.get::<ContentType>(), Some(&ContentType::json()));

        let buf = body::read_to_string(r.body.unwrap()).unwrap();

        let body_should_be = serde_json::to_string(&json_data).unwrap();
        assert_eq!(buf, body_should_be);
    }

    #[test]
    fn add_json_fail() {
        use serde::{Serialize, Serializer};
        use serde::ser::Error;
        struct MyStruct;
        impl Serialize for MyStruct {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
                where S: Serializer
                {
                    Err(S::Error::custom("nope"))
                }
        }

        let client = Client::new().unwrap();
        let some_url = "https://google.com/";
        let mut r = client.post(some_url).unwrap();
        let json_data = MyStruct{};
        assert!(r.json(&json_data).unwrap_err().is_serialization());
    }
    */
}
