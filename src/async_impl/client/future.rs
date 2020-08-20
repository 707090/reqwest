use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http::header::{
    CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, HeaderMap,
    HeaderValue, LOCATION, REFERER, TRANSFER_ENCODING,
};
use hyper::client::ResponseFuture;
use log::debug;
use tokio::time::Delay;

use crate::{Method, StatusCode, Url, Request};
use crate::async_impl::Body;
use crate::async_impl::response::Response;
#[cfg(feature = "cookies")]
use crate::cookie;
use crate::into_url::{expect_uri, try_uri};
use crate::redirect::{self, remove_sensitive_headers};

use super::add_cookie_header;
use super::ClientRef;

pub struct WrapFuture<T>(Pin<Box<dyn Future<Output=T> + Send>>);

impl<T> WrapFuture<T> {
    pub(crate) fn new<F: Future<Output=T> + Send + 'static>(future: F) -> WrapFuture<T> {
        WrapFuture(Box::pin(future))
    }
}

impl<T> Future for WrapFuture<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.as_mut().poll(cx)
    }
}

pub(super) struct RequestFuture {
    pub(super) request: Request,
    pub(super) body: Option<Option<Bytes>>,
    pub(super) timeout: Option<Delay>,

    pub(super) client: Arc<ClientRef>,
    pub(super) redirect_chain: Vec<Url>,
    pub(super) in_flight: ResponseFuture,
}

impl RequestFuture {
    fn headers(self: Pin<&mut Self>) -> &mut HeaderMap {
        unsafe { &mut Pin::get_unchecked_mut(self).request.headers }
    }

    fn timeout(self: Pin<&mut Self>) -> Pin<&mut Option<Delay>> {
        unsafe { Pin::map_unchecked_mut(self, |x| &mut x.timeout) }
    }

    fn redirect_chain(self: Pin<&mut Self>) -> &mut Vec<Url> {
        unsafe { &mut Pin::get_unchecked_mut(self).redirect_chain }
    }

    fn in_flight(self: Pin<&mut Self>) -> Pin<&mut ResponseFuture> {
        unsafe { Pin::map_unchecked_mut(self, |x| &mut x.in_flight) }
    }
}

impl Future for RequestFuture {
    type Output = Result<Response, crate::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(delay) = self.as_mut().timeout().as_mut().as_pin_mut() {
            if let Poll::Ready(()) = delay.poll(cx) {
                return Poll::Ready(Err(
                    crate::error::request(crate::error::TimedOut).with_url(self.request.url().clone())
                ));
            }
        }

        loop {
            let res = match self.as_mut().in_flight().as_mut().poll(cx) {
                Poll::Ready(Ok(res)) => res,
                Poll::Ready(Err(e)) => {
                    return Poll::Ready(Err(crate::error::request(e).with_url(self.request.url().clone())));
                }
                Poll::Pending => return Poll::Pending,
            };

            #[cfg(feature = "cookies")]
                {
                    if let Some(store_wrapper) = self.client.cookie_store.as_ref() {
                        let mut cookies = cookie::extract_response_cookies(&res.headers())
                            .filter_map(|res| res.ok())
                            .map(|cookie| cookie.into_inner().into_owned())
                            .peekable();
                        if cookies.peek().is_some() {
                            let mut store = store_wrapper.write().unwrap();
                            store.0.store_response_cookies(cookies, self.request.url());
                        }
                    }
                }

            let should_redirect = match res.status() {
                StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER => {
                    self.body = None;
                    for header in &[
                        TRANSFER_ENCODING,
                        CONTENT_ENCODING,
                        CONTENT_TYPE,
                        CONTENT_LENGTH,
                    ] {
                        self.request.headers_mut().remove(header);
                    }

                    match *self.request.method() {
                        Method::GET | Method::HEAD => {}
                        _ => {
                            *self.request.method_mut() = Method::GET;
                        }
                    }
                    true
                }
                StatusCode::TEMPORARY_REDIRECT | StatusCode::PERMANENT_REDIRECT => {
                    match self.body {
                        Some(Some(_)) | None => true,
                        Some(None) => false,
                    }
                }
                _ => false,
            };
            if should_redirect {
                let loc = res.headers().get(LOCATION).and_then(|val| {
                    // Some sites may send a utf-8 Location header,
                    // even though we're supposed to treat those bytes
                    // as opaque, we'll check specifically for utf8.
                    let loc = self.request.url().join(std::str::from_utf8(val.as_bytes()).ok()?)
                        .ok()
                        // Check that the `url` is also a valid `http::Uri`.
                        //
                        // If not, just log it and skip the redirect.
                        .filter(|url| try_uri(&url).is_some());

                    if loc.is_none() {
                        debug!("Location header had invalid URI: {:?}", val);
                    }
                    loc
                });
                if let Some(loc) = loc {
                    if self.client.referer {
                        if let Some(referer) = make_referer(&loc, self.request.url()) {
                            self.request.headers_mut().insert(REFERER, referer);
                        }
                    }
                    let url = self.request.url().clone();
                    self.as_mut().redirect_chain().push(url);
                    let action = self
                        .client
                        .redirect_policy
                        .check(res.status(), &loc, &self.redirect_chain);

                    match action {
                        redirect::ActionKind::Follow => {
                            debug!("redirecting '{}' to '{}'", self.request.url(), loc);
                            *self.request.url_mut() = loc;

                            let mut headers =
                                std::mem::replace(self.as_mut().headers(), HeaderMap::new());

                            remove_sensitive_headers(&mut headers, self.request.url(), &self.redirect_chain);
                            let uri = expect_uri(self.request.url());
                            let body = match self.body {
                                Some(Some(ref body)) => Body::reusable(body.clone()),
                                _ => Body::empty(),
                            };
                            let mut req = hyper::Request::builder()
                                .method(self.request.method().clone())
                                .uri(uri.clone())
                                .body(body.into_stream())
                                .expect("valid request parts");

                            // Add cookies from the cookie store.
                            #[cfg(feature = "cookies")]
                                {
                                    if let Some(cookie_store_wrapper) =
                                    self.client.cookie_store.as_ref()
                                    {
                                        let cookie_store = cookie_store_wrapper.read().unwrap();
                                        add_cookie_header(&mut headers, &cookie_store, self.request.url());
                                    }
                                }

                            *req.headers_mut() = headers.clone();
                            std::mem::swap(self.as_mut().headers(), &mut headers);
                            *self.as_mut().in_flight().get_mut() = self.client.hyper.request(req);
                            continue;
                        }
                        redirect::ActionKind::Stop => {
                            debug!("redirect policy disallowed redirection to '{}'", loc);
                        }
                        redirect::ActionKind::Error(err) => {
                            return Poll::Ready(Err(crate::error::redirect(err, self.request.url().clone())));
                        }
                    }
                }
            }

            debug!("response '{}' for {}", res.status(), self.request.url());
            let res = Response::new(
                res,
                self.request.url().clone(),
                self.client.accepts,
                self.timeout.take(),
            );
            return Poll::Ready(Ok(res));
        }
    }
}

impl std::fmt::Debug for RequestFuture {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("RequestFuture")
            .field("method", self.request.method())
            .field("url", self.request.url())
            .finish()
    }
}

fn make_referer(next: &Url, previous: &Url) -> Option<HeaderValue> {
    if next.scheme() == "http" && previous.scheme() == "https" {
        return None;
    }

    let mut referer = previous.clone();
    let _ = referer.set_username("");
    let _ = referer.set_password(None);
    referer.set_fragment(None);
    referer.as_str().parse().ok()
}
