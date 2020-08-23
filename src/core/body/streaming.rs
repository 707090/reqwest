use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use fallible::TryClone;
use futures_core::Stream;
use futures_util::TryStreamExt;
use http_body::Body as HttpBody;

use super::BodyClone;
use futures_timer::Delay;

pub struct StreamingBody {
    body: Pin<
        Box<
            dyn HttpBody<Data = Bytes, Error = Box<dyn std::error::Error + Send + Sync>>
                + Send
                + Sync,
        >,
    >,
    timeout: Option<Delay>,
}

impl StreamingBody {
    pub fn from_stream<S>(stream: S) -> StreamingBody
    where
        S: futures_core::stream::TryStream + Send + Sync + 'static,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        Bytes: From<S::Ok>,
    {
        let body = Box::pin(WrapStream(stream.map_ok(Bytes::from).map_err(Into::into)));
        StreamingBody {
            body,
            timeout: None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_hyper(body: hyper::Body, timeout: Option<Delay>) -> StreamingBody {
        StreamingBody {
            body: Box::pin(WrapHyper(body)),
            timeout,
        }
    }
}

impl TryClone for StreamingBody {
    type Error = crate::error::Error;

    fn try_clone(&self) -> Result<Self, Self::Error> {
        Err(crate::error::builder(
            crate::error::CannotCloneStreamingBodyError,
        ))
    }
}

impl HttpBody for StreamingBody {
    type Data = Bytes;
    type Error = crate::Error;

    fn poll_data(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        let opt_try_chunk = {
            if let Some(ref mut timeout) = self.timeout {
                if let Poll::Ready(_) = Pin::new(timeout).poll(cx) {
                    return Poll::Ready(Some(Err(crate::error::body(crate::error::TimedOut))));
                }
            }
            futures_core::ready!(Pin::new(&mut self.body).poll_data(cx))
                .map(|opt_chunk| opt_chunk.map(Into::into).map_err(crate::error::body))
        };

        Poll::Ready(opt_try_chunk)
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        _cx: &mut Context,
    ) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None))
    }

    fn is_end_stream(&self) -> bool {
        self.body.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.body.size_hint()
    }
}

impl BodyClone for StreamingBody {
    fn try_clone_body(&self) -> Option<Box<dyn BodyClone<Data = Bytes, Error = crate::Error>>> {
        None
    }
}

impl std::fmt::Debug for StreamingBody {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("StreamingBody").finish()
    }
}

struct WrapStream<S>(S);

impl<S, D, E> HttpBody for WrapStream<S>
where
    S: Stream<Item = Result<D, E>>,
    D: Into<Bytes>,
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Data = Bytes;
    type Error = E;

    fn poll_data(
        self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        // safe pin projection
        let item =
            futures_core::ready!(
                unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().0) }.poll_next(cx)?
            );

        Poll::Ready(item.map(|val| Ok(val.into())))
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        _cx: &mut Context,
    ) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None))
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct WrapHyper(hyper::Body);

#[cfg(not(target_arch = "wasm32"))]
impl HttpBody for WrapHyper {
    type Data = Bytes;
    type Error = Box<dyn std::error::Error + Send + Sync>;

    fn poll_data(
        mut self: Pin<&mut Self>,
        cx: &mut Context,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        // safe pin projection
        Pin::new(&mut self.0)
            .poll_data(cx)
            .map(|opt| opt.map(|res| res.map_err(Into::into)))
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        _cx: &mut Context,
    ) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None))
    }

    fn is_end_stream(&self) -> bool {
        self.0.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        HttpBody::size_hint(&self.0)
    }
}
