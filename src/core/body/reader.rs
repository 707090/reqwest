use std::fs::File;
use std::io::Read;

use fallible::TryClone;

use crate::core::body::BodyClone;
use bytes::{Bytes, BytesMut};
use http_body::Body as HttpBody;
use http_body::SizeHint;
use std::pin::Pin;
use std::task::{Context, Poll};

const DEFAULT_CHUNK_SIZE: usize = 8192;

pub struct ReaderBody {
    pub(crate) reader: Box<dyn Read + Send + Sync>,
    pub(crate) remaining_len: Option<usize>,
}

impl ReaderBody {
    pub fn new(reader: Box<dyn Read + Send + Sync>, len: Option<usize>) -> ReaderBody {
        ReaderBody {
            reader,
            remaining_len: len,
        }
    }
}

impl BodyClone for ReaderBody {
    fn try_clone_body(&self) -> Option<Box<dyn BodyClone<Data = Bytes, Error = crate::Error>>> {
        None
    }
}

impl HttpBody for ReaderBody {
    type Data = Bytes;
    type Error = crate::Error;

    fn poll_data(
        mut self: Pin<&mut Self>,
        _cx: &mut Context,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        let chunk_size = std::cmp::min(
            self.remaining_len.unwrap_or(DEFAULT_CHUNK_SIZE),
            DEFAULT_CHUNK_SIZE,
        );
        let mut bytes = BytesMut::with_capacity(chunk_size);
        unsafe { bytes.set_len(chunk_size) };
        match self.reader.read(bytes.as_mut()) {
            Ok(0) => Poll::Ready(None),
            Ok(size) => {
                if let Some(value) = self.remaining_len.as_mut() {
                    *value -= size;
                }
                unsafe { bytes.set_len(size) };
                Poll::Ready(Some(Ok(bytes.freeze())))
            }
            Err(e) => Poll::Ready(Some(Err(crate::error::body(e)))),
        }
    }

    fn poll_trailers(
        self: Pin<&mut Self>,
        _cx: &mut Context,
    ) -> Poll<Result<Option<http::HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None))
    }

    fn is_end_stream(&self) -> bool {
        if let Some(0) = self.remaining_len {
            true
        } else {
            false
        }
    }

    fn size_hint(&self) -> SizeHint {
        self.remaining_len
            .map(|remaining| SizeHint::with_exact(remaining as u64))
            .unwrap_or(SizeHint::default())
    }
}

impl TryClone for ReaderBody {
    type Error = crate::Error;

    fn try_clone(&self) -> Result<Self, Self::Error> {
        Err(crate::error::builder(
            crate::error::CannotCloneReaderBodyError,
        ))
    }
}

impl From<File> for ReaderBody {
    #[inline]
    fn from(f: File) -> ReaderBody {
        ReaderBody {
            remaining_len: f.metadata().map(|m| m.len() as usize).ok(),
            reader: Box::new(f),
        }
    }
}

impl std::fmt::Debug for ReaderBody {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Reader")
            .field("remaining length", &self.remaining_len)
            .finish()
    }
}
