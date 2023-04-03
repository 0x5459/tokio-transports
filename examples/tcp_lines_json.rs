use anyhow::Context;
use futures::{SinkExt, StreamExt};
use tokio::{
    io::{stdin, AsyncBufReadExt, BufReader},
    net::TcpStream,
};
use tokio_transports::{
    framed::{FramedExt, LinesCodec},
    rw::ReadWriterExt,
    serded::Json,
};

// I must admit that this library does not support tcp/udp well enough at the moment.
// The contributions to this repository are open!
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // [tcpbin.com](https://tcpbin.com/) is a TCP echo server
    let transport = TcpStream::connect("tcpbin.com:4242") // connect to TCP echo server
        .await?
        .framed_default::<LinesCodec>() // use lines codec
        .serded_default::<Json<_, _>, String, String>(); // use json to serialize and deserialize messages

    let (mut sink, stream) = transport.split();
    tokio::spawn(stream.for_each(|r| async move {
        println!("{:?}", r);
    }));

    let mut input = BufReader::new(stdin()).lines();
    while let Some(line) = input.next_line().await.context("read line from stdin")? {
        sink.send(line)
            .await
            .context("failed to send request to sink")?;
    }

    Ok(())
}
