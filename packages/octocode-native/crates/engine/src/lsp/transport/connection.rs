//! One JSON-RPC connection to a language server: request/notify, the pending
//! map, the reader task, and the single writer task.
//!
//! Ownership and failure model:
//! * **One writer task** owns the server's stdin and receives complete encoded
//!   frames over a bounded queue. A caller's cancellation can never cut a frame
//!   in half; each frame carries its own deadline, and a write that misses it
//!   (or fails) breaks the stream, which fails the connection.
//! * **One reader task** owns stdout. Replies to server→client requests are
//!   queued to the writer (bounded wait), never written inline, so a server
//!   that stops reading stdin cannot stall reads.
//! * **Failure is total**: EOF, a framing fault, a broken write, a queue that
//!   stays full, or a second consecutive request timeout marks the connection
//!   failed and fails every pending request at once (the pending map is taken,
//!   not iterated).
//! * **One slow request is not a fault**: a request that uses its whole
//!   wait (the timeout, extended while the server makes progress) is
//!   cancelled with `$/cancelRequest` and the server is kept, so the next
//!   request does not pay a cold start. Only [`RETIRE_AFTER_TIMEOUTS`]
//!   timeouts with no answered request between them retire the connection.
//!   Both tasks install a [`FailOnExit`] guard, so every exit path, including
//!   panic and abort, fails the connection.
//! * **Cancel on drop**: a dropped `request` future removes its pending entry
//!   and queues `$/cancelRequest` (best effort, non-blocking).
//! * Dropping the connection aborts both tasks.

mod shared;
mod tasks;

use super::codec::encode_frame;
use super::partial::PartialResultStore;
use super::patience::{CpuClock, ServerActivity, WaitBudget};
use super::progress::ProgressTracker;
use super::push_diagnostics::PushDiagnosticsStore;
use super::server_requests::ClientRequestContext;
use crate::error::{Error, Result};
use serde_json::{Value, json};
use shared::{CancelOnDrop, FailOnExit, Outbound, Shared, cancel_message, closed_error};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tasks::{read_loop, write_loop};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Duration, Instant};

/// Frames queued for the writer task. Bounded so a server that stops reading
/// stdin pushes back on callers instead of growing memory.
const WRITER_QUEUE_FRAMES: usize = 128;
/// Floor for a notification's write deadline. A small notification (initialized,
/// exit, a tiny didChange) must complete within this window.
const NOTIFY_BASE_DEADLINE_MS: u64 = 1_000;
/// Extra write budget granted per megabyte of notification payload, so a large
/// `didOpen` body is not guillotined by the flat floor.
const NOTIFY_MS_PER_MIB: u64 = 1_000;
/// How long a timed-out request waits for its `$/cancelRequest` to be written.
const CANCEL_WRITE_WAIT_MS: u64 = 100;
/// Request timeouts in a row, with no answered request between them, that
/// retire the connection as wedged. The first one only cancels its request.
const RETIRE_AFTER_TIMEOUTS: u32 = 2;

/// Write deadline (in ms) for a notification of `payload_bytes`.
fn notify_write_deadline_ms(payload_bytes: usize) -> u64 {
    let mib = (payload_bytes as u64) / (1024 * 1024);
    NOTIFY_BASE_DEADLINE_MS.saturating_add(mib.saturating_mul(NOTIFY_MS_PER_MIB))
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
    ) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (outbound, outbound_rx) = mpsc::channel(WRITER_QUEUE_FRAMES);
        let shared = Arc::new(Shared {
            pending: Mutex::new(Some(HashMap::new())),
            failed: AtomicBool::new(false),
            failure_reason: Mutex::new(None),
            outbound,
            push_diagnostics: PushDiagnosticsStore::new(),
            partial_results: PartialResultStore::default(),
            unparseable_frames: AtomicU64::new(0),
            consecutive_timeouts: AtomicU32::new(0),
            activity: ServerActivity::new(cpu_clock, Arc::clone(&progress)),
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

    /// `false` once the connection has failed (server exited, framing fault,
    /// broken write, or consecutive request timeouts retired it).
    pub(crate) fn is_alive(&self) -> bool {
        self.shared.is_alive()
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

    pub(crate) async fn wait_for_push_diagnostics(
        &self,
        uri: &str,
        timeout_ms: u32,
        min_version: Option<i64>,
    ) -> Option<Value> {
        self.shared
            .push_diagnostics
            .wait_for(uri, timeout_ms, min_version)
            .await
    }

    #[cfg(test)]
    fn unparseable_frames(&self) -> u64 {
        self.shared.unparseable_frames.load(Ordering::Relaxed)
    }

    /// Sends a request and waits for its response within `budget`: a base
    /// timeout in milliseconds (`u32`) or a shared [`WaitBudget`].
    ///
    /// Starvation policy (see `patience`): past the budget's base timeout
    /// the wait is extended in steps of a quarter of
    /// it, but only while the server shows progress (its process tree's CPU
    /// time grew, or a `$/progress` token is open), and never past the
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
    /// written in time (the server stopped reading stdin) or on the
    /// [`RETIRE_AFTER_TIMEOUTS`]th timeout with no answered request between:
    /// then every other pending request fails immediately and the pool
    /// restarts the server on the next acquire.
    pub(crate) async fn request(
        &self,
        method: &str,
        params: Value,
        budget: impl Into<WaitBudget>,
    ) -> Result<Value> {
        let budget = budget.into();
        let started = Instant::now();
        let deadline = started + budget.base;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let frame =
            encode_frame(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        let response = self.shared.register(id)?;
        let _cancel_on_drop = CancelOnDrop {
            shared: &self.shared,
            id,
        };
        self.shared
            .enqueue(
                Outbound {
                    frame,
                    deadline,
                    written: None,
                },
                deadline,
            )
            .await?;
        match self.shared.activity.wait(response, started, budget).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(closed_error()),
            Err(waited) => {
                let reason = format!("LSP request timed out after {}ms", waited.as_millis());
                // `true` when the cancel could not be written in time: the
                // writer queue is full or the pipe does not drain, so the
                // server stopped reading stdin.
                let mut cancel_refused = false;
                if self.shared.take_pending(id).is_some()
                    && let Ok(frame) = encode_frame(&cancel_message(id))
                {
                    // Tell the server to stop computing the abandoned request,
                    // briefly waiting so the cancel precedes any retirement.
                    let (tx, rx) = oneshot::channel();
                    let cancel_deadline =
                        Instant::now() + Duration::from_millis(CANCEL_WRITE_WAIT_MS);
                    let queued = self.shared.outbound.try_send(Outbound {
                        frame,
                        deadline: cancel_deadline,
                        written: Some(tx),
                    });
                    cancel_refused = match queued {
                        Ok(()) => !matches!(
                            tokio::time::timeout_at(cancel_deadline, rx).await,
                            Ok(Ok(Ok(())))
                        ),
                        Err(_) => true,
                    };
                }
                let timeouts = self
                    .shared
                    .consecutive_timeouts
                    .fetch_add(1, Ordering::AcqRel)
                    .saturating_add(1);
                if cancel_refused || timeouts >= RETIRE_AFTER_TIMEOUTS {
                    self.shared.fail(&format!(
                        "{reason}; LSP connection retired as wedged after {timeouts} consecutive timeout(s)"
                    ));
                }
                Err(Error::timeout(reason))
            }
        }
    }

    /// [`Self::request`] with a `partialResultToken`; streamed `$/progress`
    /// chunks are merged ahead of the final result.
    pub(crate) async fn request_with_partials(
        &self,
        method: &str,
        mut params: Value,
        budget: impl Into<WaitBudget>,
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
        let result = self.request(method, params, budget).await?;
        partials.finish(result)
    }

    /// Sends a notification and waits until it is flushed. Refused once the
    /// connection has failed.
    pub(crate) async fn notify(&self, method: &str, params: Value) -> Result<()> {
        if !self.is_alive() {
            return Err(self.shared.closed());
        }
        self.notify_best_effort(method, params).await
    }

    /// Sends a notification even when the connection is already failed (for
    /// example `exit` after a timed-out `shutdown`), as long as the writer is
    /// still healthy. The write deadline scales with payload size so a large
    /// `didOpen` gets proportional headroom.
    pub(crate) async fn notify_best_effort(&self, method: &str, params: Value) -> Result<()> {
        let frame = encode_frame(&json!({"jsonrpc":"2.0","method":method,"params":params}))?;
        let deadline =
            Instant::now() + Duration::from_millis(notify_write_deadline_ms(frame.len()));
        self.shared.write_frame(frame, deadline).await
    }
}

#[cfg(test)]
mod tests;
