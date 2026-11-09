//! The server's stderr drain into a bounded ring of capped lines.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader, ReadBuf};
#[cfg(not(windows))]
use tokio::process::ChildStderr;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub(super) const STDERR_RING_CAPACITY: usize = 100;
pub(super) const STDERR_LINE_MAX_CHARS: usize = 2_000;
/// Bytes of one stderr line kept in memory (4 × the char cap, the UTF-8
/// worst case). The rest of an overlong line is read and discarded.
const STDERR_LINE_MAX_BYTES: usize = STDERR_LINE_MAX_CHARS * 4;

/// The drain of one server's stderr.
pub(super) struct StderrDrain {
    task: JoinHandle<()>,
    exited: oneshot::Sender<()>,
}

impl StderrDrain {
    /// The server has exited: keep what it wrote, then end the drain. Every
    /// byte the server wrote is in the pipe once it exited, but a process it
    /// left behind (a daemon in its own session) can hold the pipe open for
    /// as long as it lives, so the drain reads what the pipe holds now and
    /// does not wait for EOF.
    pub(super) async fn finish(self) {
        let _ = self.exited.send(());
        let _ = self.task.await;
    }

    /// Stop draining: the server is stopped.
    pub(super) fn abort(self) {
        self.task.abort();
    }
}

/// The parent's end of a server's stderr. Unix reads the child's pipe.
/// Windows reads an overlapped named pipe it created before the spawn
/// (`server_process::StderrPipe`): the child's own stderr pipe is read on a
/// blocking thread, whose read in flight can hold bytes nothing can observe.
#[cfg(not(windows))]
pub(super) type StderrReader = ChildStderr;
#[cfg(windows)]
pub(super) type StderrReader = Overlapped<tokio::net::windows::named_pipe::NamedPipeServer>;

pub(super) fn spawn_stderr_reader(
    stderr: StderrReader,
    stderr_lines: Arc<Mutex<VecDeque<String>>>,
) -> StderrDrain {
    let (exited, on_exit) = oneshot::channel();
    let task = tokio::spawn(drain_stderr(UntilExit::new(stderr, on_exit), stderr_lines));
    StderrDrain { task, exited }
}

/// A pipe once its server has exited: what it holds now, found without
/// waiting on a writer. Every byte the server wrote is in the pipe by then,
/// but a process it left behind can hold the pipe open.
pub(super) trait ReadNow {
    /// Fill `buf` with bytes the pipe holds, or leave it unfilled when the
    /// pipe is empty or at EOF; the drain then ends.
    fn poll_read_now(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>>;
}

#[cfg(unix)]
impl ReadNow for ChildStderr {
    /// A non-blocking read of the pipe: bytes, EOF, or `WouldBlock` when it
    /// is empty.
    fn poll_read_now(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        use std::io::Read;
        use std::os::fd::AsFd;
        loop {
            // The duplicate shares the pipe's open file, so its non-blocking mode.
            let read = self
                .as_fd()
                .try_clone_to_owned()
                .map(std::fs::File::from)
                .and_then(|mut pipe| pipe.read(buf.initialize_unfilled()));
            match read {
                Ok(read) => {
                    buf.advance(read);
                    return Poll::Ready(Ok(()));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    return Poll::Ready(Ok(()));
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }
}

/// A platform with neither pipe kind reads its stream to EOF.
#[cfg(not(any(unix, windows)))]
impl ReadNow for ChildStderr {
    fn poll_read_now(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.poll_read(cx, buf)
    }
}

/// `ERROR_OPERATION_ABORTED`: how a cancelled overlapped read completes.
#[cfg(any(windows, test))]
pub(super) const READ_CANCELLED: i32 = 995;
#[cfg(windows)]
const _: () = assert!(windows_sys::Win32::Foundation::ERROR_OPERATION_ABORTED == 995);

/// An overlapped pipe's read in flight, which can be cancelled.
#[cfg(any(windows, test))]
pub(super) trait CancelRead {
    /// Cancel the read in flight. One that already took bytes completes with
    /// them; one parked on an empty pipe completes with [`READ_CANCELLED`].
    fn cancel_read(&mut self);
}

/// An overlapped pipe (Windows), read through the reactor, which keeps one
/// read in flight on it. That read takes bytes as soon as the pipe holds
/// them and hands them over a reactor turn later, so an empty pipe does not
/// prove that nothing is left. Once the server has exited, the read is
/// cancelled: it completes now, with the bytes it took or cancelled, and
/// never waits on a writer.
#[cfg(any(windows, test))]
pub(super) struct Overlapped<P>(pub(super) P);

#[cfg(any(windows, test))]
impl<P: AsyncRead + Unpin> AsyncRead for Overlapped<P> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
    }
}

#[cfg(any(windows, test))]
impl<P: AsyncRead + CancelRead + Unpin> ReadNow for Overlapped<P> {
    fn poll_read_now(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let pipe = &mut self.get_mut().0;
        // Each read after the first is a new one, scheduled when the
        // previous one was handed over.
        pipe.cancel_read();
        match Pin::new(pipe).poll_read(cx, buf) {
            Poll::Ready(Err(error)) if error.raw_os_error() == Some(READ_CANCELLED) => {
                Poll::Ready(Ok(()))
            }
            read => read,
        }
    }
}

#[cfg(windows)]
impl CancelRead for tokio::net::windows::named_pipe::NamedPipeServer {
    fn cancel_read(&mut self) {
        use std::os::windows::io::AsRawHandle;
        // SAFETY: the handle is this pipe's and stays open while `self`
        // lives. A null OVERLAPPED cancels the I/O this process issued on
        // the handle: the reactor's one read (the pipe is inbound only and
        // already connected). With nothing in flight the call fails with
        // ERROR_NOT_FOUND and changes nothing.
        unsafe {
            windows_sys::Win32::System::IO::CancelIoEx(self.as_raw_handle(), std::ptr::null());
        }
    }
}

/// A server's stderr read to EOF or, once the server has exited, to the end
/// of what the pipe holds then.
pub(super) struct UntilExit<R> {
    pipe: R,
    /// Fires when the server has exited. `None` once it fired, or once its
    /// sender is gone without firing (the drain then reads to EOF).
    on_exit: Option<oneshot::Receiver<()>>,
    exited: bool,
}

impl<R> UntilExit<R> {
    pub(super) fn new(pipe: R, on_exit: oneshot::Receiver<()>) -> Self {
        Self {
            pipe,
            on_exit: Some(on_exit),
            exited: false,
        }
    }
}

impl<R: AsyncRead + ReadNow + Unpin> AsyncRead for UntilExit<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if !this.exited {
            if let Poll::Ready(read) = Pin::new(&mut this.pipe).poll_read(cx, buf) {
                return Poll::Ready(read);
            }
            let Some(on_exit) = &mut this.on_exit else {
                return Poll::Pending;
            };
            let Poll::Ready(fired) = Pin::new(on_exit).poll(cx) else {
                return Poll::Pending;
            };
            this.on_exit = None;
            if fired.is_err() {
                return Poll::Pending;
            }
            this.exited = true;
        }
        Pin::new(&mut this.pipe).poll_read_now(cx, buf)
    }
}

/// Drain the server's stderr into the bounded ring until EOF (for a server
/// that exited, [`UntilExit`]'s end) or an I/O error. It must keep reading whatever the bytes are: a stopped drain lets
/// the pipe fill and blocks the server. Each line is capped in memory at
/// [`STDERR_LINE_MAX_BYTES`] and decoded lossily.
pub(super) async fn drain_stderr<R: AsyncRead + Unpin>(
    stderr: R,
    stderr_lines: Arc<Mutex<VecDeque<String>>>,
) {
    let mut reader = BufReader::new(stderr);
    let mut line = Vec::with_capacity(256);
    loop {
        line.clear();
        match read_capped_line(&mut reader, &mut line, STDERR_LINE_MAX_BYTES).await {
            Ok((0, _)) | Err(_) => return,
            Ok((_, truncated)) => {
                let mut text = String::from_utf8_lossy(&line).into_owned();
                if truncated {
                    text.push_str("...");
                }
                push_stderr_line(&stderr_lines, text);
            }
        }
    }
}

/// Read one `\n`-terminated line, keeping at most `cap` bytes of it in
/// `line` and consuming (discarding) the rest. The terminator and a
/// preceding `\r` are not kept. Returns the bytes consumed (0 = EOF) and
/// whether the line was cut.
pub(super) async fn read_capped_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    line: &mut Vec<u8>,
    cap: usize,
) -> std::io::Result<(usize, bool)> {
    let mut consumed = 0;
    let mut truncated = false;
    loop {
        let available = reader.fill_buf().await?;
        if available.is_empty() {
            break;
        }
        let newline = available.iter().position(|&byte| byte == b'\n');
        let content = &available[..newline.unwrap_or(available.len())];
        let keep = content.len().min(cap.saturating_sub(line.len()));
        line.extend_from_slice(&content[..keep]);
        truncated |= keep < content.len();
        let used = newline.map_or(available.len(), |index| index + 1);
        reader.consume(used);
        consumed += used;
        if newline.is_some() {
            break;
        }
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok((consumed, truncated))
}

pub(super) fn push_stderr_line(stderr_lines: &Arc<Mutex<VecDeque<String>>>, line: String) {
    let mut lines = stderr_lines
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    while lines.len() >= STDERR_RING_CAPACITY {
        lines.pop_front();
    }
    lines.push_back(truncate_stderr_line(line));
}

fn truncate_stderr_line(line: String) -> String {
    if line.chars().count() <= STDERR_LINE_MAX_CHARS {
        return line;
    }
    let mut truncated = line.chars().take(STDERR_LINE_MAX_CHARS).collect::<String>();
    truncated.push_str("...");
    truncated
}
