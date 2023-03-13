use std::{
    error::Error as StdError,
    fmt::{self, Debug},
    marker::PhantomData,
    pin::Pin,
    task::{ready, Context, Poll},
};

use bytes::{Bytes, BytesMut};
use futures_core::Stream;
use futures_sink::Sink;
use pin_project::pin_project;

use crate::framed::Framed;

#[cfg(feature = "serded-bincode")]
mod bincode;
#[cfg(feature = "serded-json")]
mod json;

#[cfg(feature = "serded-bincode")]
pub use self::bincode::Bincode;
#[cfg(feature = "serded-json")]
pub use self::json::Json;

pub trait Serializer<T> {
    type Error;

    /// Serializes `item` into a new buffer
    fn serialize(self: Pin<&mut Self>, item: &T) -> Result<Bytes, Self::Error>;
}

pub trait Deserializer<T> {
    type Error;

    /// Deserializes a value from `buf`
    fn deserialize(self: Pin<&mut Self>, src: &BytesMut) -> Result<T, Self::Error>;
}

#[pin_project]
pub struct Serded<FramedIO, Serde, Item, SinkItem> {
    #[pin]
    inner: FramedIO,
    #[pin]
    serde: Serde,
    _maker: PhantomData<(SinkItem, Item)>,
}

impl<FramedIO, Serde, Item, SinkItem> Serded<FramedIO, Serde, Item, SinkItem> {
    pub fn new(inner: FramedIO, serde: Serde) -> Self {
        Self {
            inner,
            serde,
            _maker: PhantomData,
        }
    }
}

impl<FramedIO, Serde, Item, SinkItem> Stream for Serded<FramedIO, Serde, Item, SinkItem>
where
    FramedIO: Framed,
    FramedIO::Error: StdError,
    Serde: Deserializer<Item>,
    Serde::Error: StdError,
{
    type Item = Result<Item, Error<FramedIO::Error, Serde::Error>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // TODO(0x5459): use spawn_blocking
        match ready!(self
            .as_mut()
            .project()
            .inner
            .poll_next(cx)
            .map_err(Error::Inner)?)
        {
            Some(src) => Poll::Ready(Some(
                self.as_mut()
                    .project()
                    .serde
                    .deserialize(&src)
                    .map_err(Error::Serde),
            )),
            None => Poll::Ready(None),
        }
    }
}

impl<FramedIO, Serde, Item, SinkItem> Sink<SinkItem> for Serded<FramedIO, Serde, Item, SinkItem>
where
    FramedIO: Framed,
    FramedIO::Error: StdError,
    Serde: Serializer<SinkItem>,
    Serde::Error: StdError,
{
    type Error = Error<FramedIO::Error, Serde::Error>;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_ready(cx).map_err(Error::Inner)
    }

    fn start_send(mut self: Pin<&mut Self>, item: SinkItem) -> Result<(), Self::Error> {
        // TODO(0x5459): use spawn_blocking
        let bytes = self
            .as_mut()
            .project()
            .serde
            .serialize(&item)
            .map_err(Error::Serde)?;

        self.as_mut()
            .project()
            .inner
            .start_send(bytes)
            .map_err(Error::Inner)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx).map_err(Error::Inner)
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx).map_err(Error::Inner)
    }
}

#[derive(Debug)]
pub enum Error<IE: StdError, SE: StdError> {
    Inner(IE),
    Serde(SE),
}

impl<IE: StdError, CE: StdError> fmt::Display for Error<IE, CE> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Inner(e) => write!(f, "inner error: {}", e),
            Error::Serde(e) => write!(f, "serde error: {}", e),
        }
    }
}

impl<IE: StdError, CE: StdError> StdError for Error<IE, CE> {}
