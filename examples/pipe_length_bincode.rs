use std::env::{self, current_exe};

use anyhow::Context;
use futures::{SinkExt, StreamExt};
use tokio::io::{stdin, AsyncBufReadExt, BufReader};
use tokio_transports::{
    framed::FramedExt,
    rw::{pipe, ReadWriterExt},
    serded::Bincode,
};
use tokio_util::codec::LengthDelimitedCodec;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = env::args().collect::<Vec<String>>();
    if args.len() == 2 && args[1] == "consumer" {
        consumer().await
    } else {
        producer().await
    }
}

async fn producer() -> anyhow::Result<()> {
    let transport = pipe::connect(pipe::Command::new(current_exe()?).args(vec!["consumer"])) // start consumer as a child process
        .framed_default::<LengthDelimitedCodec>() // use length delimited codec
        .serded_default::<Bincode<_, _>, String, String>(); // use json to serialize and deserialize messages

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

async fn consumer() -> anyhow::Result<()> {
    let transport = pipe::listen() // listen to the stdio
        .framed_default::<LengthDelimitedCodec>() // use length delimited codec
        .serded_default::<Bincode<_, _>, String, String>(); // use json to serialize and deserialize messages

    let (sink, stream) = transport.split();

    stream.forward(sink).await?;
    Ok(())
}
