//! The server's stderr drain into a bounded ring of capped lines.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader, ReadBuf};
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

pub(super) fn spawn_stderr_reader(
    stderr: ChildStderr,
    stderr_lines: Arc<Mutex<VecDeque<String>>>,
) -> StderrDrain {
    let (exited, on_exit) = oneshot::channel();
    let task = tokio::spawn(drain_stderr(UntilExit::new(stderr, on_exit), stderr_lines));
    StderrDrain { task, exited }
}

/// What a pipe holds once the server has exited, found without waiting.
pub(super) enum Held {
    /// Bytes read from it straight away (0: EOF).
    #[cfg_attr(
        all(not(unix), not(test)),
        expect(dead_code, reason = "only a Unix pipe is read without waiting")
    )]
    Read(usize),
    /// Bytes the stream's own read delivers (Windows reads the pipe on a
    /// blocking thread, whose read in flight takes them), or an EOF that
    /// follows because no writer is left.
    #[cfg_attr(
        all(unix, not(test)),
        expect(dead_code, reason = "a Unix pipe is read without waiting")
    )]
    InStream,
    /// Nothing, while a writer still holds the pipe open.
    Nothing,
}

/// A pipe's bytes found without waiting on its writers.
pub(super) trait ReadNow {
    fn read_now(&mut self, buf: &mut [u8]) -> std::io::Result<Held>;
}

impl ReadNow for ChildStderr {
    #[cfg(unix)]
    fn read_now(&mut self, buf: &mut [u8]) -> std::io::Result<Held> {
        use std::io::Read;
        use std::os::fd::AsFd;
        // The duplicate shares the pipe's open file, so its non-blocking mode.
        let read = self
            .as_fd()
            .try_clone_to_owned()
            .map(std::fs::File::from)
            .and_then(|mut pipe| pipe.read(buf));
        match read {
            Ok(read) => Ok(Held::Read(read)),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(Held::Nothing),
            Err(error) => Err(error),
        }
    }

    /// The pipe is read on a blocking thread, so this only peeks at how many
    /// bytes it holds; a failed peek (every writer gone) waits for the EOF.
    #[cfg(windows)]
    fn read_now(&mut self, _buf: &mut [u8]) -> std::io::Result<Held> {
        use std::os::windows::io::AsRawHandle;
        let mut available = 0_u32;
        // SAFETY: the handle is this pipe's and stays open while `self`
        // lives. With a null buffer of size 0 the call reads no bytes and
        // writes only the count of bytes available into `available`.
        let peeked = unsafe {
            windows_sys::Win32::System::Pipes::PeekNamedPipe(
                self.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &raw mut available,
                std::ptr::null_mut(),
            )
        };
        Ok(if peeked != 0 && available == 0 {
            Held::Nothing
        } else {
            Held::InStream
        })
    }

    #[cfg(not(any(unix, windows)))]
    fn read_now(&mut self, _buf: &mut [u8]) -> std::io::Result<Held> {
        Ok(Held::InStream)
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
        loop {
            match this.pipe.read_now(buf.initialize_unfilled()) {
                Ok(Held::Read(read)) => {
                    buf.advance(read);
                    return Poll::Ready(Ok(()));
                }
                Ok(Held::InStream) => return Pin::new(&mut this.pipe).poll_read(cx, buf),
                // Nothing is left of what the server wrote: a read that
                // already finished is delivered, and then its stderr ends.
                Ok(Held::Nothing) => {
                    return match Pin::new(&mut this.pipe).poll_read(cx, buf) {
                        Poll::Ready(read) => Poll::Ready(read),
                        Poll::Pending => Poll::Ready(Ok(())),
                    };
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
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
