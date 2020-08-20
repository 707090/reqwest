use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};

use ::bytes::Bytes;
use fallible::TryClone;
use futures_core::Stream;
use http_body::Body as HttpBody;
use tokio::time::Delay;

use self::bytes::BytesBody;
use streaming::StreamingBody;

mod bytes;
mod streaming;

/// An asynchronous request body.
pub struct Body {
	inner: Inner,
}

impl Body {
	fn inner(self: Pin<&mut Self>) -> Pin<&mut Inner> {
		unsafe { self.map_unchecked_mut(|b| &mut b.inner)}
	}
}

// The `Stream` trait isn't stable, so the impl isn't public.
pub(crate) struct ImplStream(Body);

enum Inner {
	Bytes(BytesBody),
	Streaming(StreamingBody),
}

impl Body {
	/// Returns a reference to the internal data of the `Body`.
	///
	/// `None` is returned, if the underlying data is a stream.
	pub fn as_bytes(&self) -> Option<&[u8]> {
		match &self.inner {
			Inner::Bytes(bytes_body) => Some(bytes_body.as_bytes()),
			Inner::Streaming(_) => None,
		}
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
		Body { inner: Inner::Streaming(StreamingBody::from_stream(stream)) }
	}

	pub(crate) fn response(body: hyper::Body, timeout: Option<Delay>) -> Body {
		Body {
			inner: Inner::Streaming(StreamingBody::from_hyper(body, timeout)),
		}
	}

	#[cfg(feature = "blocking")]
	pub(crate) fn wrap(body: hyper::Body) -> Body {
		Body {
			inner: Inner::Streaming(StreamingBody::from_hyper(body, None)),
		}
	}

	pub(crate) fn empty() -> Body {
		Body::reusable(Bytes::new())
	}

	pub(crate) fn reusable(chunk: Bytes) -> Body {
		Body {
			inner: Inner::Bytes(BytesBody::new(chunk)),
		}
	}

	pub(crate) fn try_reuse(&self) -> Option<Bytes> {
        match &self.inner {
			Inner::Bytes(bytes_body) => Some(bytes_body.body.clone()),
			Inner::Streaming(_) => None,
		}
    }

	pub(crate) fn into_stream(self) -> ImplStream {
		ImplStream(self)
	}

	pub(crate) fn content_length(&self) -> Option<u64> {
		match &self.inner {
			Inner::Bytes(bytes_body) => bytes_body.content_length(),
			Inner::Streaming(streaming_body) => streaming_body.content_length(),
		}
	}
}

impl TryClone for Body {
	type Error = crate::error::Error;

	fn try_clone(&self) -> Result<Self, Self::Error> {
		match &self.inner {
			Inner::Bytes(bytes_body) => Ok(Self { inner: Inner::Bytes(bytes_body.clone())}),
			Inner::Streaming(streaming_body) =>
				streaming_body.try_clone().map(|streaming_body| Self {
					inner: Inner::Streaming(streaming_body)
				}),
		}
	}
}

impl From<Bytes> for Body {
	#[inline]
	fn from(bytes: Bytes) -> Body {
		Body::reusable(bytes)
	}
}

impl From<Vec<u8>> for Body {
	#[inline]
	fn from(vec: Vec<u8>) -> Body {
		Body::reusable(vec.into())
	}
}

impl From<&'static [u8]> for Body {
	#[inline]
	fn from(s: &'static [u8]) -> Body {
		Body::reusable(Bytes::from_static(s))
	}
}

impl From<String> for Body {
	#[inline]
	fn from(s: String) -> Body {
		Body::reusable(s.into())
	}
}

impl From<&'static str> for Body {
	#[inline]
	fn from(s: &'static str) -> Body {
		s.as_bytes().into()
	}
}

impl fmt::Debug for Body {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.debug_struct("Body").finish()
	}
}

impl HttpBody for ImplStream {
	type Data = Bytes;
	type Error = crate::Error;

	fn poll_data(
		self: Pin<&mut Self>,
		cx: &mut Context,
	) -> Poll<Option<Result<Self::Data, Self::Error>>> {
		let body_pin = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
		match body_pin.inner().get_mut() {
			Inner::Streaming(streaming_body) => {
				Pin::new(streaming_body).poll_data(cx)
			},
			Inner::Bytes(bytes_body) => Pin::new(bytes_body).poll_data(cx),
		}
	}

	fn poll_trailers(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
		let body_pin = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
		match body_pin.inner().get_mut() {
			Inner::Streaming(streaming_body) => Pin::new(streaming_body).poll_trailers(cx),
			Inner::Bytes(bytes_body) => Pin::new(bytes_body).poll_trailers(cx),
		}
	}

	fn is_end_stream(&self) -> bool {
		match &self.0.inner {
			Inner::Streaming(streaming_body) => streaming_body.is_end_stream(),
			Inner::Bytes(bytes_body) => bytes_body.is_end_stream(),
		}
	}

	fn size_hint(&self) -> http_body::SizeHint {
		match &self.0.inner {
			Inner::Streaming(streaming_body) => streaming_body.size_hint(),
			Inner::Bytes(bytes_body) => bytes_body.size_hint(),
		}
	}
}

impl Stream for ImplStream {
	type Item = Result<Bytes, crate::Error>;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
		self.poll_data(cx)
	}
}

#[cfg(test)]
mod tests {
	use super::Body;

	#[test]
	fn test_as_bytes() {
		let test_data = b"Test body";
		let body = Body::from(&test_data[..]);
		assert_eq!(body.as_bytes(), Some(&test_data[..]));
	}
}
