use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body::Body as HttpBody;

use super::BodyClone;

#[derive(Clone, Debug)]
pub struct BytesBody {
    pub(super) body: Bytes,
}

impl BytesBody {
    pub fn new(bytes: Bytes) -> BytesBody {
        BytesBody { body: bytes }
    }

    /// Returns a reference to the internal data of the `Body`.
    ///
    /// `None` is returned, if the underlying data is a stream.
    pub fn as_bytes(&self) -> &[u8] {
        self.body.as_ref()
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

    fn poll_trailers(
        self: Pin<&mut Self>,
        _cx: &mut Context,
    ) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
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

impl BodyClone for BytesBody {
    fn try_clone_body(&self) -> Option<Box<dyn BodyClone<Data = Bytes, Error = crate::Error>>> {
        Some(Box::new(self.clone()))
    }
}

impl From<Bytes> for BytesBody {
    #[inline]
    fn from(bytes: Bytes) -> BytesBody {
        BytesBody::new(bytes)
    }
}

impl From<Vec<u8>> for BytesBody {
    #[inline]
    fn from(vec: Vec<u8>) -> BytesBody {
        BytesBody::new(vec.into())
    }
}

impl From<&'static [u8]> for BytesBody {
    #[inline]
    fn from(s: &'static [u8]) -> BytesBody {
        BytesBody::new(Bytes::from_static(s))
    }
}

impl From<String> for BytesBody {
    #[inline]
    fn from(s: String) -> BytesBody {
        BytesBody::new(s.into())
    }
}

impl From<&'static str> for BytesBody {
    #[inline]
    fn from(s: &'static str) -> BytesBody {
        s.as_bytes().into()
    }
}

#[cfg(test)]
mod tests {
    use super::BytesBody;

    #[test]
    fn test_as_bytes() {
        let test_data = b"Test body";
        let body = BytesBody::from(&test_data[..]);
        assert_eq!(body.as_bytes(), &test_data[..]);
    }
}
