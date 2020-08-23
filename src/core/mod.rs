use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Future;

pub mod body;
pub mod multipart;
pub mod request;

// Hyper uses a runtime and futures must be Send
if_hyper! {
    pub struct WrapFuture<T>(Pin<Box<dyn Future<Output = T> + Send>>);

    impl<T> WrapFuture<T> {
        pub(crate) fn new<F: Future<Output = T> + Send + 'static>(future: F) -> WrapFuture<T> {
            WrapFuture(Box::pin(future))
        }
    }


}
// WASM is single threaded and futures from JS are not Send
if_wasm! {
    pub struct WrapFuture<T>(Pin<Box<dyn Future<Output = T>>>);

    impl<T> WrapFuture<T> {
        pub(crate) fn new<F: Future<Output = T> + 'static>(future: F) -> WrapFuture<T> {
            WrapFuture(Box::pin(future))
        }
    }
}
impl<T> Future for WrapFuture<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.as_mut().poll(cx)
    }
}
