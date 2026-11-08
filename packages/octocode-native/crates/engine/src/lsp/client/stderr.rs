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

/// A pipe's bytes read without waiting. `None` where the pipe cannot be read
/// so; `WouldBlock` when it holds nothing while a writer still has it open.
pub(super) trait ReadNow {
    fn read_now(&mut self, buf: &mut [u8]) -> Option<std::io::Result<usize>>;
}

impl ReadNow for ChildStderr {
    #[cfg(unix)]
    fn read_now(&mut self, buf: &mut [u8]) -> Option<std::io::Result<usize>> {
        use std::io::Read;
        use std::os::fd::AsFd;
        // The duplicate shares the pipe's open file, so its non-blocking mode.
        Some(
            self.as_fd()
                .try_clone_to_owned()
                .map(std::fs::File::from)
                .and_then(|mut pipe| pipe.read(buf)),
        )
    }

    /// The Job Object ends every process of the tree, so EOF comes.
    #[cfg(not(unix))]
    fn read_now(&mut self, _buf: &mut [u8]) -> Option<std::io::Result<usize>> {
        None
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
        if let Poll::Ready(read) = Pin::new(&mut this.pipe).poll_read(cx, buf) {
            return Poll::Ready(read);
        }
        if let Some(on_exit) = &mut this.on_exit {
            let Poll::Ready(fired) = Pin::new(on_exit).poll(cx) else {
                return Poll::Pending;
            };
            this.exited = fired.is_ok();
            this.on_exit = None;
        }
        if !this.exited {
            return Poll::Pending;
        }
        loop {
            match this.pipe.read_now(buf.initialize_unfilled()) {
                None => return Poll::Pending,
                Some(Ok(read)) => {
                    buf.advance(read);
                    return Poll::Ready(Ok(()));
                }
                // Nothing is left of what the server wrote: its stderr ends.
                Some(Err(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    return Poll::Ready(Ok(()));
                }
                Some(Err(error)) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Some(Err(error)) => return Poll::Ready(Err(error)),
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
