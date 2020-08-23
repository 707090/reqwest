use std::future::Future;

use futures_util::TryStreamExt;
use js_sys::Promise;
use url::Url;
use wasm_bindgen::prelude::{UnwrapThrowExt as _, wasm_bindgen};
use wasm_streams::ReadableStream;

use crate::Request;

use super::Response;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = fetch)]
    fn fetch_with_request(input: &web_sys::Request) -> Promise;
}

/// dox
#[derive(Clone, Debug)]
pub struct Client(());

/// dox
#[derive(Debug)]
pub struct ClientBuilder(());

impl Client {
    /// dox
    pub fn new() -> Self {
        Client::builder().build().unwrap_throw()
    }

    /// dox
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Executes a `Request`.
    ///
    /// A `Request` can be built manually with `Request::new()` or obtained
    /// from a RequestBuilder with `RequestBuilder::build()`.
    ///
    /// You should prefer to use the `RequestBuilder` and
    /// `RequestBuilder::send()`.
    ///
    /// # Errors
    ///
    /// This method fails if there was an error while sending request,
    /// redirect loop was detected or redirect limit was exhausted.
    pub fn execute(
        &self,
        request: Request,
    ) -> impl Future<Output = Result<Response, crate::Error>> {
        self.execute_request(request)
    }

    pub(super) fn execute_request(
        &self,
        req: Request,
    ) -> impl Future<Output = crate::Result<Response>> {
        fetch(req)
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

async fn fetch(req: Request) -> crate::Result<Response> {
    let Request {
        method,
        url,
        headers,
        body,
        timeout: _timeout,
        cors,
    } = req;

    // Build the js Request
    let mut init = web_sys::RequestInit::new();
    init.method(method.as_str());

    let js_headers = web_sys::Headers::new()
        .map_err(crate::error::wasm)
        .map_err(crate::error::builder)?;

    for (name, value) in &headers {
        js_headers
            .append(
                name.as_str(),
                value.to_str().map_err(crate::error::builder)?,
            )
            .map_err(crate::error::wasm)
            .map_err(crate::error::builder)?;
    }
    init.headers(&js_headers.into());

    // When req.cors is true, do nothing because the default mode is 'cors'
    if !cors {
        init.mode(web_sys::RequestMode::NoCors);
    }

    if let Some(body) = body {
        init.body(Some(
            ReadableStream::from_stream(
                body.map_ok(|bytes| unsafe { js_sys::Uint8Array::view(bytes.as_ref()) }.slice(0, bytes.len() as u32).into())
                    .map_err(|error| format!("{:?}", error).into())).into_raw().as_ref()
        ));
    }

    let js_req = web_sys::Request::new_with_str_and_init(url.as_str(), &init)
        .map_err(crate::error::wasm)
        .map_err(crate::error::builder)?;

    // Await the fetch() promise
    let p = fetch_with_request(&js_req);
    let js_resp = super::promise::<web_sys::Response>(p)
        .await
        .map_err(crate::error::request)?;

    // Convert from the js Response
    let mut resp = http::Response::builder()
        .status(js_resp.status());

    let url = Url::parse(&js_resp.url()).expect_throw("url parse");

    let js_headers = js_resp.headers();
    let js_iter = js_sys::try_iter(&js_headers)
        .expect_throw("headers try_iter")
        .expect_throw("headers have an iterator");

    for item in js_iter {
        let item = item.expect_throw("headers iterator doesn't throw");
        let v: Vec<String> = item.into_serde().expect_throw("headers into_serde");
        resp = resp.header(
            v.get(0).expect_throw("headers name"),
            v.get(1).expect_throw("headers value"),
        );
    }

    resp.body(js_resp)
        .map(|resp| Response::new(resp, url))
        .map_err(crate::error::request)
}

// ===== impl ClientBuilder =====

impl ClientBuilder {
    /// dox
    pub fn new() -> Self {
        ClientBuilder(())
    }

    /// dox
    pub fn build(self) -> Result<Client, crate::Error> {
        Ok(Client(()))
    }
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}
