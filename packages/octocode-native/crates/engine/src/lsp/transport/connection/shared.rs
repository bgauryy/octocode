//! State the connection handle, reader task, and writer task share, plus the
//! guards that fail the connection or cancel a request on every exit path.

use super::super::codec::encode_frame;
use super::super::partial::PartialResultStore;
use super::super::patience::ServerActivity;
use super::super::push_diagnostics::PushDiagnosticsStore;
use super::NOTIFY_BASE_DEADLINE_MS;
use crate::error::{Error, Result};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};
use tokio::time::{Duration, Instant};

type PendingSender = oneshot::Sender<Result<Value>>;

/// One complete frame for the writer task.
pub(super) struct Outbound {
    pub(super) frame: Vec<u8>,
    pub(super) deadline: Instant,
    /// Completed once the frame is flushed (or the write failed).
    pub(super) written: Option<oneshot::Sender<Result<()>>>,
}

/// Whether the connection still serves requests.
pub(super) enum ConnState {
    /// Serving: the waiter of each request still pending, by id.
    Open(HashMap<u64, PendingSender>),
    /// Failed, for this reason (the first wins): new requests are refused
    /// with it and every waiter has already been failed.
    Failed(Error),
}

/// State shared by the handle, the reader task, and the writer task.
pub(super) struct Shared {
    pub(super) state: Mutex<ConnState>,
    pub(super) outbound: mpsc::Sender<Outbound>,
    pub(super) push_diagnostics: Arc<PushDiagnosticsStore>,
    pub(super) partial_results: PartialResultStore,
    /// Frames whose body was not valid JSON (skipped). A spike means the
    /// stream lost its framing.
    pub(super) unparseable_frames: AtomicU64,
    /// Request timeouts since the server last answered a pending request.
    pub(super) consecutive_timeouts: AtomicU32,
    /// The server's progress signals, which extend a request's wait.
    pub(super) activity: ServerActivity,
    /// The server registered for `workspace/didChangeWatchedFiles`
    /// (`client/registerCapability`), so it expects file events for files
    /// it has not opened.
    pub(super) watches_files: AtomicBool,
}

impl Shared {
    pub(super) fn new_state() -> Mutex<ConnState> {
        Mutex::new(ConnState::Open(HashMap::new()))
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ConnState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn is_alive(&self) -> bool {
        matches!(*self.state(), ConnState::Open(_))
    }

    /// Registers a waiter for `id`; refused once the connection has failed.
    pub(super) fn register(&self, id: u64) -> Result<oneshot::Receiver<Result<Value>>> {
        let (tx, rx) = oneshot::channel();
        match &mut *self.state() {
            ConnState::Open(pending) => {
                pending.insert(id, tx);
                Ok(rx)
            }
            ConnState::Failed(error) => Err(error.clone()),
        }
    }

    /// Removes and returns the waiter for `id`, if it is still pending.
    pub(super) fn take_pending(&self, id: u64) -> Option<PendingSender> {
        match &mut *self.state() {
            ConnState::Open(pending) => pending.remove(&id),
            ConnState::Failed(_) => None,
        }
    }

    /// Marks the connection failed and fails every pending request with
    /// `reason`. Idempotent; the first reason wins for the drained waiters.
    pub(super) fn fail(&self, reason: &str) {
        self.fail_with(Error::connection_closed(reason));
    }

    /// [`Self::fail`] with the typed cause every drained or later request
    /// reports.
    pub(super) fn fail_with(&self, error: Error) {
        let drained = {
            let mut state = self.state();
            let ConnState::Open(pending) = &mut *state else {
                return;
            };
            let drained = std::mem::take(pending);
            *state = ConnState::Failed(error.clone());
            drained
        };
        for sender in drained.into_values() {
            let _ = sender.send(Err(error.clone()));
        }
    }

    /// Queues a frame without waiting. Used from `Drop` (cancel on drop).
    pub(super) fn try_enqueue(&self, frame: Vec<u8>, deadline: Instant) {
        let _ = self.outbound.try_send(Outbound {
            frame,
            deadline,
            written: None,
        });
    }

    /// Queues a frame, waiting for queue space until `deadline`.
    pub(super) async fn enqueue(&self, outbound: Outbound, deadline: Instant) -> Result<()> {
        match tokio::time::timeout_at(deadline, self.outbound.send(outbound)).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(self.closed()),
            Err(_) => {
                let reason = "LSP request write timed out: server is not reading stdin";
                self.fail(reason);
                Err(Error::timeout(reason))
            }
        }
    }

    /// Queues a frame and waits until it is flushed, both bounded by `deadline`.
    pub(super) async fn write_frame(&self, frame: Vec<u8>, deadline: Instant) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.enqueue(
            Outbound {
                frame,
                deadline,
                written: Some(tx),
            },
            deadline,
        )
        .await?;
        match tokio::time::timeout_at(deadline, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(closed_error()),
            Err(_) => {
                // The writer task enforces the same deadline and breaks the
                // stream if the frame is stuck mid-write.
                let reason = "LSP notification write timed out";
                self.fail(reason);
                Err(Error::timeout(reason))
            }
        }
    }

    /// The error for a request refused because the connection failed.
    pub(super) fn closed(&self) -> Error {
        match &*self.state() {
            ConnState::Failed(error) => error.clone(),
            ConnState::Open(_) => closed_error(),
        }
    }

    pub(super) fn closed_reason(&self, reason: &str) -> String {
        match self.unparseable_frames.load(Ordering::Relaxed) {
            0 => reason.to_owned(),
            skipped => format!("{reason} ({skipped} unparseable frame(s) skipped)"),
        }
    }
}

pub(super) fn closed_error() -> Error {
    Error::connection_closed("JSON-RPC connection closed")
}

/// Fails the connection when dropped, on every exit path of a task (return,
/// break, panic, or abort).
pub(super) struct FailOnExit {
    pub(super) shared: Arc<Shared>,
    pub(super) reason: String,
}

impl FailOnExit {
    pub(super) fn new(shared: &Arc<Shared>, reason: &str) -> Self {
        Self {
            shared: Arc::clone(shared),
            reason: reason.to_owned(),
        }
    }
}

impl Drop for FailOnExit {
    fn drop(&mut self) {
        let reason = self.shared.closed_reason(&self.reason);
        self.shared.fail(&reason);
    }
}

/// Armed for the lifetime of a `request` future. If the future is dropped
/// while the request is still pending, the entry is removed and
/// `$/cancelRequest` is queued. A request that already completed (or a
/// connection that already failed) has no entry, so nothing is sent.
pub(super) struct CancelOnDrop<'a> {
    pub(super) shared: &'a Shared,
    pub(super) id: u64,
}

impl Drop for CancelOnDrop<'_> {
    fn drop(&mut self) {
        if self.shared.take_pending(self.id).is_some()
            && let Ok(frame) = encode_frame(&cancel_message(self.id))
        {
            let deadline = Instant::now() + Duration::from_millis(NOTIFY_BASE_DEADLINE_MS);
            self.shared.try_enqueue(frame, deadline);
        }
    }
}

pub(super) fn cancel_message(id: u64) -> Value {
    json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}})
}
