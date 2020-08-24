use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Future;

pub mod body;
pub mod multipart;
pub mod request;

pub trait Client {
    type Response;

    fn send(&self, request: crate::Result<request::Request>) -> Self::Response;
}

pub struct SendFuture<T>(TryFuture<T, crate::Error>);

impl<T> SendFuture<T> {
    // Hyper uses a runtime and futures must be Send
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn future<F: Future<Output = Result<T, crate::Error>> + Send + 'static>(
        future: F,
    ) -> SendFuture<T> {
        SendFuture(TryFuture::Ok(Box::pin(future)))
    }

    // WASM is single threaded and futures from JS are not Send
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn future<F: Future<Output = Result<T, crate::Error>> + 'static>(
        future: F,
    ) -> SendFuture<T> {
        SendFuture(TryFuture::Ok(Box::pin(future)))
    }

    pub(crate) fn error(error: crate::Error) -> SendFuture<T> {
        SendFuture(TryFuture::Err(Some(error)))
    }
}

impl<T> Future for SendFuture<T> {
    type Output = Result<T, crate::Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe { self.map_unchecked_mut(|sf| &mut sf.0) }.poll(cx)
    }
}

impl<T> std::fmt::Debug for SendFuture<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SendFuture")
    }
}

// Hyper uses a runtime and futures must be Send
#[cfg(not(target_arch = "wasm32"))]
type FutureSendIfNotWasm<T> = dyn Future<Output = T> + Send;
// WASM is single threaded and futures from JS are not Send
#[cfg(target_arch = "wasm32")]
type FutureSendIfNotWasm<T> = dyn Future<Output = T>;

enum TryFuture<Value, Error: Unpin> {
    Ok(Pin<Box<FutureSendIfNotWasm<Result<Value, Error>>>>),
    Err(Option<Error>),
}

impl<Value, Error: Unpin> Future for TryFuture<Value, Error> {
    type Output = Result<Value, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.get_mut() {
            TryFuture::Ok(inner_future) => inner_future.as_mut().poll(cx),
            TryFuture::Err(error) => {
                Poll::Ready(Err(error.take().expect("Polled error multiple times")))
            }
        }
    }
}
