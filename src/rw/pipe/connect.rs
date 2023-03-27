use std::{
    collections::HashMap,
    ffi::OsString,
    future::Future,
    io,
    path::PathBuf,
    pin::Pin,
    task::{ready, Context, Poll},
    time::Duration,
};

use pin_project::pin_project;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader, Lines, ReadBuf},
    process::{Child, ChildStdout, Command as TokioCommand},
    time::{sleep, Sleep},
};

/// Executes the command as a child process and establish pipe connection,
///
/// # Example
/// ```
/// use tokio::io::{AsyncReadExt, AsyncWriteExt};
///
/// use tokio_transport::rw::{
///     pipe::{connect, Command},
///     util::ReadWriterExt,
/// };
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let mut cat = connect(Command::new("/bin/cat"));
///     // Write some data.
///     cat.ready().await?.write_all(b"hello world").await?;
///
///     let mut buf = vec![0; 11];
///     cat.read_exact(&mut buf).await?;
///     assert_eq!(b"hello world", buf.as_slice());
///     Ok(())
/// }
/// ```
pub fn connect(cmd: Command) -> ChildProcess {
    let make_cmd = MakeCommand::new(cmd.bin, cmd.args, cmd.envs);
    ChildProcess::new(
        make_cmd,
        cmd.auto_restart,
        cmd.ready_timeout,
        cmd.ready_message,
    )
}

/// Representation of a running or exited child process.
#[pin_project]
pub struct ChildProcess {
    auto_restart: bool,
    ready_message: Option<String>,
    ready_timeout: Option<Duration>,
    make_cmd: MakeCommand,

    #[cfg(feature = "tracing")]
    span: tracing::Span,

    #[pin]
    state: ChildState,
}

impl ChildProcess {
    pub(crate) fn new(
        make_cmd: MakeCommand,
        auto_restart: bool,
        ready_timeout: Option<Duration>,
        ready_message: Option<String>,
    ) -> Self {
        #[cfg(feature = "tracing")]
        let span = tracing::error_span!("child process", child_pid = tracing::field::Empty);
        Self {
            state: ChildState::Restarting(restart(
                make_cmd.make(),
                ready_timeout,
                ready_message.clone(),
            )),
            auto_restart,
            ready_message,
            make_cmd,
            #[cfg(feature = "tracing")]
            span,
            ready_timeout,
        }
    }

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut pinned = self.as_mut().project();
        loop {
            let next_state = match pinned.state.as_mut().project() {
                ChildStateProj::Running(child) => match child.try_wait()? {
                    None => return Poll::Ready(Ok(())),
                    Some(status) => {
                        #[cfg(feature = "tracing")]
                        (|| {
                            pinned.span.in_scope(|| {
                                tracing::warn!("child process is exited: {}", status);
                            });
                            *pinned.span = tracing::error_span!(
                                "child process",
                                child_pid = tracing::field::Empty
                            );
                        })();
                        if *pinned.auto_restart {
                            // restart the child process
                            ChildState::Restarting(restart(
                                pinned.make_cmd.make(),
                                *pinned.ready_timeout,
                                pinned.ready_message.clone(),
                            ))
                        } else {
                            ChildState::Exit
                        }
                    }
                },
                ChildStateProj::Restarting(fut) => {
                    let child = ready!(fut.poll(cx))?;
                    #[cfg(feature = "tracing")]
                    pinned.span.record("child_pid", child.id());
                    #[cfg(feature = "tracing")]
                    pinned.span.in_scope(|| {
                        tracing::info!("child process ready");
                    });
                    ChildState::Running(child)
                }
                ChildStateProj::Exit => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "child process is exited",
                    )))
                }
            };
            pinned.state.as_mut().set(next_state);
        }
    }

    fn child(self: Pin<&mut Self>) -> io::Result<&mut Child> {
        match self.project().state.project() {
            ChildStateProj::Running(child) => Ok(child),
            _ => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "child process is not running. please call `processor.poll_ready`",
            )),
        }
    }
}

impl AsyncRead for ChildProcess {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        ready!(self.as_mut().poll_ready(cx))?;
        Pin::new(self.child()?.stdout.as_mut().unwrap()).poll_read(cx, buf)
    }
}

impl AsyncWrite for ChildProcess {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        ready!(self.as_mut().poll_ready(cx))?;
        Pin::new(self.child()?.stdin.as_mut().unwrap()).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().poll_ready(cx))?;
        Pin::new(self.child()?.stdin.as_mut().unwrap()).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().poll_ready(cx))?;
        Pin::new(self.child()?.stdin.as_mut().unwrap()).poll_shutdown(cx)
    }
}

#[pin_project(project=ChildStateProj)]
enum ChildState {
    Running(Child),
    Restarting(#[pin] RestartFuture),
    Exit,
}

fn restart(
    cmd: TokioCommand,
    stable_timeout: Option<Duration>,
    ready_msg: Option<String>,
) -> RestartFuture {
    RestartFuture {
        state: RestartState::Init(cmd),
        ready_msg,
        stable_timeout,
    }
}

#[pin_project]
struct RestartFuture {
    #[pin]
    state: RestartState,
    ready_msg: Option<String>,
    stable_timeout: Option<Duration>,
}

impl Future for RestartFuture {
    type Output = io::Result<Child>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut pinned = self.as_mut().project();
        loop {
            let next_state = match pinned.state.as_mut().project() {
                RestartStateProj::Init(cmd) => RestartState::Started(Some(cmd.spawn()?)),
                RestartStateProj::Started(child) => {
                    let mut child = child.take().unwrap();
                    #[cfg(feature = "tracing")]
                    tracing::debug!(pid=?child.id(), "child process was started");

                    // ready_msg is None, which means there is no need to
                    // wait for the child process to be ready
                    if pinned.ready_msg.is_none() {
                        return Poll::Ready(Ok(child));
                    }

                    let child_stdout = BufReader::new(child.stdout.take().unwrap());

                    RestartState::WaitReadyMsg {
                        child: Some(child),
                        lines: Some(child_stdout.lines()),
                        timeout: pinned.stable_timeout.map(|x| Box::pin(sleep(x))),
                    }
                }
                RestartStateProj::WaitReadyMsg {
                    child,
                    lines,
                    timeout,
                } => {
                    // check timeout
                    if let Some(timeout) = timeout {
                        if Poll::Ready(()) == timeout.as_mut().poll(cx) {
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "timeout exceeded before child get ready",
                            )));
                        }
                    }

                    let next_line = ready!(Pin::new(lines.as_mut().unwrap()).poll_next_line(cx))?;
                    return Poll::Ready(match next_line {
                        Some(x) if Some(x.trim()) != pinned.ready_msg.as_deref() => {
                            Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("unexpected first line: {}", x),
                            ))
                        }
                        None => Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "unexpected first line: EOF",
                        )),
                        _ => {
                            let mut child = child.take().expect("polled after complete");
                            child.stdout = Some(
                                lines
                                    .take()
                                    .expect("polled after complete")
                                    .into_inner()
                                    .into_inner(),
                            );
                            Ok(child)
                        }
                    });
                }
            };
            pinned.state.set(next_state);
        }
    }
}

#[pin_project(project=RestartStateProj)]
enum RestartState {
    Init(TokioCommand),
    Started(Option<Child>),
    WaitReadyMsg {
        child: Option<Child>,
        lines: Option<Lines<BufReader<ChildStdout>>>,
        timeout: Option<Pin<Box<Sleep>>>,
    },
}

/// A `process::Command` builder.
pub struct Command {
    pub bin: PathBuf,
    pub args: Vec<OsString>,
    pub envs: HashMap<OsString, OsString>,
    pub auto_restart: bool,
    pub ready_timeout: Option<Duration>,
    pub ready_message: Option<String>,
}

impl Command {
    pub fn new(bin: impl Into<PathBuf>) -> Self {
        Self {
            bin: bin.into(),
            args: Vec::new(),
            envs: HashMap::new(),
            auto_restart: true,
            ready_timeout: None,
            ready_message: None,
        }
    }

    /// Adds an argument to pass to the program.
    pub fn arg(&mut self, arg: impl Into<OsString>) -> &mut Command {
        self.args.push(arg.into());
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        for arg in args {
            self.arg(arg);
        }
        self
    }

    /// Adds or updates multiple environment variable mappings.
    pub fn envs<K, V>(mut self, vars: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<OsString>,
        V: Into<OsString>,
    {
        for (key, val) in vars {
            self.envs.insert(key.into(), val.into());
        }
        self
    }

    /// Controls whether the process is automatically restarted when it exits.
    pub fn auto_restart(mut self, auto_restart: bool) -> Self {
        self.auto_restart = auto_restart;
        self
    }

    /// Wait a limited time until the process is ready (output ready message).
    /// ready_timeout is None indicating unlimited waiting time
    pub fn ready_timeout(mut self, ready_timeout: impl Into<Duration>) -> Self {
        self.ready_timeout = Some(ready_timeout.into());
        self
    }

    pub fn ready_message(mut self, ready_message: impl Into<String>) -> Self {
        self.ready_message = Some(ready_message.into());
        self
    }
}

pub(crate) struct MakeCommand {
    bin: PathBuf,
    args: Vec<OsString>,
    envs: HashMap<OsString, OsString>,
}

impl MakeCommand {
    pub fn new(bin: PathBuf, args: Vec<OsString>, envs: HashMap<OsString, OsString>) -> Self {
        Self { bin, args, envs }
    }

    pub fn make(&self) -> TokioCommand {
        let mut cmd = TokioCommand::new(self.bin.clone());
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .args(self.args.iter())
            .envs(self.envs.iter())
            .kill_on_drop(true);
        cmd
    }
}
