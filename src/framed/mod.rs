use bytes::{Bytes, BytesMut};
use futures_core::Stream;
use futures_sink::Sink;

use crate::serded::Serded;

mod lines_codec;
pub use lines_codec::*;

impl<T: ?Sized> Framed for T where T: Sink<Bytes> + Stream<Item = Result<BytesMut, Self::Error>> {}

pub trait Framed: Sink<Bytes> + Stream<Item = Result<BytesMut, Self::Error>> {}

impl<T: ?Sized> FramedExt for T where T: Framed {}

pub trait FramedExt: Framed {
    fn serded<Codec, Item, SinkItem>(
        self,
        serde_codec: Codec,
    ) -> Serded<Self, Codec, Item, SinkItem>
    where
        Self: Sized,
    {
        Serded::new(self, serde_codec)
    }

    fn serded_default<Codec, Item, SinkItem>(self) -> Serded<Self, Codec, Item, SinkItem>
    where
        Self: Sized,
        Codec: Default,
    {
        self.serded(Codec::default())
    }
}
