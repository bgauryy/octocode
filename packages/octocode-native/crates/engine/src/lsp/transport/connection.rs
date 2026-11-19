//! One JSON-RPC connection to a language server: request/notify, the pending
//! map, the reader task, and the single writer task.
//!
//! Ownership and failure model:
//! * **One writer task** owns the server's stdin and receives complete encoded
//!   frames over a bounded queue. A caller's cancellation can never cut a frame
//!   in half. Each frame carries a stall window that starts when the writer
//!   takes it, not when it was queued: a write fails only when the server
//!   reads nothing for that window, and then the stream is broken, which
//!   fails the connection and closes the queue. Callers wait for queue space
//!   and for their frame's write while the server reads, however long the
//!   frames ahead take: a frame's place in the queue is never a fault, and
//!   only a server that reads nothing for the frame's window is.
//! * **One reader task** owns stdout. Replies to server→client requests are
//!   queued to the writer, never written inline; the reader waits for queue
//!   space only while the server reads its input, so a server that stops
//!   reading stdin cannot stall reads past a reply's stall window.
//! * **Failure is total**: EOF, a framing fault, a broken or stalled write, or
//!   a second consecutive timeout of a server that showed no activity marks
//!   the connection failed and fails every pending request at once (the
//!   pending map is taken, not iterated). Each such request's error names its
//!   own method, then the failure's first reason, which may be another
//!   request's.
//! * **One slow request is not a fault**: a request that uses its whole
//!   wait (the timeout, extended while the server makes progress) is
//!   cancelled with `$/cancelRequest` and the server is kept, so the next
//!   request does not pay a cold start. Only [`RETIRE_AFTER_TIMEOUTS`]
//!   timeouts that an idle server ended, with no answered request between
//!   them, retire the connection; a wait that the caller's deadline ended
//!   while the server still worked never counts.
//!   Both tasks install a [`FailOnExit`] guard, so every exit path, including
//!   panic and abort, fails the connection.
//! * **Cancel on drop**: a dropped `request` future removes its pending entry
//!   and queues `$/cancelRequest` (best effort, non-blocking).
//! * Dropping the connection aborts both tasks.

mod shared;
mod tasks;

use super::codec::encode_frame;
use super::partial::PartialResultStore;
use super::patience::{ActivityMark, CpuClock, RequestWait, ServerActivity, WaitBudget};
use super::progress::ProgressTracker;
use super::push_diagnostics::PushDiagnosticsStore;
use super::server_requests::ClientRequestContext;
use crate::error::{Error, ErrorKind, Result};
use crate::lsp::clock::LspClock;
use serde_json::{Value, json};
use shared::{CancelOnDrop, FailOnExit, Outbound, Shared, cancel_message};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use tasks::{read_loop, write_loop};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Duration;

/// Frames queued for the writer task. Bounded so a server that stops reading
/// stdin pushes back on callers instead of growing memory.
const WRITER_QUEUE_FRAMES: usize = 128;
/// Floor of a notification's stall window: how long the writer may see the
/// server read nothing of a small notification (initialized, exit, a tiny
/// didChange) before the stream counts as stuck.
const NOTIFY_BASE_DEADLINE_MS: u64 = 1_000;
/// Extra stall window granted per megabyte of notification payload, so a
/// server that reads a large `didOpen` body in bursts is not cut by the flat
/// floor.
const NOTIFY_MS_PER_MIB: u64 = 1_000;
/// Stall window of a timed-out request's `$/cancelRequest`: a server that
/// reads none of it for this long once the writer reaches it stopped reading
/// stdin, and the connection is retired.
const CANCEL_WRITE_WAIT_MS: u64 = 100;
/// Timeouts in a row that an idle server ended, with no answered request
/// between them, that retire the connection as wedged. The first one only
/// cancels its request.
const RETIRE_AFTER_TIMEOUTS: u32 = 2;

/// Stall window (in ms) of a notification of `payload_bytes`.
fn notify_stall_ms(payload_bytes: usize) -> u64 {
    let mib = (payload_bytes as u64) / (1024 * 1024);
    NOTIFY_BASE_DEADLINE_MS.saturating_add(mib.saturating_mul(NOTIFY_MS_PER_MIB))
}

/// `error`, a failure of the connection itself, as the error of the
/// `method` request it failed while the request waited: the request names
/// itself, and the connection's first failure, which may be another
/// request's, follows.
fn not_answered(method: &str, mut error: Error) -> Error {
    error.reason = format!("LSP request {method} was not answered: {}", error.reason);
    error
}

/// `error`, the failure the connection already had, as the refusal of
/// `what` (the operation refused, when known): the failure came earlier,
/// and its reason may name another request.
fn failed_earlier(what: Option<std::fmt::Arguments<'_>>, mut error: Error) -> Error {
    error.reason = match what {
        Some(what) => format!(
            "{what} was not sent: the LSP connection failed earlier: {}",
            error.reason
        ),
        None => format!("the LSP connection failed earlier: {}", error.reason),
    };
    error
}

pub(crate) struct JsonRpcConnection {
    shared: Arc<Shared>,
    next_id: AtomicU64,
    reader_task: JoinHandle<()>,
    writer_task: JoinHandle<()>,
}

impl Drop for JsonRpcConnection {
    fn drop(&mut self) {
        self.reader_task.abort();
        self.writer_task.abort();
    }
}

impl JsonRpcConnection {
    pub(crate) fn new<R, W>(
        reader: R,
        writer: W,
        context: ClientRequestContext,
        progress: Arc<ProgressTracker>,
        cpu_clock: Option<CpuClock>,
        clock: LspClock,
    ) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (outbound, outbound_rx) = mpsc::channel(WRITER_QUEUE_FRAMES);
        let shared = Arc::new(Shared {
            state: Shared::new_state(),
            outbound,
            push_diagnostics: PushDiagnosticsStore::new(),
            partial_results: PartialResultStore::default(),
            unparseable_frames: AtomicU64::new(0),
            consecutive_timeouts: AtomicU32::new(0),
            activity: ServerActivity::new(cpu_clock, Arc::clone(&progress), clock.clone()),
            file_watches: std::sync::Mutex::default(),
            clock,
            written_bytes: AtomicU64::new(0),
        });
        // The guards are created before spawning so they run even if a task is
        // aborted before its first poll.
        let writer_task = tokio::spawn(write_loop(
            writer,
            outbound_rx,
            FailOnExit::new(&shared, "JSON-RPC writer stopped"),
        ));
        let reader_task = tokio::spawn(read_loop(
            reader,
            FailOnExit::new(&shared, "LSP connection closed"),
            context,
            progress,
        ));
        Self {
            shared,
            next_id: AtomicU64::new(1),
            reader_task,
            writer_task,
        }
    }

    /// Cumulative CPU time of the server's process tree; `None` when unreadable.
    pub(crate) fn server_cpu_time(&self) -> Option<tokio::time::Duration> {
        self.shared.activity.cpu_time()
    }

    /// What the server has shown so far, for [`Self::worked_since`].
    pub(crate) fn activity_mark(&self) -> ActivityMark {
        self.shared.activity.mark()
    }

    /// Whether the server worked since `mark`: CPU time, a new work-done
    /// notice or any message (see `patience`).
    pub(crate) fn worked_since(&self, mark: &ActivityMark) -> bool {
        self.shared.activity.worked_since(mark)
    }

    /// `false` once the connection has failed (server exited, framing fault,
    /// broken write, or consecutive request timeouts retired it).
    pub(crate) fn is_alive(&self) -> bool {
        self.shared.is_alive()
    }

    /// `Err` once the connection has failed, saying that the failure came
    /// earlier: its reason may name another request.
    pub(crate) fn ensure_alive(&self) -> Result<()> {
        if self.is_alive() {
            Ok(())
        } else {
            Err(failed_earlier(None, self.shared.closed()))
        }
    }

    /// Retire a session whose state can no longer be used safely. The pool
    /// observes this before reuse and owns stopping and replacing the process.
    pub(crate) fn retire(&self, reason: &str) {
        self.shared.fail(reason);
    }

    /// Retire the connection because the server was killed for exceeding its
    /// memory cap: every pending and later request fails with `reason` as
    /// [`crate::error::ErrorKind::ResourceExhausted`]. Idempotent; the first
    /// reason wins.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub(crate) fn fail_resource_exhausted(&self, reason: &str) {
        self.shared.fail_with(Error::resource_exhausted(reason));
    }

    pub(crate) fn clear_push_diagnostics(&self, uri: &str) {
        self.shared.push_diagnostics.clear(uri);
    }

    /// See [`super::push_diagnostics::PushDiagnosticsStore::supersede`].
    pub(crate) fn supersede_push_diagnostics(&self, uri: &str) {
        self.shared.push_diagnostics.supersede(uri);
    }

    /// See [`super::push_diagnostics::PushDiagnosticsStore::supersede_all`].
    pub(crate) fn supersede_all_push_diagnostics(&self) {
        self.shared.push_diagnostics.supersede_all();
    }

    /// Whether the server registered for `workspace/didChangeWatchedFiles`
    /// (a `client/registerCapability` it sent after `initialize`).
    pub(crate) fn watches_files(&self) -> bool {
        !self
            .shared
            .file_watches
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    }

    /// The reports of `documents` (`uri`, minimum version), waiting while any
    /// lacks a fresh one and the server works: a window of `quiet_ms` in
    /// which it shows no sign of work (CPU time, a work-done notice, any
    /// message, a report included) ends the wait, and so does the caller's
    /// deadline (the [`WaitBudget`] ceiling); dropping the future (a cancel)
    /// ends it at once. Each entry is the document's fresh report, else the
    /// one a later change superseded, else `None`, read when the wait ends:
    /// a report the last window brought counts. `quiet_ms` 0 reads without
    /// waiting.
    pub(crate) async fn wait_for_push_diagnostics(
        &self,
        documents: &[(String, Option<i64>)],
        quiet_ms: u32,
    ) -> Vec<Option<Value>> {
        let store = &self.shared.push_diagnostics;
        if quiet_ms > 0 {
            let clock = &self.shared.clock;
            let quiet = Duration::from_millis(u64::from(quiet_ms));
            let budget = WaitBudget::new(quiet_ms, clock.now());
            loop {
                let mark = self.shared.activity.mark();
                let window = budget.capped(clock.deadline_after(quiet));
                if store.wait_for_all(documents, window, clock).await
                    || budget.is_spent(clock.now())
                    || !self.shared.activity.worked_since(&mark)
                {
                    break;
                }
            }
        }
        documents
            .iter()
            .map(|(uri, min_version)| store.report(uri, *min_version, true))
            .collect()
    }

    #[cfg(test)]
    fn unparseable_frames(&self) -> u64 {
        self.shared.unparseable_frames.load(Ordering::Relaxed)
    }

    /// Sends a request and waits for its response within `wait`: a base
    /// timeout in milliseconds (`u32`) or a shared [`WaitBudget`].
    ///
    /// Starvation policy (see `patience`): past the budget's base timeout
    /// the wait is extended in steps of a quarter of
    /// it, but only while the server shows progress (its process tree's CPU
    /// time grew, it sent a work-done notice or any other message), and never past the
    /// budget's ceiling (the caller's deadline under
    /// [`super::with_caller_deadline`], else twice the base; retries that
    /// pass one [`WaitBudget`] share it). A server with no progress times
    /// out at the base, so a cold server on an oversubscribed machine
    /// answers while a hung one still fails. Dropping the future cancels the request at any point of the
    /// wait.
    ///
    /// Timeout policy: a request that consumes its whole wait is cancelled
    /// with `$/cancelRequest` and fails with a timeout; the server is kept, so
    /// one slow walk on a large tree does not cost a cold start. The
    /// connection is retired as wedged only when the cancel cannot be
    /// written (the server stopped reading stdin) or on the
    /// [`RETIRE_AFTER_TIMEOUTS`]th timeout that an idle server ended with no
    /// answered request between: then every other pending request fails
    /// immediately and the pool restarts the server on the next acquire. A
    /// wait that its ceiling (the caller's deadline) ended while the server
    /// still worked says the server is slow, not wedged, and never counts.
    ///
    /// When the connection fails, before the request is sent or while it
    /// waits, the error names this request and then the failure's first
    /// reason, which may be another request's.
    pub(crate) async fn request(
        &self,
        method: &str,
        params: Value,
        wait: impl Into<RequestWait>,
    ) -> Result<Value> {
        let started = self.shared.clock.now();
        let budget = wait.into().budget(started);
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let frame =
            encode_frame(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        let response = self
            .shared
            .register(id)
            .map_err(|error| failed_earlier(Some(format_args!("LSP request {method}")), error))?;
        let _cancel_on_drop = CancelOnDrop {
            shared: &self.shared,
            id,
        };
        self.shared
            .enqueue(Outbound {
                frame,
                stall: budget.base,
                written: None,
            })
            .await
            .map_err(|error| not_answered(method, error))?;
        match self.shared.activity.wait(response, started, budget).await {
            Ok(Ok(result)) => result.map_err(|error| match error.kind() {
                ErrorKind::ConnectionClosed | ErrorKind::ResourceExhausted => {
                    not_answered(method, error)
                }
                _ => error,
            }),
            Ok(Err(_)) => Err(not_answered(method, self.shared.closed())),
            Err(unanswered) => {
                let cause = match unanswered.idle {
                    Some(idle) => format!(
                        "the server is running but showed no activity (CPU, progress or messages) in its last {}ms",
                        idle.as_millis()
                    ),
                    None => "the server was still working when the wait ended".to_owned(),
                };
                let reason = format!(
                    "LSP request {method} timed out after {}ms: {cause}",
                    unanswered.waited.as_millis()
                );
                let cancel_refused = self.cancel_abandoned(id).await;
                let timeouts = if unanswered.idle.is_some() {
                    self.shared
                        .consecutive_timeouts
                        .fetch_add(1, Ordering::AcqRel)
                        .saturating_add(1)
                } else {
                    self.shared.consecutive_timeouts.load(Ordering::Acquire)
                };
                if cancel_refused {
                    self.shared.fail(&format!(
                        "{reason}; LSP connection retired as wedged: the server stopped reading its input, so the cancel could not be written"
                    ));
                } else if timeouts >= RETIRE_AFTER_TIMEOUTS {
                    self.shared.fail(&format!(
                        "{reason}; LSP connection retired as wedged after {timeouts} consecutive timeouts of a server that showed no activity"
                    ));
                }
                Err(Error::timeout(reason))
            }
        }
    }

    /// Tells the server to stop computing the abandoned request `id` and
    /// waits until the `$/cancelRequest` is written, so it precedes any
    /// retirement: while the server reads the frames queued ahead, however
    /// long they take. `true` when the cancel cannot be written (the queue
    /// is full, or the server read nothing for the cancel's stall window),
    /// so the server stopped reading stdin; `false` when it was written or
    /// nothing was pending.
    async fn cancel_abandoned(&self, id: u64) -> bool {
        if self.shared.take_pending(id).is_none() {
            return false;
        }
        let Ok(frame) = encode_frame(&cancel_message(id)) else {
            return false;
        };
        let (tx, rx) = oneshot::channel();
        let stall = Duration::from_millis(CANCEL_WRITE_WAIT_MS);
        let queued = self.shared.outbound.try_send(Outbound {
            frame,
            stall,
            written: Some(tx),
        });
        match queued {
            Ok(()) => !matches!(self.shared.while_reading(stall, rx).await, Some(Ok(Ok(())))),
            Err(_) => true,
        }
    }

    /// [`Self::request`] with a `partialResultToken`; streamed `$/progress`
    /// chunks are merged ahead of the final result.
    pub(crate) async fn request_with_partials(
        &self,
        method: &str,
        mut params: Value,
        wait: impl Into<RequestWait>,
    ) -> Result<Value> {
        let token = format!(
            "octocode-partial-{}",
            self.next_id.fetch_add(1, Ordering::Relaxed)
        );
        let Some(object) = params.as_object_mut() else {
            return Err(Error::new("LSP partial-result params must be an object"));
        };
        object.insert(
            "partialResultToken".to_owned(),
            Value::String(token.clone()),
        );
        // The guard discards the collected chunks if this future is dropped.
        let partials = self.shared.partial_results.begin(token);
        let result = self.request(method, params, wait).await?;
        partials.finish(result)
    }

    /// Sends a notification and waits until it is flushed. Refused once the
    /// connection has failed.
    pub(crate) async fn notify(&self, method: &str, params: Value) -> Result<()> {
        if !self.is_alive() {
            return Err(failed_earlier(
                Some(format_args!("LSP notification {method}")),
                self.shared.closed(),
            ));
        }
        self.notify_best_effort(method, params).await
    }

    /// Sends a notification even when the connection is already failed (for
    /// example `exit` after a timed-out `shutdown`), as long as the writer is
    /// still healthy. Waits while frames queued ahead are written; the write
    /// fails only when the server reads nothing of it for its stall window,
    /// which scales with payload size so a large `didOpen` gets proportional
    /// headroom.
    pub(crate) async fn notify_best_effort(&self, method: &str, params: Value) -> Result<()> {
        let frame = encode_frame(&json!({"jsonrpc":"2.0","method":method,"params":params}))?;
        let stall = Duration::from_millis(notify_stall_ms(frame.len()));
        self.shared.write_frame(frame, stall).await
    }
}

#[cfg(test)]
mod tests;
