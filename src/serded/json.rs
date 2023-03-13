use std::{marker::PhantomData, pin::Pin};

use bytes::{Buf, Bytes, BytesMut};

use super::{Deserializer, Serializer};

pub struct Json<Item, SinkItem> {
    _maker: PhantomData<(Item, SinkItem)>,
}

impl<Item, SinkItem> Default for Json<Item, SinkItem> {
    fn default() -> Self {
        Self {
            _maker: Default::default(),
        }
    }
}

impl<Item, SinkItem> Deserializer<Item> for Json<Item, SinkItem>
where
    Item: serde::de::DeserializeOwned,
{
    type Error = serde_json::Error;

    fn deserialize(self: Pin<&mut Self>, src: &BytesMut) -> Result<Item, Self::Error> {
        serde_json::from_reader(std::io::Cursor::new(src).reader())
    }
}

impl<Item, SinkItem> Serializer<SinkItem> for Json<Item, SinkItem>
where
    SinkItem: serde::Serialize,
{
    type Error = serde_json::Error;

    fn serialize(self: Pin<&mut Self>, sink_item: &SinkItem) -> Result<Bytes, Self::Error> {
        serde_json::to_vec(sink_item).map(Into::into)
    }
}
