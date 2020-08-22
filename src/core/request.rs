use std::convert::TryFrom;
use std::time::Duration;

use base64::encode;
use fallible::TryClone;
use futures_core::Future;
use futures_util::future::ready;
use http::{request::Parts, Request as HttpRequest};
use serde::Serialize;
#[cfg(feature = "json")]
use serde_json;
use url::Url;

use crate::async_impl::client::future::WrapFuture;
use crate::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE};
use crate::Method;
use crate::{multipart, Body, IntoUrl, Response};

/// A request which can be executed with `Client::execute()`.
pub struct Request {
    pub(crate) method: Method,
    pub(crate) url: Url,
    pub(crate) headers: HeaderMap,
    pub(crate) body: Option<Body>,
    pub(crate) timeout: Option<Duration>,
    pub(crate) cors: bool,
}

impl Request {
    /// Constructs a new request.
    #[inline]
    pub fn new(method: Method, url: Url) -> Self {
        Request {
            method,
            url,
            headers: HeaderMap::new(),
            body: None,
            timeout: None,
            cors: true,
        }
    }

    /// Get the method.
    #[inline]
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// Get a mutable reference to the method.
    #[inline]
    pub fn method_mut(&mut self) -> &mut Method {
        &mut self.method
    }

    /// Get the url.
    #[inline]
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Get a mutable reference to the url.
    #[inline]
    pub fn url_mut(&mut self) -> &mut Url {
        &mut self.url
    }

    /// Get the headers.
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Get a mutable reference to the headers.
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }

    /// Get the body.
    #[inline]
    pub fn body(&self) -> Option<&Body> {
        self.body.as_ref()
    }

    /// Get a mutable reference to the body.
    #[inline]
    pub fn body_mut(&mut self) -> &mut Option<Body> {
        &mut self.body
    }

    /// Get the timeout.
    #[inline]
    pub fn timeout(&self) -> Option<&Duration> {
        self.timeout.as_ref()
    }

    /// Get a mutable reference to the timeout.
    #[inline]
    pub fn timeout_mut(&mut self) -> &mut Option<Duration> {
        &mut self.timeout
    }
}

impl TryClone for Request {
    type Error = crate::error::Error;

    fn try_clone(&self) -> Result<Self, Self::Error> {
        let body = match self.body.as_ref() {
            Some(ref body) => Some((*body).try_clone()?),
            None => None,
        };
        let mut req = Request::new(self.method().clone(), self.url().clone());
        *req.timeout_mut() = self.timeout().cloned();
        *req.headers_mut() = self.headers().clone();
        req.body = body;
        Ok(req)
    }
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt_request_fields(&mut f.debug_struct("Request"), self).finish()
    }
}

impl<T: Into<Body>> TryFrom<HttpRequest<T>> for Request {
    type Error = crate::Error;

    fn try_from(req: HttpRequest<T>) -> crate::Result<Self> {
        let (parts, body) = req.into_parts();
        let Parts {
            method,
            uri,
            headers,
            ..
        } = parts;
        let url = Url::parse(&uri.to_string()).map_err(crate::error::builder)?;
        Ok(Request {
            method,
            url,
            headers,
            body: Some(body.into()),
            timeout: None,
            cors: true,
        })
    }
}

/// A builder to construct the properties of a `Request`.
pub struct RequestBuilder {
    pub(crate) request: crate::Result<Request>,
}

impl RequestBuilder {
    /// Start building a `Request` with the `Method` and `Url`.
    ///
    /// Returns a `RequestBuilder`, which will allow setting headers and
    /// request body before sending.
    ///
    /// # Errors
    ///
    /// This method fails whenever supplied `Url` cannot be parsed.
    pub fn new<U: IntoUrl>(method: Method, url: U) -> RequestBuilder {
        let request = url.into_url().map(move |url| Request::new(method, url));
        let mut builder = RequestBuilder { request };

        let auth = builder
            .request
            .as_mut()
            .ok()
            .and_then(|req| crate::core::request::extract_authority(&mut req.url));

        if let Some((username, password)) = auth {
            builder.basic_auth(username, password)
        } else {
            builder
        }
    }

    /// Convenience method to make a `GET` request to a URL.
    ///
    /// # Errors
    ///
    /// This method fails whenever supplied `Url` cannot be parsed.
    pub fn get<U: IntoUrl>(url: U) -> RequestBuilder {
        RequestBuilder::new(Method::GET, url)
    }

    /// Convenience method to make a `POST` request to a URL.
    ///
    /// # Errors
    ///
    /// This method fails whenever supplied `Url` cannot be parsed.
    pub fn post<U: IntoUrl>(url: U) -> RequestBuilder {
        RequestBuilder::new(Method::POST, url)
    }

    /// Convenience method to make a `PUT` request to a URL.
    ///
    /// # Errors
    ///
    /// This method fails whenever supplied `Url` cannot be parsed.
    pub fn put<U: IntoUrl>(url: U) -> RequestBuilder {
        RequestBuilder::new(Method::PUT, url)
    }

    /// Convenience method to make a `PATCH` request to a URL.
    ///
    /// # Errors
    ///
    /// This method fails whenever supplied `Url` cannot be parsed.
    pub fn patch<U: IntoUrl>(url: U) -> RequestBuilder {
        RequestBuilder::new(Method::PATCH, url)
    }

    /// Convenience method to make a `DELETE` request to a URL.
    ///
    /// # Errors
    ///
    /// This method fails whenever supplied `Url` cannot be parsed.
    pub fn delete<U: IntoUrl>(url: U) -> RequestBuilder {
        RequestBuilder::new(Method::DELETE, url)
    }

    /// Convenience method to make a `HEAD` request to a URL.
    ///
    /// # Errors
    ///
    /// This method fails whenever supplied `Url` cannot be parsed.
    pub fn head<U: IntoUrl>(url: U) -> RequestBuilder {
        RequestBuilder::new(Method::HEAD, url)
    }

    /// Add a `Header` to this Request.
    ///
    /// ```rust
    /// use reqwest::header::USER_AGENT;
    ///
    /// # use reqwest::Error;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// let client = reqwest::Client::new();
    /// let res = reqwest::RequestBuilder::get("https://www.rust-lang.org")
    ///     .header(USER_AGENT, "foo")
    ///     .send(&client)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn header<K, V>(self, key: K, value: V) -> RequestBuilder
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        self.header_sensitive(key, value, false)
    }

    /// Add a `Header` to this Request with ability to define if header_value is sensitive.
    fn header_sensitive<K, V>(mut self, key: K, value: V, sensitive: bool) -> RequestBuilder
    where
        HeaderName: TryFrom<K>,
        <HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        HeaderValue: TryFrom<V>,
        <HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        let mut error = None;
        if let Ok(ref mut req) = self.request {
            match <HeaderName as TryFrom<K>>::try_from(key) {
                Ok(key) => match <HeaderValue as TryFrom<V>>::try_from(value) {
                    Ok(mut value) => {
                        value.set_sensitive(sensitive);
                        req.headers_mut().append(key, value);
                    }
                    Err(e) => error = Some(crate::error::builder(e.into())),
                },
                Err(e) => error = Some(crate::error::builder(e.into())),
            };
        }
        if let Some(err) = error {
            self.request = Err(err);
        }
        self
    }

    /// Add a set of Headers to the existing ones on this Request.
    ///
    /// The headers will be merged in to any already set.
    ///
    /// ```rust
    /// use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT, CONTENT_TYPE};
    /// # use std::fs;
    ///
    /// fn construct_headers() -> HeaderMap {
    ///     let mut headers = HeaderMap::new();
    ///     headers.insert(USER_AGENT, HeaderValue::from_static("reqwest"));
    ///     headers.insert(CONTENT_TYPE, HeaderValue::from_static("image/png"));
    ///     headers
    /// }
    ///
    /// # use reqwest::Error;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// let file = fs::File::open("much_beauty.png")?;
    /// let client = reqwest::Client::new();
    /// let res = reqwest::RequestBuilder::post("http://httpbin.org/post")
    ///     .headers(construct_headers())
    ///     .body(file)
    ///     .send(&client)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn headers(mut self, headers: crate::header::HeaderMap) -> RequestBuilder {
        if let Ok(ref mut req) = self.request {
            crate::util::replace_headers(req.headers_mut(), headers);
        }
        self
    }

    /// Enable HTTP basic authentication.
    ///
    /// ```rust
    /// # use reqwest::Error;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// let client = reqwest::Client::new();
    /// let resp = reqwest::RequestBuilder::delete("http://httpbin.org/delete")
    ///     .basic_auth("admin", Some("good password"))
    ///     .send(&client)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn basic_auth<U, P>(self, username: U, password: Option<P>) -> RequestBuilder
    where
        U: std::fmt::Display,
        P: std::fmt::Display,
    {
        let auth = match password {
            Some(password) => format!("{}:{}", username, password),
            None => format!("{}:", username),
        };
        let header_value = format!("Basic {}", encode(&auth));
        self.header_sensitive(crate::header::AUTHORIZATION, &*header_value, true)
    }

    /// Enable HTTP bearer authentication.
    ///
    /// ```rust
    /// # use reqwest::Error;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// let client = reqwest::Client::new();
    /// let resp = reqwest::RequestBuilder::delete("http://httpbin.org/delete")
    ///     .bearer_auth("token")
    ///     .send(&client)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn bearer_auth<T>(self, token: T) -> RequestBuilder
    where
        T: std::fmt::Display,
    {
        let header_value = format!("Bearer {}", token);
        self.header_sensitive(crate::header::AUTHORIZATION, header_value, true)
    }

    /// Set the request body.
    ///
    /// # Examples
    ///
    /// Using a string:
    ///
    /// ```rust
    /// # use reqwest::Error;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// let client = reqwest::Client::new();
    /// let res = reqwest::RequestBuilder::post("http://httpbin.org/post")
    ///     .body("from a &str!")
    ///     .send(&client)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Using a `File`:
    ///
    /// ```rust
    /// # use reqwest::Error;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// let file = std::fs::File::open("from_a_file.txt")?;
    /// let client = reqwest::Client::new();
    /// let res = reqwest::RequestBuilder::post("http://httpbin.org/post")
    ///     .body(file)
    ///     .send(&client)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Using arbitrary bytes:
    ///
    /// ```rust
    /// # use std::fs;
    /// # use reqwest::Error;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// // from bytes!
    /// let bytes: Vec<u8> = vec![1, 10, 100];
    /// let client = reqwest::Client::new();
    /// let res = reqwest::RequestBuilder::post("http://httpbin.org/post")
    ///     .body(bytes)
    ///     .send(&client)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn body<T: Into<Body>>(mut self, body: T) -> RequestBuilder {
        if let Ok(ref mut req) = self.request {
            *req.body_mut() = Some(body.into());
        }
        self
    }

    /// Enables a request timeout.
    ///
    /// The timeout is applied from the when the request starts connecting
    /// until the response body has finished. It affects only this request
    /// and overrides the timeout configured using `ClientBuilder::timeout()`.
    pub fn timeout(mut self, timeout: Duration) -> RequestBuilder {
        if let Ok(ref mut req) = self.request {
            *req.timeout_mut() = Some(timeout);
        }
        self
    }

    /// Modify the query string of the URL.
    ///
    /// Modifies the URL of this request, adding the parameters provided.
    /// This method appends and does not overwrite. This means that it can
    /// be called multiple times and that existing query parameters are not
    /// overwritten if the same key is used. The key will simply show up
    /// twice in the query string.
    /// Calling `.query(&[("foo", "a"), ("foo", "b")])` gives `"foo=a&foo=b"`.
    ///
    /// ```rust
    /// # use reqwest::Error;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// let client = reqwest::Client::new();
    /// let res = reqwest::RequestBuilder::get("http://httpbin.org")
    ///     .query(&[("lang", "rust")])
    ///     .send(&client)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Note
    /// This method does not support serializing a single key-value
    /// pair. Instead of using `.query(("key", "val"))`, use a sequence, such
    /// as `.query(&[("key", "val")])`. It's also possible to serialize structs
    /// and maps into a key-value pair.
    ///
    /// # Errors
    /// This method will fail if the object you provide cannot be serialized
    /// into a query string.
    pub fn query<T: Serialize + ?Sized>(mut self, query: &T) -> RequestBuilder {
        let mut error = None;
        if let Ok(ref mut req) = self.request {
            let url = req.url_mut();
            let mut pairs = url.query_pairs_mut();
            let serializer = serde_urlencoded::Serializer::new(&mut pairs);

            if let Err(err) = query.serialize(serializer) {
                error = Some(crate::error::builder(err));
            }
        }
        if let Ok(ref mut req) = self.request {
            if let Some("") = req.url().query() {
                req.url_mut().set_query(None);
            }
        }
        if let Some(err) = error {
            self.request = Err(err);
        }
        self
    }

    /// Send a form body.
    ///
    /// Sets the body to the url encoded serialization of the passed value,
    /// and also sets the `Content-Type: application/x-www-form-urlencoded`
    /// header.
    ///
    /// ```rust
    /// # use reqwest::Error;
    /// # use std::collections::HashMap;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// let mut params = HashMap::new();
    /// params.insert("lang", "rust");
    ///
    /// let client = reqwest::Client::new();
    /// let res = reqwest::RequestBuilder::post("http://httpbin.org")
    ///     .form(&params)
    ///     .send(&client)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// This method fails if the passed value cannot be serialized into
    /// url encoded format
    pub fn form<T: Serialize + ?Sized>(mut self, form: &T) -> RequestBuilder {
        let mut error = None;
        if let Ok(ref mut req) = self.request {
            match serde_urlencoded::to_string(form) {
                Ok(body) => {
                    req.headers_mut().insert(
                        CONTENT_TYPE,
                        HeaderValue::from_static("application/x-www-form-urlencoded"),
                    );
                    *req.body_mut() = Some(body.into());
                }
                Err(err) => error = Some(crate::error::builder(err)),
            }
        }
        if let Some(err) = error {
            self.request = Err(err);
        }
        self
    }

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

    /// Send a JSON body.
    ///
    /// Sets the body to the JSON serialization of the passed value, and
    /// also sets the `Content-Type: application/json` header.
    ///
    /// # Optional
    ///
    /// This requires the optional `json` feature enabled.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use reqwest::Error;
    /// # use std::collections::HashMap;
    /// #
    /// # async fn run() -> Result<(), Error> {
    /// let mut map = HashMap::new();
    /// map.insert("lang", "rust");
    ///
    /// let client = reqwest::Client::new();
    /// let res = reqwest::RequestBuilder::post("http://httpbin.org")
    ///     .json(&map)
    ///     .send(&client)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Serialization can fail if `T`'s implementation of `Serialize` decides to
    /// fail, or if `T` contains a map with non-string keys.
    #[cfg(feature = "json")]
    pub fn json<T: Serialize + ?Sized>(mut self, json: &T) -> RequestBuilder {
        let mut error = None;
        if let Ok(ref mut req) = self.request {
            match serde_json::to_vec(json) {
                Ok(body) => {
                    req.headers_mut()
                        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    *req.body_mut() = Some(body.into());
                }
                Err(err) => error = Some(crate::error::builder(err)),
            }
        }
        if let Some(err) = error {
            self.request = Err(err);
        }
        self
    }

    /// Disable CORS on fetching the request.
    ///
    /// # WASM
    ///
    /// This option is only effective with WebAssembly target.
    ///
    /// The [request mode][mdn] will be set to 'no-cors'.
    ///
    /// [mdn]: https://developer.mozilla.org/en-US/docs/Web/API/Request/mode
    pub fn fetch_mode_no_cors(mut self) -> RequestBuilder {
        if let Ok(ref mut req) = self.request {
            req.cors = false;
        }
        self
    }

    /// Build a `Request`, which can be inspected, modified and executed with
    /// `Client::execute()`.
    pub fn build(self) -> crate::Result<Request> {
        self.request
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
    pub fn send(
        self,
        client: &crate::async_impl::Client,
    ) -> impl Future<Output = Result<Response, crate::Error>> {
        match self.request {
            Ok(req) => WrapFuture::new(client.execute(req)),
            Err(err) => WrapFuture::new(ready(Err(err))),
        }
    }

    /// TODO: This is a temporary measure until the clients can be genericized in the next commit
    pub fn temp_send_blocking(
        self,
        client: &crate::blocking::Client,
    ) -> Result<crate::blocking::Response, crate::Error> {
        match self.request {
            Ok(req) => client.execute(req),
            Err(err) => Err(err),
        }
    }
}

impl TryClone for RequestBuilder {
    type Error = crate::error::Error;

    /// Attempts to clone the `RequestBuilder`.
    ///
    /// Err is returned if a body is which can not be cloned. This can be because the body is a
    /// stream.
    ///
    /// # Examples
    ///
    /// With a static body
    ///
    /// ```rust
    /// # use fallible::TryClone;
    /// # fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let builder = reqwest::RequestBuilder::post("http://httpbin.org/post")
    ///     .body("from a &str!");
    /// let clone = builder.try_clone();
    /// assert!(clone.is_ok());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Without a body
    ///
    /// ```rust
    /// # use fallible::TryClone;
    /// # fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let builder = reqwest::RequestBuilder::get("http://httpbin.org/get");
    /// let clone = builder.try_clone();
    /// assert!(clone.is_ok());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// With a non-clonable body
    ///
    /// ```rust
    /// # use fallible::TryClone;
    /// # fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let builder = reqwest::RequestBuilder::get("http://httpbin.org/get")
    ///     .body(reqwest::Body::from_reader(std::io::empty(), None));
    /// let clone = builder.try_clone();
    /// assert!(clone.is_err());
    /// # Ok(())
    /// # }
    /// ```
    fn try_clone(&self) -> Result<Self, Self::Error> {
        match self.request {
            Ok(ref req) => Ok(RequestBuilder {
                request: Ok((*req).try_clone()?),
            }),
            Err(ref err) => Err(err.clone()),
        }
    }
}

impl std::fmt::Debug for RequestBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut builder = f.debug_struct("RequestBuilder");
        match self.request {
            Ok(ref req) => fmt_request_fields(&mut builder, req).finish(),
            Err(ref err) => builder.field("error", err).finish(),
        }
    }
}

fn fmt_request_fields<'a, 'b>(
    f: &'a mut std::fmt::DebugStruct<'a, 'b>,
    req: &Request,
) -> &'a mut std::fmt::DebugStruct<'a, 'b> {
    f.field("method", &req.method)
        .field("url", &req.url)
        .field("headers", &req.headers)
}

/// Check the request URL for a "username:password" type authority, and if
/// found, remove it from the URL and return it.
pub(crate) fn extract_authority(url: &mut Url) -> Option<(String, Option<String>)> {
    use percent_encoding::percent_decode;

    if url.has_authority() {
        let username: String = percent_decode(url.username().as_bytes())
            .decode_utf8()
            .ok()?
            .into();
        let password = url.password().and_then(|pass| {
            percent_decode(pass.as_bytes())
                .decode_utf8()
                .ok()
                .map(String::from)
        });
        if !username.is_empty() || password.is_some() {
            url.set_username("")
                .expect("has_authority means set_username shouldn't fail");
            url.set_password(None)
                .expect("has_authority means set_password shouldn't fail");
            return Some((username, password));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::convert::TryFrom;

    use http::Request as HttpRequest;
    use serde::Serialize;
    #[cfg(feature = "json")]
    use serde_json;
    use serde_urlencoded;

    use crate::core::body;
    use crate::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, HOST};
    use crate::Method;
    use crate::{Request, RequestBuilder};

    #[test]
    fn basic_get_request() {
        let some_url = "https://google.com/";
        let r = RequestBuilder::get(some_url).build().unwrap();

        assert_eq!(r.method(), &Method::GET);
        assert_eq!(r.url().as_str(), some_url);
    }

    #[test]
    fn basic_head_request() {
        let some_url = "https://google.com/";
        let r = RequestBuilder::head(some_url).build().unwrap();

        assert_eq!(r.method(), &Method::HEAD);
        assert_eq!(r.url().as_str(), some_url);
    }

    #[test]
    fn basic_post_request() {
        let some_url = "https://google.com/";
        let r = RequestBuilder::post(some_url).build().unwrap();

        assert_eq!(r.method(), &Method::POST);
        assert_eq!(r.url().as_str(), some_url);
    }

    #[test]
    fn basic_put_request() {
        let some_url = "https://google.com/";
        let r = RequestBuilder::put(some_url).build().unwrap();

        assert_eq!(r.method(), &Method::PUT);
        assert_eq!(r.url().as_str(), some_url);
    }

    #[test]
    fn basic_patch_request() {
        let some_url = "https://google.com/";
        let r = RequestBuilder::patch(some_url).build().unwrap();

        assert_eq!(r.method(), &Method::PATCH);
        assert_eq!(r.url().as_str(), some_url);
    }

    #[test]
    fn basic_delete_request() {
        let some_url = "https://google.com/";
        let r = RequestBuilder::delete(some_url).build().unwrap();

        assert_eq!(r.method(), &Method::DELETE);
        assert_eq!(r.url().as_str(), some_url);
    }

    #[test]
    fn add_header() {
        let some_url = "https://google.com/";
        let header = HeaderValue::from_static("google.com");

        // Add a copy of the header to the request builder
        let r = RequestBuilder::post(some_url)
            .header(HOST, header.clone())
            .build()
            .unwrap();

        // then check it was actually added
        assert_eq!(r.headers().get(HOST), Some(&header));
    }

    #[test]
    fn add_headers() {
        let some_url = "https://google.com/";
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("google.com"));

        // Add a copy of the headers to the request builder
        let r = RequestBuilder::post(some_url)
            .headers(headers.clone())
            .build()
            .unwrap();

        // then make sure they were added correctly
        assert_eq!(r.headers(), &headers);
    }

    #[test]
    fn add_headers_multi() {
        let some_url = "https://google.com/";
        let mut headers = HeaderMap::new();
        headers.append(ACCEPT, HeaderValue::from_static("application/json"));
        headers.append(ACCEPT, HeaderValue::from_static("application/xml"));

        // Add a copy of the headers to the request builder
        let r = RequestBuilder::post(some_url)
            .headers(headers.clone())
            .build()
            .unwrap();

        // then make sure they were added correctly
        assert_eq!(r.headers(), &headers);
        let mut all_values = r.headers().get_all(ACCEPT).iter();
        assert_eq!(all_values.next().unwrap(), &"application/json");
        assert_eq!(all_values.next().unwrap(), &"application/xml");
        assert_eq!(all_values.next(), None);
    }

    #[test]
    fn add_body() {
        let some_url = "https://google.com/";
        let body = "Some interesting content";
        let mut r = RequestBuilder::post(some_url).body(body).build().unwrap();

        let buf = body::read_to_string(r.body_mut().take().unwrap()).unwrap();

        assert_eq!(buf, body);
    }

    #[test]
    fn add_query_append() {
        let some_url = "https://google.com/";
        let r = RequestBuilder::get(some_url)
            .query(&[("foo", "bar")])
            .query(&[("qux", 3)])
            .build()
            .unwrap();
        assert_eq!(r.url().query(), Some("foo=bar&qux=3"));
    }

    #[test]
    fn add_query_append_same() {
        let some_url = "https://google.com/";
        let r = RequestBuilder::get(some_url)
            .query(&[("foo", "a"), ("foo", "b")])
            .build()
            .unwrap();
        assert_eq!(r.url().query(), Some("foo=a&foo=b"));
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
        let r = RequestBuilder::get(some_url)
            .query(&params)
            .build()
            .unwrap();
        assert_eq!(r.url().query(), Some("foo=bar&qux=3"));
    }

    #[test]
    fn add_query_map() {
        let some_url = "https://google.com/";
        let mut params = BTreeMap::new();
        params.insert("foo", "bar");
        params.insert("qux", "three");

        let r = RequestBuilder::get(some_url)
            .query(&params)
            .build()
            .unwrap();
        assert_eq!(r.url().query(), Some("foo=bar&qux=three"));
    }

    #[test]
    fn add_form() {
        let some_url = "https://google.com/";
        let mut form_data = HashMap::new();
        form_data.insert("foo", "bar");

        let mut r = RequestBuilder::post(some_url)
            .form(&form_data)
            .build()
            .unwrap();

        // Make sure the content type was set
        assert_eq!(
            r.headers().get(CONTENT_TYPE).unwrap(),
            &"application/x-www-form-urlencoded"
        );

        let buf = body::read_to_string(r.body_mut().take().unwrap()).unwrap();

        let body_should_be = serde_urlencoded::to_string(&form_data).unwrap();
        assert_eq!(buf, body_should_be);
    }

    #[test]
    #[cfg(feature = "json")]
    fn add_json() {
        let some_url = "https://google.com/";
        let mut json_data = HashMap::new();
        json_data.insert("foo", "bar");

        let mut r = RequestBuilder::post(some_url)
            .json(&json_data)
            .build()
            .unwrap();

        // Make sure the content type was set
        assert_eq!(r.headers().get(CONTENT_TYPE).unwrap(), &"application/json");

        let buf = body::read_to_string(r.body_mut().take().unwrap()).unwrap();

        let body_should_be = serde_json::to_string(&json_data).unwrap();
        assert_eq!(buf, body_should_be);
    }

    #[test]
    #[cfg(feature = "json")]
    fn add_json_fail() {
        use serde::ser::Error as _;
        use serde::{Serialize, Serializer};
        use std::error::Error as _;

        struct MyStruct;
        impl Serialize for MyStruct {
            fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                Err(S::Error::custom("nope"))
            }
        }

        let some_url = "https://google.com/";
        let json_data = MyStruct;

        let err = RequestBuilder::post(some_url)
            .json(&json_data)
            .build()
            .unwrap_err();
        assert!(err.is_builder());
        assert!(err.source().unwrap().is::<serde_json::Error>());
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
    fn convert_url_authority_into_basic_auth() {
        let some_url = "https://Aladdin:open sesame@localhost/";

        let req = RequestBuilder::get(some_url)
            .build()
            .expect("request build");

        assert_eq!(req.url().as_str(), "https://localhost/");
        assert_eq!(
            req.headers()["authorization"],
            "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
    }

    #[test]
    fn test_basic_auth_sensitive_header() {
        let some_url = "https://localhost/";

        let req = RequestBuilder::get(some_url)
            .basic_auth("Aladdin", Some("open sesame"))
            .build()
            .expect("request build");

        assert_eq!(req.url().as_str(), "https://localhost/");
        assert_eq!(
            req.headers()["authorization"],
            "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
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
        let http_request = HttpRequest::builder()
            .method("GET")
            .uri("http://localhost/")
            .header("User-Agent", "my-awesome-agent/1.0")
            .body("test test test")
            .unwrap();
        let mut req: Request = Request::try_from(http_request).unwrap();
        assert!(req.body().is_some());
        assert_eq!(
            &body::read_to_string(req.body.take().unwrap()).unwrap()[..],
            &"test test test"[..]
        );
        let headers = req.headers();
        assert_eq!(headers.get("User-Agent").unwrap(), "my-awesome-agent/1.0");
        assert_eq!(req.method(), Method::GET);
        assert_eq!(req.url().as_str(), "http://localhost/");
    }
}
