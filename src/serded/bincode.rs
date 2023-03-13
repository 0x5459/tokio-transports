use std::{io, marker::PhantomData, pin::Pin};

use bincode as bincode_crate;
use bincode_crate::Options;
use bytes::{Bytes, BytesMut};

use super::{Deserializer, Serializer};

pub struct Bincode<Item, SinkItem, O = bincode_crate::DefaultOptions> {
    options: O,
    _maker: PhantomData<(SinkItem, Item)>,
}

impl<Item, SinkItem> Default for Bincode<Item, SinkItem> {
    fn default() -> Self {
        Bincode {
            options: Default::default(),
            _maker: PhantomData,
        }
    }
}

impl<Item, SinkItem, O> Deserializer<Item> for Bincode<Item, SinkItem, O>
where
    Item: serde::de::DeserializeOwned,
    O: Options + Clone,
{
    type Error = io::Error;

    fn deserialize(self: Pin<&mut Self>, src: &BytesMut) -> Result<Item, Self::Error> {
        self.options
            .clone()
            .deserialize(src)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

impl<Item, SinkItem, O> Serializer<SinkItem> for Bincode<Item, SinkItem, O>
where
    SinkItem: serde::Serialize,
    O: Options + Clone,
{
    type Error = io::Error;

    fn serialize(self: Pin<&mut Self>, sink_item: &SinkItem) -> Result<Bytes, Self::Error> {
        self.options
            .clone()
            .serialize(sink_item)
            .map(Into::into)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}
