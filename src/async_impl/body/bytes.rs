use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use hyper::body::HttpBody;

#[derive(Clone)]
pub struct BytesBody {
	pub(super) body: Bytes,
}

impl BytesBody {
	pub fn new(bytes: Bytes) -> BytesBody {
		BytesBody { body: bytes }
	}

	pub fn as_bytes(&self) -> &[u8] {
		self.body.as_ref()
	}

	pub fn content_length(&self) -> Option<u64> {
		Some(self.body.len() as u64)
	}
}

impl HttpBody for BytesBody {
	type Data = Bytes;
	type Error = crate::Error;

	fn poll_data(
		mut self: Pin<&mut Self>,
		_cx: &mut Context,
	) -> Poll<Option<Result<Self::Data, Self::Error>>> {
		let opt_try_chunk = if self.body.is_empty() {
			None
		} else {
			Some(Ok(std::mem::replace(&mut self.body, Bytes::new())))
		};

		Poll::Ready(opt_try_chunk)
	}

	fn poll_trailers(self: Pin<&mut Self>, _cx: &mut Context) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
		Poll::Ready(Ok(None))
	}

	fn is_end_stream(&self) -> bool {
		self.body.is_empty()
	}

	fn size_hint(&self) -> http_body::SizeHint {
		let mut hint = http_body::SizeHint::default();
		hint.set_exact(self.body.len() as u64);
		hint
	}
}