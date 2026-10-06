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
//!   stays full, or a request timeout marks the connection failed and fails
//!   every pending request at once (the pending map is taken, not iterated).
//!   Both tasks install a [`FailOnExit`] guard, so every exit path, including
//!   panic and abort, fails the connection.
//! * **Cancel on drop**: a dropped `request` future removes its pending entry
//!   and queues `$/cancelRequest` (best effort, non-blocking).
//! * Dropping the connection aborts both tasks.

use super::codec::{Frame, encode_frame, read_frame};
use super::partial::PartialResultStore;
use super::progress::ProgressTracker;
use super::push_diagnostics::PushDiagnosticsStore;
use super::server_requests::{ClientRequestContext, client_response_for};
use crate::error::{Error, Result, RpcError};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
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
/// How long the reader may wait for writer-queue space to reply to a server
/// request before declaring the server wedged (it is not reading stdin).
const REPLY_ENQUEUE_DEADLINE_MS: u64 = 1_000;

type PendingSender = oneshot::Sender<Result<Value>>;

/// Write deadline (in ms) for a notification of `payload_bytes`.
fn notify_write_deadline_ms(payload_bytes: usize) -> u64 {
    let mib = (payload_bytes as u64) / (1024 * 1024);
    NOTIFY_BASE_DEADLINE_MS.saturating_add(mib.saturating_mul(NOTIFY_MS_PER_MIB))
}

/// One complete frame for the writer task.
struct Outbound {
    frame: Vec<u8>,
    deadline: Instant,
    /// Completed once the frame is flushed (or the write failed).
    written: Option<oneshot::Sender<Result<()>>>,
}

/// State shared by the handle, the reader task, and the writer task.
struct Shared {
    /// `None` once the connection has failed: new requests are refused and
    /// every waiter has already been failed.
    pending: StdMutex<Option<HashMap<u64, PendingSender>>>,
    failed: AtomicBool,
    /// Why the connection failed (the first reason wins), so requests made
    /// after the failure report it instead of a generic "closed".
    failure_reason: StdMutex<Option<String>>,
    outbound: mpsc::Sender<Outbound>,
    push_diagnostics: Arc<PushDiagnosticsStore>,
    partial_results: PartialResultStore,
    /// Frames whose body was not valid JSON (skipped). A spike means the
    /// stream lost its framing.
    unparseable_frames: AtomicU64,
}

impl Shared {
    fn is_alive(&self) -> bool {
        !self.failed.load(Ordering::Acquire)
    }

    /// Registers a waiter for `id`; refused once the connection has failed.
    fn register(&self, id: u64) -> Result<oneshot::Receiver<Result<Value>>> {
        let (tx, rx) = oneshot::channel();
        let mut pending = self.pending.lock().map_err(|_| self.closed())?;
        match pending.as_mut() {
            Some(map) if self.is_alive() => {
                map.insert(id, tx);
                Ok(rx)
            }
            _ => Err(self.closed()),
        }
    }

    /// Removes and returns the waiter for `id`, if it is still pending.
    fn take_pending(&self, id: u64) -> Option<PendingSender> {
        self.pending
            .lock()
            .ok()?
            .as_mut()
            .and_then(|map| map.remove(&id))
    }

    /// Marks the connection failed and fails every pending request with
    /// `reason`. Idempotent; the first reason wins for the drained waiters.
    fn fail(&self, reason: &str) {
        if let Ok(mut slot) = self.failure_reason.lock()
            && slot.is_none()
        {
            *slot = Some(reason.to_owned());
        }
        self.failed.store(true, Ordering::Release);
        let drained = self
            .pending
            .lock()
            .ok()
            .and_then(|mut pending| pending.take());
        for sender in drained.into_iter().flat_map(HashMap::into_values) {
            let _ = sender.send(Err(Error::connection_closed(reason)));
        }
    }

    /// Queues a frame without waiting. Used from `Drop` (cancel on drop).
    fn try_enqueue(&self, frame: Vec<u8>, deadline: Instant) {
        let _ = self.outbound.try_send(Outbound {
            frame,
            deadline,
            written: None,
        });
    }

    /// Queues a frame, waiting for queue space until `deadline`.
    async fn enqueue(&self, outbound: Outbound, deadline: Instant) -> Result<()> {
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
    async fn write_frame(&self, frame: Vec<u8>, deadline: Instant) -> Result<()> {
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
    fn closed(&self) -> Error {
        match self
            .failure_reason
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
        {
            Some(reason) => Error::connection_closed(reason),
            None => closed_error(),
        }
    }

    fn closed_reason(&self, reason: &str) -> String {
        match self.unparseable_frames.load(Ordering::Relaxed) {
            0 => reason.to_owned(),
            skipped => format!("{reason} ({skipped} unparseable frame(s) skipped)"),
        }
    }
}

fn closed_error() -> Error {
    Error::connection_closed("JSON-RPC connection closed")
}

/// Fails the connection when dropped, on every exit path of a task (return,
/// break, panic, or abort).
struct FailOnExit {
    shared: Arc<Shared>,
    reason: String,
}

impl FailOnExit {
    fn new(shared: &Arc<Shared>, reason: &str) -> Self {
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
struct CancelOnDrop<'a> {
    shared: &'a Shared,
    id: u64,
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

fn cancel_message(id: u64) -> Value {
    json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}})
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
    ) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (outbound, outbound_rx) = mpsc::channel(WRITER_QUEUE_FRAMES);
        let shared = Arc::new(Shared {
            pending: StdMutex::new(Some(HashMap::new())),
            failed: AtomicBool::new(false),
            failure_reason: StdMutex::new(None),
            outbound,
            push_diagnostics: PushDiagnosticsStore::new(),
            partial_results: PartialResultStore::default(),
            unparseable_frames: AtomicU64::new(0),
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
    /// broken write, or a request timeout retired it).
    pub(crate) fn is_alive(&self) -> bool {
        self.shared.is_alive()
    }

    /// Retire the connection: every pending and later request fails with
    /// `reason` (used when the server is killed for exceeding its memory
    /// cap). Idempotent; the first reason wins.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Wired on macOS only.
    pub(crate) fn fail(&self, reason: &str) {
        self.shared.fail(reason);
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

    /// Sends a request and waits for its response until `timeout_ms`.
    ///
    /// Timeout policy: a request that consumes its whole timeout means the
    /// server is wedged, not merely slow (indexing waits go through readiness),
    /// so the request is cancelled and the connection is retired: every other
    /// pending request fails immediately and the pool restarts the server.
    pub(crate) async fn request(
        &self,
        method: &str,
        params: Value,
        timeout_ms: u32,
    ) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
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
        match tokio::time::timeout_at(deadline, response).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(closed_error()),
            Err(_) => {
                let reason = format!("LSP request timed out after {timeout_ms}ms");
                if self.shared.take_pending(id).is_some()
                    && let Ok(frame) = encode_frame(&cancel_message(id))
                {
                    // Tell the server to stop computing the abandoned request,
                    // briefly waiting so the cancel precedes retirement.
                    let (tx, rx) = oneshot::channel();
                    let cancel_deadline =
                        Instant::now() + Duration::from_millis(CANCEL_WRITE_WAIT_MS);
                    let queued = self.shared.outbound.try_send(Outbound {
                        frame,
                        deadline: cancel_deadline,
                        written: Some(tx),
                    });
                    if queued.is_ok() {
                        let _ = tokio::time::timeout_at(cancel_deadline, rx).await;
                    }
                }
                self.shared
                    .fail(&format!("{reason}; LSP connection retired as wedged"));
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
        timeout_ms: u32,
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
        let result = self.request(method, params, timeout_ms).await?;
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

async fn write_loop<W>(mut writer: W, mut frames: mpsc::Receiver<Outbound>, mut exit: FailOnExit)
where
    W: AsyncWrite + Unpin,
{
    while let Some(outbound) = frames.recv().await {
        let write = async {
            writer.write_all(&outbound.frame).await?;
            writer.flush().await
        };
        let (result, broken) = match tokio::time::timeout_at(outbound.deadline, write).await {
            Ok(Ok(())) => (Ok(()), None),
            Ok(Err(error)) => {
                let reason = format!("JSON-RPC connection write failed: {error}");
                (Err(Error::connection_closed(reason.clone())), Some(reason))
            }
            Err(_) => {
                // Possibly a partial frame on the wire: the stream is unusable.
                let reason = "LSP request write timed out".to_owned();
                (Err(Error::timeout(reason.clone())), Some(reason))
            }
        };
        if let Some(written) = outbound.written {
            let _ = written.send(result);
        }
        if let Some(reason) = broken {
            exit.reason = reason;
            return;
        }
    }
}

async fn read_loop<R>(
    reader: R,
    mut exit: FailOnExit,
    context: ClientRequestContext,
    progress: Arc<ProgressTracker>,
) where
    R: AsyncRead + Unpin,
{
    let shared = Arc::clone(&exit.shared);
    let mut reader = BufReader::new(reader);
    loop {
        let body = match read_frame(&mut reader).await {
            Ok(Frame::Body(body)) => body,
            Ok(Frame::Empty) => continue,
            Ok(Frame::Eof) => return,
            Err(error) => {
                exit.reason = error.0;
                return;
            }
        };
        let Ok(Value::Object(message)) = serde_json::from_slice::<Value>(&body) else {
            shared.unparseable_frames.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        drop(body);
        if let Err(reason) = dispatch(message, &shared, &context, &progress).await {
            exit.reason = reason;
            return;
        }
    }
}

/// Routes one inbound message. `Err` carries a fatal reason.
async fn dispatch(
    mut message: Map<String, Value>,
    shared: &Shared,
    context: &ClientRequestContext,
    progress: &ProgressTracker,
) -> std::result::Result<(), String> {
    if let Some(Value::String(method)) = message.remove("method") {
        let params = message.remove("params");
        match (message.remove("id"), method.as_str()) {
            (Some(id), _) => {
                let reply = client_response_for(&method, params.as_ref(), context).into_message(id);
                let frame = encode_frame(&reply).map_err(|error| error.reason)?;
                let deadline = Instant::now() + Duration::from_millis(REPLY_ENQUEUE_DEADLINE_MS);
                let queued = Outbound {
                    frame,
                    deadline: deadline + Duration::from_millis(NOTIFY_BASE_DEADLINE_MS),
                    written: None,
                };
                match tokio::time::timeout_at(deadline, shared.outbound.send(queued)).await {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => return Err("JSON-RPC writer stopped".to_owned()),
                    Err(_) => {
                        return Err(format!(
                            "LSP server stopped reading replies (reply to {method} could not be queued)"
                        ));
                    }
                }
            }
            (None, "$/progress") => {
                if let Some(params) = params {
                    handle_progress(params, shared, progress).await;
                }
            }
            (None, "experimental/serverStatus") => {
                if let Some(quiescent) = params
                    .as_ref()
                    .and_then(|params| params.get("quiescent"))
                    .and_then(Value::as_bool)
                {
                    progress.on_server_status(quiescent).await;
                }
            }
            (None, "textDocument/publishDiagnostics") => {
                if let Some(params) = params {
                    shared.push_diagnostics.record(params);
                }
            }
            (None, _) => {}
        }
        return Ok(());
    }
    let Some(id) = message.get("id").and_then(parse_response_id) else {
        return Ok(());
    };
    let result = match message.remove("error") {
        Some(error) => Err(Error::rpc(RpcError::from_value(error))),
        None => Ok(message.remove("result").unwrap_or(Value::Null)),
    };
    if let Some(sender) = shared.take_pending(id) {
        let _ = sender.send(result);
    }
    Ok(())
}

/// Matches a response `id` back to a pending request key. We send integer ids,
/// but JSON-RPC permits string ids and some servers echo a stringified integer.
fn parse_response_id(id: &Value) -> Option<u64> {
    id.as_u64()
        .or_else(|| id.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
}

async fn handle_progress(mut params: Value, shared: &Shared, progress: &ProgressTracker) {
    let token = params.get("token").and_then(|token| {
        token
            .as_str()
            .map(str::to_owned)
            .or_else(|| token.as_u64().map(|n| n.to_string()))
    });
    let Some(token) = token else {
        return;
    };
    // Our partial-result tokens carry result chunks (which may themselves have
    // a `kind` field, e.g. a diagnostic report); every other token is
    // work-done progress.
    if shared.partial_results.is_tracking(&token) {
        if let Some(value) = params.get_mut("value").map(Value::take) {
            shared.partial_results.record(&token, value);
        }
        return;
    }
    match params
        .get("value")
        .and_then(|value| value.get("kind"))
        .and_then(Value::as_str)
    {
        Some("begin") => progress.on_begin(token).await,
        Some("end") => progress.on_end(&token).await,
        _ => {}
    }
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
