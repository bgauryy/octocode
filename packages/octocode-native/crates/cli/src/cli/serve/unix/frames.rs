//! Frames: what a server says on the socket while a call runs.
//!
//! A reply line is the call as printed (a CLI call) or as an MCP result (an
//! MCP call: [`MCP`], [`MCP_ERROR`]). Before it, the server may write any
//! number of frame lines: `{"progress":"<line>"}`, one per progress line
//! the call raised ([`octocode_io::notice::progress_sink`]) in the order
//! raised, and `{"alive":true}` each [`HEARTBEAT`] the call runs without
//! another frame. The client prints each progress line on stderr as it
//! arrives, as an in-process call does, takes any frame as the server being
//! alive, and ends on the first line that is not a frame. A server and
//! client of different builds never meet: the socket name hashes the
//! executable's identity (`exe_identity`), so a client that knows frames
//! reaches a server that sends them.
//!
//! One writer ([`Frames`]) owns a connection's write side, so no line is
//! cut by another, and the reply follows every progress line taken before
//! it. Writing never holds up the call: the call is polled while a line
//! waits for the client, and a client that reads slower than the call
//! raises progress gets the newest [`BACKLOG`] lines (progress is advisory;
//! the newest says where the call is). A client that takes no byte for
//! [`READ_STALL`] is gone, as one that closed.

use octocode_io::http::READ_STALL;
use octocode_io::notice::Scope;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::time::Instant;

/// The key of a progress frame.
pub(super) const PROGRESS: &str = "progress";

/// The key of a heartbeat frame: the server is alive and the call runs.
pub(super) const ALIVE: &str = "alive";

/// The key of an MCP call's reply: `{"mcp":{"text","isError"}}`.
pub(super) const MCP: &str = "mcp";

/// The key of the reply to an MCP call that is a protocol error (cancelled,
/// closing): `{"mcpError":{"code","message"}}`.
pub(super) const MCP_ERROR: &str = "mcpError";

/// The key of a request that asks the server to warm an MCP session's
/// workspace: `{"prewarm":true,...}`, answered `{"prewarmed":true}`.
pub(super) const PREWARM: &str = "prewarm";

/// How long a running call goes without a frame: a tenth of the
/// [`READ_STALL`] a client waits through, so a long call that says nothing
/// is never taken for a dead server.
pub(super) const HEARTBEAT: Duration = match READ_STALL.checked_div(10) {
    Some(beat) => beat,
    None => READ_STALL,
};

/// Progress lines kept for a client that has not taken them yet; past it the
/// oldest go.
pub(super) const BACKLOG: usize = 64;

/// When a connection says it is alive, and how long a write waits for the
/// client to take a byte.
#[derive(Clone, Copy, Debug)]
pub(super) struct Pace {
    pub(super) heartbeat: Duration,
    pub(super) silence: Duration,
}

impl Pace {
    /// A server's: [`HEARTBEAT`] and [`READ_STALL`].
    pub(super) const SERVED: Self = Self {
        heartbeat: HEARTBEAT,
        silence: READ_STALL,
    };
}

/// The progress lines a call raised that its client has not been sent, at
/// most [`BACKLOG`]: the call's threads push, the connection's writer takes.
#[derive(Default)]
pub(super) struct Backlog {
    lines: Mutex<VecDeque<String>>,
    raised: tokio::sync::Notify,
}

impl Backlog {
    fn push(&self, line: &str) {
        let mut lines = self.lines.lock().unwrap_or_else(PoisonError::into_inner);
        if lines.len() >= BACKLOG {
            lines.pop_front();
        }
        lines.push_back(line.to_owned());
        drop(lines);
        self.raised.notify_one();
    }

    fn take(&self) -> Option<String> {
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
    }
}

/// A scope for one call whose progress lines queue on the returned
/// backlog, for [`Frames::run`] to write.
pub(super) fn progress_scope() -> (Scope, Arc<Backlog>) {
    let backlog = Arc::new(Backlog::default());
    let queue = Arc::clone(&backlog);
    let scope = Scope::with_progress(move |line| queue.push(line));
    (scope, backlog)
}

/// The client is gone: a write failed, or it took no byte for the pace's
/// silence.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Gone;

/// One connection's write side: whole lines, one at a time.
pub(super) struct Frames<W> {
    write: W,
    pace: Pace,
    /// The line being written, and how much of it the client has taken.
    line: Vec<u8>,
    sent: usize,
}

/// What woke [`Frames::run`].
enum Woke<T> {
    Done(T),
    Wrote(Result<(), Gone>),
    Stalled,
    Raised,
    Beat,
}

impl<W: AsyncWrite + Unpin> Frames<W> {
    pub(super) const fn new(write: W, pace: Pace) -> Self {
        Self {
            write,
            pace,
            line: Vec::new(),
            sent: 0,
        }
    }

    const fn busy(&self) -> bool {
        self.sent < self.line.len()
    }

    fn start(&mut self, value: &serde_json::Value) {
        self.line = value.to_string().into_bytes();
        self.line.push(b'\n');
        self.sent = 0;
    }

    /// Hand the client more of the line in flight. Cancel-safe: a write that
    /// does not complete took nothing.
    async fn advance(&mut self) -> Result<(), Gone> {
        let rest = self.line.get(self.sent..).unwrap_or_default();
        match self.write.write(rest).await {
            Ok(0) | Err(_) => Err(Gone),
            Ok(taken) => {
                self.sent += taken;
                Ok(())
            }
        }
    }

    /// Finish the line in flight, each wait for the client within the
    /// pace's silence.
    async fn finish(&mut self) -> Result<(), Gone> {
        while self.busy() {
            let stall = Instant::now() + self.pace.silence;
            tokio::time::timeout_at(stall, self.advance())
                .await
                .map_err(|_| Gone)??;
        }
        self.write.flush().await.map_err(|_| Gone)
    }

    /// Write `value` as one line, after the line in flight.
    pub(super) async fn send(&mut self, value: &serde_json::Value) -> Result<(), Gone> {
        self.finish().await?;
        self.start(value);
        self.finish().await
    }

    /// Finish the line in flight and write the progress lines left in
    /// `backlog`: what the reply follows.
    pub(super) async fn drain(&mut self, backlog: &Backlog) -> Result<(), Gone> {
        self.finish().await?;
        while let Some(line) = backlog.take() {
            self.send(&serde_json::json!({ PROGRESS: line })).await?;
        }
        Ok(())
    }

    /// Poll `work` to its end while `backlog`'s lines and heartbeats go to
    /// the client; a write never holds up the work. A line may still be in
    /// flight when the work ends: [`Self::drain`] finishes it. [`Gone`]
    /// leaves the work unfinished.
    pub(super) async fn run<F: Future>(
        &mut self,
        mut work: std::pin::Pin<&mut F>,
        backlog: &Backlog,
    ) -> Result<F::Output, Gone> {
        let pace = self.pace;
        let mut beat = tokio::time::interval_at(Instant::now() + pace.heartbeat, pace.heartbeat);
        beat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut stall = Instant::now();
        loop {
            if !self.busy()
                && let Some(line) = backlog.take()
            {
                self.start(&serde_json::json!({ PROGRESS: line }));
                stall = Instant::now() + pace.silence;
            }
            let busy = self.busy();
            let woke = tokio::select! {
                biased;
                done = work.as_mut() => Woke::Done(done),
                wrote = self.advance(), if busy => Woke::Wrote(wrote),
                () = tokio::time::sleep_until(stall), if busy => Woke::Stalled,
                () = backlog.raised.notified(), if !busy => Woke::Raised,
                _ = beat.tick() => Woke::Beat,
            };
            match woke {
                Woke::Done(done) => return Ok(done),
                Woke::Wrote(wrote) => {
                    wrote?;
                    stall = Instant::now() + pace.silence;
                }
                Woke::Stalled => return Err(Gone),
                Woke::Raised => {}
                // A line in flight already says the server is alive.
                Woke::Beat if !self.busy() => {
                    self.start(&serde_json::json!({ ALIVE: true }));
                    stall = Instant::now() + pace.silence;
                }
                Woke::Beat => {}
            }
        }
    }
}
