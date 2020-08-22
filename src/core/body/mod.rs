use std::pin::Pin;
use std::task::{Context, Poll};

use ::bytes::Bytes;
use fallible::TryClone;
use futures_core::Stream;
use http::{HeaderMap, HeaderValue};
use http_body::Body as HttpBody;
use tokio::time::Delay;

use streaming::StreamingBody;

use self::bytes::BytesBody;

mod bytes;
mod streaming;

pub trait BodyClone: HttpBody<Data=Bytes, Error=crate::Error> + Send + Sync + std::fmt::Debug {
    fn try_clone_body(&self) -> Option<Box<dyn BodyClone>>;
}

impl TryClone for Body {
	type Error = crate::Error;

	fn try_clone(&self) -> Result<Self, Self::Error> {
		self.0.try_clone_body().map(|b| Body(b.into())).ok_or(crate::error::body(crate::error::CannotCloneReaderBodyError))
	}
}
#[derive(Debug)]
/// An asynchronous request body.
pub struct Body(pub(crate) Pin<Box<dyn BodyClone + 'static>>);

impl Body {
	pub(crate) fn new<B>(body: B) -> Body
		where B: BodyClone + 'static
	{
		Body(Box::pin(body))
	}

	/// Wrap a futures `Stream` in a box inside `Body`.
	///
	/// # Example
	///
	/// ```
	/// # use reqwest::Body;
	/// # use futures_util;
	/// # fn main() {
	/// let chunks: Vec<Result<_, ::std::io::Error>> = vec![
	///     Ok("hello"),
	///     Ok(" "),
	///     Ok("world"),
	/// ];
	///
	/// let stream = futures_util::stream::iter(chunks);
	///
	/// let body = Body::wrap_stream(stream);
	/// # }
	/// ```
	///
	/// # Optional
	///
	/// This requires the `stream` feature to be enabled.
	#[cfg(feature = "stream")]
	pub fn wrap_stream<S>(stream: S) -> Body
		where
			S: futures_core::stream::TryStream + Send + Sync + 'static,
			S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
			Bytes: From<S::Ok>,
	{
		Body::stream(stream)
	}

	pub(crate) fn stream<S>(stream: S) -> Body
		where
			S: futures_core::stream::TryStream + Send + Sync + 'static,
			S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
			Bytes: From<S::Ok>,
	{
		Body::new(StreamingBody::from_stream(stream))
	}

	pub(crate) fn response(body: hyper::Body, timeout: Option<Delay>) -> Body {
		Body::new(StreamingBody::from_hyper(body, timeout))
	}

	#[cfg(feature = "blocking")]
	pub(crate) fn wrap(body: hyper::Body) -> Body {
		Body::new(StreamingBody::from_hyper(body, None))
	}

	pub(crate) fn empty() -> Body {
		Body::reusable(Bytes::new())
	}

	pub(crate) fn reusable(chunk: Bytes) -> Body {
		Body::new(BytesBody::new(chunk))
	}

	pub(crate) fn content_length(&self) -> Option<u64> {
		HttpBody::size_hint(self).exact()
	}
}

impl HttpBody for Body {
    type Data = Bytes;
    type Error = crate::Error;

    fn poll_data(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        self.0.as_mut().poll_data(cx)
    }

    fn poll_trailers(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<Option<HeaderMap<HeaderValue>>, Self::Error>> {
        self.0.as_mut().poll_trailers(cx)
    }

	fn is_end_stream(&self) -> bool {
		self.0.is_end_stream()
	}

	fn size_hint(&self) -> http_body::SizeHint {
		self.0.size_hint()
	}
}

impl Stream for Body {
	type Item = Result<Bytes, crate::Error>;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
		self.poll_data(cx)
	}
}

impl<T: Into<BytesBody>> From<T> for Body {
	fn from(into_bytes_body: T) -> Self {
		Body::new(into_bytes_body.into())
	}
}

impl Default for Body {
	fn default() -> Self {
		Body::empty()
	}
}

// The `Stream` trait isn't stable, so the impl isn't public.
// pub(crate) struct ImplStream(BodyOld);
