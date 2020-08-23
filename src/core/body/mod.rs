use std::pin::Pin;
use std::task::{Context, Poll};

use ::bytes::Bytes;
use fallible::TryClone;
use futures_core::Stream;
use http::{HeaderMap, HeaderValue};
use http_body::Body as HttpBody;

use streaming::StreamingBody;

use self::bytes::BytesBody;
use crate::core::body::reader::ReaderBody;
use std::fs::File;
use std::io::Read;

mod reader;

mod bytes;
mod streaming;

pub trait BodyClone:
    HttpBody<Data = Bytes, Error = crate::Error> + Send + Sync + std::fmt::Debug
{
    fn try_clone_body(&self) -> Option<Box<dyn BodyClone>>;
}

impl TryClone for Body {
    type Error = crate::Error;

    fn try_clone(&self) -> Result<Self, Self::Error> {
        self.0
            .try_clone_body()
            .map(|b| Body(b.into()))
            .ok_or(crate::error::body(crate::error::CannotCloneBodyError))
    }
}

#[derive(Debug)]
/// A request body.
pub struct Body(pub(crate) Pin<Box<dyn BodyClone + 'static>>);

impl Body {
    pub(crate) fn new<B>(body: B) -> Body
    where
        B: BodyClone + 'static,
    {
        Body(Box::pin(body))
    }

    /// Create a `Body` from a `Read` where the size may be known in advance
    /// but the data should not be fully loaded into memory. This will
    /// set the `Content-Length` header and stream from the `Read`.
    ///
    /// Note: This is for example purposes. Body implements From<File>, so using `Body::from(file)`
    /// would be a simpler way to achieve the same thing.
    /// ```rust
    /// # use std::fs::File;
    /// # use reqwest::Body;
    /// # fn run() -> Result<(), Box<std::error::Error>> {
    /// let file = File::open("a_large_file.txt")?;
    /// let file_size = file.metadata()?.len();
    /// let body = Body::from_reader(file, Some(file_size as usize));
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_reader<R: Read + Send + Sync + 'static>(reader: R, len: Option<usize>) -> Body {
        Body::new(ReaderBody::new(Box::new(reader), len))
    }

    /// Wrap a futures `Stream` inside `Body`.
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
    /// let body = Body::from_stream(stream);
    /// # }
    /// ```
    ///
    /// # Optional
    ///
    /// This requires the `stream` feature to be enabled.
    #[cfg(feature = "stream")]
    pub fn from_stream<S>(stream: S) -> Body
    where
        S: futures_core::stream::TryStream + Send + Sync + 'static,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        Bytes: From<S::Ok>,
    {
        Body::from_stream_inner(stream)
    }

    pub(crate) fn from_stream_inner<S>(stream: S) -> Body
    where
        S: futures_core::stream::TryStream + Send + Sync + 'static,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        Bytes: From<S::Ok>,
    {
        Body::new(StreamingBody::from_stream(stream))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn response(body: hyper::Body, timeout: Option<futures_timer::Delay>) -> Body {
        Body::new(StreamingBody::from_hyper(body, timeout))
    }

    pub(crate) fn empty() -> Body {
        Body::from_bytes(Bytes::new())
    }

    pub(crate) fn from_bytes(chunk: Bytes) -> Body {
        Body::new(BytesBody::new(chunk))
    }

    pub(crate) fn content_length(&self) -> Option<u64> {
        HttpBody::size_hint(self).exact()
    }
}

impl HttpBody for Body {
    type Data = Bytes;
    type Error = crate::Error;

    fn poll_data(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        self.0.as_mut().poll_data(cx)
    }

    fn poll_trailers(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Option<HeaderMap<HeaderValue>>, Self::Error>> {
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

impl<T> From<T> for Body
where
    BytesBody: From<T>,
{
    fn from(into_bytes_body: T) -> Self {
        Body::new(BytesBody::from(into_bytes_body))
    }
}

impl From<File> for Body {
    #[inline]
    fn from(f: File) -> Body {
        Body::new(ReaderBody::from(f))
    }
}

impl Default for Body {
    fn default() -> Self {
        Body::empty()
    }
}

// The `Stream` trait isn't stable, so the impl isn't public.
// pub(crate) struct ImplStream(Body);

// useful for tests, but not publicly exposed
#[cfg(test)]
pub(crate) fn read_to_string(body: Body) -> crate::Result<String> {
    use futures_util::stream::TryStreamExt;

    let mut rt = tokio::runtime::Builder::new()
        .basic_scheduler()
        .enable_all()
        .build()
        .expect("test tokio runtime");
    let s = body.map_ok(|try_c| try_c.to_vec()).try_concat();

    match rt.block_on(s) {
        Ok(output) => String::from_utf8(output).map_err(crate::error::decode),
        Err(e) => Err(e),
    }
}
