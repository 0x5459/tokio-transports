use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project::pin_project;
use tokio::io::{stdin, stdout, AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf, Stdin, Stdout};

/// Creates a new ReadWriter using stdio
pub fn listen() -> Stdio {
    Stdio {
        stdin: stdin(),
        stdout: stdout(),
    }
}

pub async fn listen_ready_message(ready_message: impl AsRef<str>) -> io::Result<Stdio> {
    async fn writeln<W: AsyncWrite + Unpin>(w: &mut W, buf: &[u8]) -> io::Result<()> {
        w.write_all(buf).await?;
        w.write_u8(b'\n').await?;
        w.flush().await
    }

    let mut stdout = stdout();
    writeln(&mut stdout, ready_message.as_ref().as_bytes()).await?;
    Ok(Stdio {
        stdin: stdin(),
        stdout,
    })
}

/// A handle to the standard input/output.
#[pin_project]
pub struct Stdio {
    #[pin]
    stdin: Stdin,
    #[pin]
    stdout: Stdout,
}

impl AsyncRead for Stdio {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.project().stdin.poll_read(cx, buf)
    }
}

impl AsyncWrite for Stdio {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().stdout.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().stdout.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().stdout.poll_shutdown(cx)
    }
}
