use crate::error::{Error, Result, Status};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use super::push_diagnostics::PushDiagnosticsStore;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, oneshot, watch};
use tokio::time::{Duration, Instant};

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;
const MAX_JSON_RPC_CONTENT_LENGTH: usize = 64 * 1024 * 1024;
/// Upper bound on a single header line (e.g. `Content-Length: <n>`). A well-formed
/// LSP header line is a few dozen bytes; anything approaching this cap is a server
/// streaming an unterminated line to exhaust memory via `read_line`.
const MAX_HEADER_LINE_BYTES: u64 = 8 * 1024;
/// Upper bound on the whole header block (all lines up to the blank separator).
/// Bounds the aggregate even when each individual line stays under the per-line cap.
const MAX_HEADER_BLOCK_BYTES: usize = 64 * 1024;
/// Cap on concurrently-active `$/progress` begin tokens. A conformant server has a
/// handful of in-flight progress streams; beyond this a server is either buggy or
/// hostile, so extra begins are ignored rather than growing the set without bound.
const MAX_ACTIVE_PROGRESS_TOKENS: usize = 512;
pub(super) const MAX_PUSH_DIAGNOSTIC_DOCUMENTS: usize = 256;
pub(super) const MAX_PUSH_DIAGNOSTICS_PER_DOCUMENT: usize = 2_000;
pub(super) const MAX_PUSH_DIAGNOSTIC_BYTES_PER_DOCUMENT: usize = 256 * 1024;
const MAX_PARTIAL_RESULT_BYTES: usize = 16 * 1024 * 1024;
/// Floor for a notification's write deadline. A small notification (initialized,
/// exit, a tiny didChange) must complete within this window.
const NOTIFY_BASE_DEADLINE_MS: u64 = 1_000;
/// Extra write budget granted per megabyte of notification payload, so a large
/// `didOpen` body is not guillotined by the flat floor before it can be flushed
/// to a server that drains stdin slowly.
const NOTIFY_MS_PER_MIB: u64 = 1_000;

/// Write deadline (in ms) for a notification of `payload_bytes`, scaled to the
/// payload so large `didOpen` bodies get proportional headroom while small
/// notifications keep the 1s floor.
fn notify_write_deadline_ms(payload_bytes: usize) -> u64 {
    let mib = (payload_bytes as u64) / (1024 * 1024);
    NOTIFY_BASE_DEADLINE_MS.saturating_add(mib.saturating_mul(NOTIFY_MS_PER_MIB))
}

#[derive(Default)]
struct PartialResultBuffer {
    values: Vec<Value>,
    bytes: usize,
    overflowed: bool,
}

#[derive(Default)]
struct PartialResultStore {
    buffers: StdMutex<HashMap<String, PartialResultBuffer>>,
}

impl PartialResultStore {
    fn begin(&self, token: String) {
        if let Ok(mut buffers) = self.buffers.lock() {
            buffers.insert(token, PartialResultBuffer::default());
        }
    }

    fn record(&self, token: &str, value: &Value) {
        let Ok(mut buffers) = self.buffers.lock() else {
            return;
        };
        let Some(buffer) = buffers.get_mut(token) else {
            return;
        };
        let bytes = serde_json::to_vec(value).map_or(MAX_PARTIAL_RESULT_BYTES + 1, |v| v.len());
        if buffer.bytes.saturating_add(bytes) > MAX_PARTIAL_RESULT_BYTES {
            buffer.overflowed = true;
            return;
        }
        buffer.bytes += bytes;
        buffer.values.push(value.clone());
    }

    fn finish(&self, token: &str, final_result: Value) -> Result<Value> {
        let buffer = self
            .buffers
            .lock()
            .ok()
            .and_then(|mut buffers| buffers.remove(token))
            .unwrap_or_default();
        if buffer.overflowed {
            return Err(Error::new(
                Status::GenericFailure,
                "LSP partial results exceeded the bounded collection limit",
            ));
        }
        Ok(buffer
            .values
            .into_iter()
            .rev()
            .fold(final_result, |accumulated, partial| {
                merge_partial_result(partial, accumulated)
            }))
    }
}

/// How `wait_until_idle` concluded — a readiness signal the JS layer uses to
/// tell "server confirmed indexing is done" apart from "server never told us,
/// we only waited a fixed window" apart from "server is still busy".
///
/// This distinction is what lets a *zero-results* semantic query be reported
/// honestly: only `ProgressIdle` means the empty answer reflects the indexed
/// project; the other two mean the emptiness might just be "not indexed yet".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Readiness {
    /// Saw at least one `$/progress` cycle and drained it to idle — the server
    /// announced indexing and we waited for it to finish.
    ProgressIdle,
    /// Never saw any `$/progress`; only the settle window elapsed. Normal for
    /// servers that do not report indexing (e.g. typescript-language-server),
    /// so completion cannot be confirmed — not an error.
    SilentServer,
    /// `$/progress` was still active when `timeout_ms` expired — the server is
    /// (as far as we know) still indexing.
    Timeout,
}

impl Readiness {
    /// Stable string form crossing the napi boundary into JS.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProgressIdle => "progressIdle",
            Self::SilentServer => "settledWithoutProgress",
            Self::Timeout => "timeout",
        }
    }
}

/// Tracks in-flight `$/progress` tokens emitted by a language server.
///
/// After `initialized` is sent, servers like `rust-analyzer` begin asynchronous
/// project indexing and announce it via `$/progress begin`/`end` notifications.
/// `wait_until_idle` gates on all such tokens completing (or a deadline firing).
///
/// Two-phase wait:
///   1. **Settle** (`SETTLE_MS`): wait for the first `begin` to arrive.
///      Servers that don't use progress return immediately after this window.
///   2. **Drain**: wait until every active token ends or the original deadline expires.
pub struct ProgressTracker {
    active: Mutex<HashSet<String>>,
    /// `true` once at least one `begin` notification has been received.
    ever_active: AtomicBool,
    count_tx: watch::Sender<usize>,
    count_rx: watch::Receiver<usize>,
}

impl ProgressTracker {
    pub fn new() -> Arc<Self> {
        let (count_tx, count_rx) = watch::channel(0usize);
        Arc::new(Self {
            active: Mutex::new(HashSet::new()),
            ever_active: AtomicBool::new(false),
            count_tx,
            count_rx,
        })
    }

    pub async fn on_begin(&self, token: String) {
        let mut active = self.active.lock().await;
        // Cap the active set so a server emitting an unbounded stream of distinct
        // `begin` tokens (never matched by `end`) cannot grow memory or wedge
        // `wait_until_idle` forever. Re-inserting an already-tracked token is fine;
        // only genuinely new tokens beyond the cap are dropped.
        if !active.contains(&token) && active.len() >= MAX_ACTIVE_PROGRESS_TOKENS {
            return;
        }
        active.insert(token);
        self.ever_active.store(true, Ordering::Release);
        let _ = self.count_tx.send(active.len());
    }

    pub async fn on_end(&self, token: &str) {
        let mut active = self.active.lock().await;
        active.remove(token);
        let _ = self.count_tx.send(active.len());
    }

    /// Blocks until all in-flight tokens end **and** a quiescence window passes
    /// with no new tokens starting, or until `timeout_ms` elapses.
    ///
    /// Servers like `rust-analyzer` emit several sequential `$/progress` waves
    /// (e.g. crate loading -> workspace analysis -> cache priming).  Without
    /// the quiescence window we would return after the *first* wave, before the
    /// server is fully ready to answer queries.
    pub async fn wait_until_idle(&self, timeout_ms: u64) -> Readiness {
        /// Wait this long for the very first `$/progress begin` after
        /// `initialized` is sent.
        ///
        /// This window has to absorb two very different server behaviours:
        ///   * Servers that announce indexing via `$/progress` — they emit a
        ///     `begin` within this window and we then drain to completion.
        ///   * Servers that index WITHOUT progress events — the only safe
        ///     signal we have is elapsed time, so the window must be long
        ///     enough that the server has plausibly finished its initial work
        ///     before we let the first query through.
        ///
        /// 100 ms was too aggressive: a server indexing silently would race the
        /// first query and return wrong/empty results. We use a conservative
        /// few-second window instead, always bounded by the caller's
        /// `timeout_ms` so `wait_for_ready` can never block longer than asked.
        const SETTLE_MS: u64 = 2_000;
        /// After count reaches 0, wait this long for any follow-up wave before
        /// declaring the server idle.  Sized to bridge the typical gap between
        /// rust-analyzer progress waves (~10-100 ms in practice).
        const QUIESCE_MS: u64 = 200;

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);

        let mut rx = self.count_rx.clone();

        // Phase 1 -- settle: wait briefly for the first $/progress begin.
        if *rx.borrow() == 0 && !self.ever_active.load(Ordering::Acquire) {
            let settle = Duration::from_millis(SETTLE_MS.min(timeout_ms));
            let became_active = tokio::time::timeout(settle, rx.wait_for(|c| *c > 0))
                .await
                .is_ok();
            if !became_active {
                // Server does not use progress -- we only waited the settle
                // window, so we cannot confirm indexing actually finished.
                return Readiness::SilentServer;
            }
        }

        Self::drain_until_quiet(&mut rx, deadline, QUIESCE_MS).await
    }

    /// Snapshot the progress stream so a later [`Self::wait_until_idle_after`]
    /// observes every `begin`/`end` that arrives from this point on — even one
    /// that starts and finishes before the waiter is polled.
    pub fn subscribe(&self) -> watch::Receiver<usize> {
        let mut rx = self.count_rx.clone();
        rx.borrow_and_update();
        rx
    }

    /// Like [`Self::wait_until_idle`] but scoped to progress that starts AFTER
    /// `rx` was taken via [`Self::subscribe`], with a caller-chosen settle.
    ///
    /// Servers such as `typescript-language-server` announce nothing on
    /// `initialized` and only start loading a project once a document is
    /// opened (`$/progress begin` ~100 ms after `didOpen`). Waiting here after
    /// the first `didOpen` keeps queries from racing that load. The settle is
    /// short because the triggering event is known; servers that never report
    /// progress pay only `settle_ms`.
    pub async fn wait_until_idle_after(
        &self,
        mut rx: watch::Receiver<usize>,
        settle_ms: u64,
        timeout_ms: u64,
    ) -> Readiness {
        const QUIESCE_MS: u64 = 200;
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        if *rx.borrow() == 0 {
            let settle = Duration::from_millis(settle_ms.min(timeout_ms));
            // Any update since `subscribe` counts: a begin/end pair that already
            // completed still bumps the channel version.
            if tokio::time::timeout(settle, rx.changed()).await.is_err() {
                return Readiness::SilentServer;
            }
        }
        Self::drain_until_quiet(&mut rx, deadline, QUIESCE_MS).await
    }

    /// Drain + quiesce loop: repeat until a full `quiesce_ms` window passes
    /// with no active tokens and no new ones starting, or `deadline` expires.
    async fn drain_until_quiet(
        rx: &mut watch::Receiver<usize>,
        deadline: Instant,
        quiesce_ms: u64,
    ) -> Readiness {
        loop {
            // Wait for count to reach zero.
            let remaining = deadline.saturating_duration_since(Instant::now());
            if tokio::time::timeout(remaining, rx.wait_for(|c| *c == 0))
                .await
                .is_err()
            {
                return Readiness::Timeout; // Deadline expired while tokens active.
            }

            // Quiesce: wait briefly to see if a new wave starts.
            let remaining = deadline.saturating_duration_since(Instant::now());
            let full_quiescence = Duration::from_millis(quiesce_ms);
            let quiesce = full_quiescence.min(remaining);
            // Any update breaks quiescence, including a short begin/end wave
            // whose active count was coalesced back to zero before we woke.
            let new_wave = tokio::time::timeout(quiesce, rx.changed()).await.is_ok();
            if !new_wave {
                return if remaining >= full_quiescence {
                    Readiness::ProgressIdle
                } else {
                    Readiness::Timeout
                };
            }
            // A new wave started; loop back and drain it too.
        }
    }
}
type SharedWriter<W> = Arc<Mutex<W>>;

#[derive(Clone)]
pub struct ClientRequestContext {
    pub configuration: Value,
    pub workspace_folders: Value,
}

pub struct JsonRpcConnection<W>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    writer: SharedWriter<W>,
    next_id: AtomicU64,
    pending: PendingMap,
    /// `true` once the read loop has exited (EOF/read error) and failed every
    /// pending request. Lets the JS client pool tell a live connection from a
    /// crashed one at `acquire()` time instead of only via the idle timer.
    failed: Arc<AtomicBool>,
    push_diagnostics: Arc<PushDiagnosticsStore>,
    partial_results: Arc<PartialResultStore>,
}

impl<W> JsonRpcConnection<W>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    pub fn new<R>(
        reader: R,
        writer: W,
        context: ClientRequestContext,
        progress: Arc<ProgressTracker>,
    ) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let writer = Arc::new(Mutex::new(writer));
        let failed = Arc::new(AtomicBool::new(false));
        let push_diagnostics = PushDiagnosticsStore::new();
        let partial_results = Arc::new(PartialResultStore::default());
        tokio::spawn(read_loop(
            reader,
            Arc::clone(&pending),
            Arc::clone(&writer),
            context,
            Arc::clone(&progress),
            Arc::clone(&failed),
            Arc::clone(&push_diagnostics),
            Arc::clone(&partial_results),
        ));
        Self {
            writer,
            next_id: AtomicU64::new(1),
            pending,
            failed,
            push_diagnostics,
            partial_results,
        }
    }

    /// `false` once the read loop has observed the connection close (server
    /// crashed or exited) and failed all pending requests.
    pub fn is_alive(&self) -> bool {
        !self.failed.load(Ordering::Acquire)
    }

    pub fn clear_push_diagnostics(&self, uri: &str) {
        self.push_diagnostics.clear(uri);
    }

    pub async fn wait_for_push_diagnostics(
        &self,
        uri: &str,
        timeout_ms: u32,
        min_version: Option<i64>,
    ) -> Option<Value> {
        self.push_diagnostics
            .wait_for(uri, timeout_ms, min_version)
            .await
    }

    pub async fn request(&self, method: &str, params: Value, timeout_ms: u32) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
        if !self.is_alive() {
            return Err(Error::new(
                Status::GenericFailure,
                "JSON-RPC connection closed",
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let message = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        if let Err(err) = self.write_before(&message, deadline).await {
            self.pending.lock().await.remove(&id);
            return Err(err);
        }
        match tokio::time::timeout_at(deadline, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(Error::new(
                Status::GenericFailure,
                "JSON-RPC response channel closed",
            )),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                // Tell the server to stop computing the now-abandoned request via
                // LSP `$/cancelRequest`; otherwise it keeps burning CPU on a
                // result nobody will read (and can head-of-line block later work
                // on single-threaded servers). Best-effort: we are already
                // returning a timeout error, so a write failure here is moot.
                let cancellation =
                    json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}});
                let _ = self
                    .write_before(&cancellation, Instant::now() + Duration::from_millis(100))
                    .await;
                // Retire the connection: a request that consumed its whole
                // timeout indicates a wedged (not merely slow) server, so mark it
                // failed to make `is_alive()` report it and let the pool
                // evict/restart it instead of re-serving a hung server. Done
                // AFTER the cancel write above so the cancellation still goes out
                // (write_before short-circuits once the connection is failed).
                self.failed.store(true, Ordering::Release);
                Err(Error::new(
                    Status::GenericFailure,
                    format!("LSP request timed out after {timeout_ms}ms"),
                ))
            }
        }
    }

    pub async fn request_with_partials(
        &self,
        method: &str,
        mut params: Value,
        timeout_ms: u32,
    ) -> Result<Value> {
        let token = format!(
            "octocode-partial-{}",
            self.next_id.fetch_add(1, Ordering::SeqCst)
        );
        let Some(object) = params.as_object_mut() else {
            return Err(Error::new(
                Status::InvalidArg,
                "LSP partial-result params must be an object",
            ));
        };
        object.insert(
            "partialResultToken".to_owned(),
            Value::String(token.clone()),
        );
        self.partial_results.begin(token.clone());
        match self.request(method, params, timeout_ms).await {
            Ok(result) => self.partial_results.finish(&token, result),
            Err(error) => {
                let _ = self.partial_results.finish(&token, Value::Null);
                Err(error)
            }
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let message = json!({"jsonrpc":"2.0","method":method,"params":params});
        // didOpen/initialized/exit writes must not hold startup or shutdown open
        // indefinitely when a server stops draining stdin. The deadline scales
        // with payload size so a large didOpen body gets proportional headroom
        // instead of being guillotined by a flat 1s cap.
        let payload_bytes = serde_json::to_vec(&message)
            .map(|body| body.len())
            .unwrap_or(0);
        let deadline =
            Instant::now() + Duration::from_millis(notify_write_deadline_ms(payload_bytes));
        self.write_before(&message, deadline).await
    }

    async fn write_before(&self, message: &Value, deadline: Instant) -> Result<()> {
        if !self.is_alive() {
            return Err(Error::new(
                Status::GenericFailure,
                "JSON-RPC connection closed",
            ));
        }
        let result = tokio::time::timeout_at(deadline, write_message(&self.writer, message)).await;
        match result {
            Ok(Ok(())) => Ok(()),
            failure => {
                // A cancelled write can leave a partial Content-Length frame.
                // Do not reuse the stream or let concurrent requests await replies.
                self.failed.store(true, Ordering::Release);
                fail_all_pending(&self.pending, "JSON-RPC connection write failed").await;
                Err(match failure {
                    Ok(Err(error)) => error,
                    _ => Error::new(Status::GenericFailure, "LSP request write timed out"),
                })
            }
        }
    }
}

async fn read_loop<R, W>(
    reader: R,
    pending: PendingMap,
    writer: SharedWriter<W>,
    context: ClientRequestContext,
    progress: Arc<ProgressTracker>,
    failed: Arc<AtomicBool>,
    push_diagnostics: Arc<PushDiagnosticsStore>,
    partial_results: Arc<PartialResultStore>,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let mut reader = BufReader::new(reader);
    loop {
        let content_length = match read_headers(&mut reader).await {
            Ok(HeaderOutcome::Frame(len)) => len,
            Ok(HeaderOutcome::Eof) | Err(_) => break,
        };
        if content_length == 0 {
            // Empty/length-less frame: nothing to parse, keep the connection open.
            continue;
        }
        if content_length > MAX_JSON_RPC_CONTENT_LENGTH {
            // An oversized frame is unrecoverable: we cannot resync the stream to
            // the next frame boundary. Mark the connection dead BEFORE returning
            // so `is_alive()` reports it, otherwise the pool keeps handing out a
            // client whose read loop has already exited (30s hangs + leaked child).
            failed.store(true, Ordering::Release);
            fail_all_pending(
                &pending,
                &format!(
                    "LSP response exceeded maximum JSON-RPC frame size: {content_length} bytes"
                ),
            )
            .await;
            return;
        }
        let mut body = vec![0u8; content_length];
        if reader.read_exact(&mut body).await.is_err() {
            break;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&body) else {
            continue;
        };
        if let Some(method) = value.get("method").and_then(Value::as_str) {
            // Track $/progress begin/end so wait_for_ready can gate on indexing completion.
            if method == "$/progress" {
                handle_progress_notification(&value, &progress, &partial_results).await;
            }
            if method == "textDocument/publishDiagnostics"
                && let Some(params) = value.get("params")
            {
                push_diagnostics.record(params);
            }
            if let Some(id) = value.get("id").cloned() {
                let response = match client_response_for(method, value.get("params"), &context) {
                    ClientResponse::Result(result) => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": result,
                    }),
                    ClientResponse::Error { code, message } => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": code, "message": message },
                    }),
                };
                let _ = write_message(&writer, &response).await;
            }
            continue;
        }
        let Some(id) = value.get("id").and_then(parse_response_id) else {
            continue;
        };
        let result = if let Some(error) = value.get("error") {
            Err(Error::new(
                Status::GenericFailure,
                format!("LSP error: {error}"),
            ))
        } else {
            Ok(value.get("result").cloned().unwrap_or(Value::Null))
        };
        if let Some(sender) = pending.lock().await.remove(&id) {
            let _ = sender.send(result);
        }
    }
    failed.store(true, Ordering::Release);
    fail_all_pending(&pending, "LSP connection closed").await;
}

/// Matches a response `id` field back to a pending request key.
///
/// We send integer ids, but the JSON-RPC spec also permits string ids and some
/// servers echo our integer back as a stringified integer (e.g. `"3"`). Accept
/// both so such responses resolve instead of waiting for the request timeout.
fn parse_response_id(id: &Value) -> Option<u64> {
    id.as_u64()
        .or_else(|| id.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
}

async fn handle_progress_notification(
    value: &Value,
    progress: &Arc<ProgressTracker>,
    partial_results: &Arc<PartialResultStore>,
) {
    let params = value.get("params");
    let token = params.and_then(|p| p.get("token")).and_then(|t| {
        t.as_str()
            .map(str::to_owned)
            .or_else(|| t.as_u64().map(|n| n.to_string()))
    });
    let kind = params
        .and_then(|p| p.get("value"))
        .and_then(|v| v.get("kind"))
        .and_then(Value::as_str);
    match (token, kind) {
        (Some(token), Some("begin")) => progress.on_begin(token).await,
        (Some(token), Some("end")) => progress.on_end(&token).await,
        (Some(token), None) => {
            if let Some(value) = params.and_then(|params| params.get("value")) {
                partial_results.record(&token, value);
            }
        }
        _ => {}
    }
}

fn merge_partial_result(partial: Value, final_result: Value) -> Value {
    match (partial, final_result) {
        (Value::Array(mut partial), Value::Array(final_values)) => {
            partial.extend(final_values);
            Value::Array(partial)
        }
        (Value::Object(partial), Value::Object(mut final_values)) => {
            for (key, partial_value) in partial {
                let merged = final_values
                    .remove(&key)
                    .map(|final_value| merge_partial_result(partial_value.clone(), final_value))
                    .unwrap_or(partial_value);
                final_values.insert(key, merged);
            }
            Value::Object(final_values)
        }
        (partial, Value::Null) => partial,
        (_, final_result) => final_result,
    }
}

async fn fail_all_pending(pending: &PendingMap, reason: &str) {
    let pending_requests = {
        let mut pending = pending.lock().await;
        pending
            .drain()
            .map(|(_, sender)| sender)
            .collect::<Vec<_>>()
    };
    for sender in pending_requests {
        let _ = sender.send(Err(Error::new(Status::GenericFailure, reason)));
    }
}

/// A reply to a server->client request: either a JSON-RPC `result` payload or a
/// JSON-RPC `error`. Genuinely unknown methods map to MethodNotFound (-32601)
/// rather than a misleading `result: null` (which a server could mistake for a
/// successful empty response).
#[derive(Debug)]
enum ClientResponse {
    Result(Value),
    Error { code: i64, message: String },
}

fn client_response_for(
    method: &str,
    params: Option<&Value>,
    context: &ClientRequestContext,
) -> ClientResponse {
    match method {
        "workspace/configuration" => {
            let item_count = params
                .and_then(|value| value.get("items"))
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            ClientResponse::Result(Value::Array(
                (0..item_count)
                    .map(|_| context.configuration.clone())
                    .collect(),
            ))
        }
        "workspace/workspaceFolders" => ClientResponse::Result(context.workspace_folders.clone()),
        "workspace/applyEdit" => ClientResponse::Result(json!({ "applied": false })),
        "client/registerCapability"
        | "client/unregisterCapability"
        | "window/showMessageRequest"
        | "window/workDoneProgress/create" => ClientResponse::Result(Value::Null),
        other => ClientResponse::Error {
            code: -32601,
            message: format!("Method not found: {other}"),
        },
    }
}

/// Outcome of reading one JSON-RPC header block.
///
/// `Eof` means the stream closed (connection should tear down). `Frame(len)`
/// carries the body length to read next — a length of `0` (e.g. a blank-line
/// frame with no `Content-Length`, or an explicit `Content-Length: 0`) is a
/// well-formed but empty frame the read loop skips, NOT a reason to disconnect.
enum HeaderOutcome {
    Eof,
    Frame(usize),
}

async fn read_headers<R>(reader: &mut BufReader<R>) -> std::io::Result<HeaderOutcome>
where
    R: AsyncRead + Unpin,
{
    let mut content_length = None;
    let mut header_bytes = 0usize;
    loop {
        let mut line = String::new();
        // Bound the per-line read so an unterminated header line cannot grow the
        // String without limit. `take` caps how many bytes `read_line` will pull.
        let bytes = (&mut *reader)
            .take(MAX_HEADER_LINE_BYTES)
            .read_line(&mut line)
            .await?;
        if bytes == 0 {
            return Ok(HeaderOutcome::Eof);
        }
        // A line that consumed the entire per-line budget without a terminating
        // newline is oversized/unbounded; refuse the connection cleanly.
        if bytes as u64 == MAX_HEADER_LINE_BYTES && !line.ends_with('\n') {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "LSP header line exceeded maximum size",
            ));
        }
        header_bytes = header_bytes.saturating_add(bytes);
        if header_bytes > MAX_HEADER_BLOCK_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "LSP header block exceeded maximum size",
            ));
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            // End of header block. A missing Content-Length is treated as a
            // zero-length (empty) frame so a single malformed frame does not
            // tear down the whole connection.
            return Ok(HeaderOutcome::Frame(content_length.unwrap_or(0)));
        }
        // LSP headers are case-insensitive (per the base protocol, which mirrors
        // HTTP); match the field name without regard to case.
        if let Some((name, value)) = trimmed.split_once(':')
            && name.trim().eq_ignore_ascii_case("Content-Length")
        {
            content_length = value.trim().parse::<usize>().ok();
        }
    }
}

async fn write_message<W>(writer: &SharedWriter<W>, message: &Value) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let body = serde_json::to_vec(message).map_err(|err| {
        Error::new(
            Status::GenericFailure,
            format!("Serialize JSON-RPC failed: {err}"),
        )
    })?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut writer = writer.lock().await;
    writer
        .write_all(header.as_bytes())
        .await
        .map_err(io_error)?;
    writer.write_all(&body).await.map_err(io_error)?;
    writer.flush().await.map_err(io_error)
}

fn io_error(err: std::io::Error) -> Error {
    Error::new(Status::GenericFailure, err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, sink};

    #[test]
    fn notify_write_deadline_scales_with_payload_size() {
        // A small notification keeps the 1s floor; a large didOpen body must not
        // be guillotined by a flat 1s deadline — the budget grows with payload.
        assert_eq!(notify_write_deadline_ms(0), NOTIFY_BASE_DEADLINE_MS);
        assert_eq!(notify_write_deadline_ms(1_024), NOTIFY_BASE_DEADLINE_MS);
        assert_eq!(
            notify_write_deadline_ms(3 * 1024 * 1024),
            NOTIFY_BASE_DEADLINE_MS + 3 * NOTIFY_MS_PER_MIB
        );
        // A larger payload yields a strictly larger deadline (monotonic).
        assert!(notify_write_deadline_ms(8 * 1024 * 1024) > notify_write_deadline_ms(1024 * 1024));
    }

    #[test]
    fn partial_result_store_merges_array_and_object_chunks_in_protocol_order() {
        let store = PartialResultStore::default();
        store.begin("locations".to_owned());
        store.record("locations", &json!([{"uri":"a"}]));
        store.record("locations", &json!([{"uri":"b"}]));
        assert_eq!(
            store
                .finish("locations", json!([{"uri":"c"}]))
                .expect("merge"),
            json!([{"uri":"a"},{"uri":"b"},{"uri":"c"}])
        );

        store.begin("diagnostics".to_owned());
        store.record("diagnostics", &json!({"items":[{"message":"first"}]}));
        assert_eq!(
            store
                .finish(
                    "diagnostics",
                    json!({"kind":"full","items":[{"message":"last"}]})
                )
                .expect("merge"),
            json!({
                "kind":"full",
                "items":[{"message":"first"},{"message":"last"}]
            })
        );
    }

    #[test]
    fn request_with_partials_collects_progress_before_final_response() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let (client_w, server_r) = duplex(8192);
            let (mut server_w, client_r) = duplex(8192);
            let connection = JsonRpcConnection::new(
                client_r,
                client_w,
                ClientRequestContext {
                    configuration: Value::Null,
                    workspace_folders: Value::Null,
                },
                ProgressTracker::new(),
            );
            let server = tokio::spawn(async move {
                let mut reader = BufReader::new(server_r);
                let HeaderOutcome::Frame(length) =
                    read_headers(&mut reader).await.expect("request header")
                else {
                    panic!("request frame expected");
                };
                let mut body = vec![0; length];
                reader.read_exact(&mut body).await.expect("request body");
                let request: Value = serde_json::from_slice(&body).expect("request json");
                let token = request["params"]["partialResultToken"].clone();
                let id = request["id"].clone();
                for message in [
                    json!({
                        "jsonrpc":"2.0","method":"$/progress",
                        "params":{"token":token,"value":[{"uri":"partial"}]}
                    }),
                    json!({"jsonrpc":"2.0","id":id,"result":[{"uri":"final"}]}),
                ] {
                    let body = serde_json::to_vec(&message).expect("response json");
                    server_w
                        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
                        .await
                        .expect("response header");
                    server_w.write_all(&body).await.expect("response body");
                }
            });

            let result = connection
                .request_with_partials("textDocument/references", json!({}), 1_000)
                .await
                .expect("partial request");
            server.await.expect("server");
            assert_eq!(result, json!([{"uri":"partial"},{"uri":"final"}]));
        });
    }

    #[test]
    fn client_response_for_unknown_method_is_method_not_found() {
        let context = ClientRequestContext {
            configuration: json!({ "settings": true }),
            workspace_folders: json!([{ "uri": "file:///w", "name": "workspace" }]),
        };
        // A genuinely unknown server->client request must be answered with a
        // JSON-RPC MethodNotFound error, not a misleading `result: null`.
        match client_response_for("nonexistent/method", None, &context) {
            ClientResponse::Error { code, .. } => assert_eq!(code, -32601),
            other => panic!("expected -32601 MethodNotFound, got {other:?}"),
        }
        // Known handled methods still yield their expected results.
        match client_response_for("workspace/workspaceFolders", None, &context) {
            ClientResponse::Result(value) => {
                assert_eq!(value, context.workspace_folders.clone());
            }
            other => panic!("expected workspaceFolders result, got {other:?}"),
        }
        match client_response_for("workspace/applyEdit", None, &context) {
            ClientResponse::Result(value) => assert_eq!(value, json!({ "applied": false })),
            other => panic!("expected applyEdit result, got {other:?}"),
        }
        // Known-but-null-returning methods are preserved as results, not errors.
        match client_response_for("client/registerCapability", None, &context) {
            ClientResponse::Result(Value::Null) => {}
            other => panic!("registerCapability must stay a null result, got {other:?}"),
        }
        match client_response_for("window/workDoneProgress/create", None, &context) {
            ClientResponse::Result(Value::Null) => {}
            other => panic!("workDoneProgress/create must return a null result, got {other:?}"),
        }
    }

    #[test]
    fn read_headers_is_case_insensitive_for_content_length() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let (mut server, client_reader) = duplex(1024);
            server
                .write_all(b"content-length: 42\r\n\r\n")
                .await
                .expect("write lowercase header");
            drop(server);
            let mut reader = BufReader::new(client_reader);
            let outcome = read_headers(&mut reader).await.expect("read headers");
            assert!(matches!(outcome, HeaderOutcome::Frame(42)));
        });
    }

    #[test]
    fn read_headers_treats_missing_length_as_empty_frame_not_eof() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let (mut server, client_reader) = duplex(1024);
            // A header block with NO Content-Length followed by a blank line.
            server
                .write_all(b"X-Unknown: whatever\r\n\r\n")
                .await
                .expect("write length-less header");
            drop(server);
            let mut reader = BufReader::new(client_reader);
            let outcome = read_headers(&mut reader).await.expect("read headers");
            // Must be an (empty) frame, NOT Eof — the connection survives it.
            assert!(matches!(outcome, HeaderOutcome::Frame(0)));
        });
    }

    #[test]
    fn read_headers_reports_eof_on_closed_stream() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let (server, client_reader) = duplex(1024);
            drop(server);
            let mut reader = BufReader::new(client_reader);
            let outcome = read_headers(&mut reader).await.expect("read headers");
            assert!(matches!(outcome, HeaderOutcome::Eof));
        });
    }

    #[test]
    fn connection_is_alive_until_the_server_stream_closes() {
        // Drives the pool-crash-detection fix: the JS client pool must be able
        // to tell a live connection from one whose read loop has already hit
        // EOF (server crashed / stream closed) so it can evict a stale pooled
        // entry instead of returning it to the next acquire().
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let (client_w, server_r) = duplex(1024);
            let (server_w, client_r) = duplex(1024);

            let conn = JsonRpcConnection::new(
                client_r,
                client_w,
                ClientRequestContext {
                    configuration: Value::Null,
                    workspace_folders: Value::Null,
                },
                ProgressTracker::new(),
            );

            assert!(conn.is_alive(), "connection should start alive");

            // Close both ends of the "server" side so the client's read loop
            // observes EOF, the same signal a real crashed server produces.
            drop(server_w);
            drop(server_r);

            // The read loop runs as a spawned task on the same runtime; poll
            // briefly for it to process the EOF rather than assuming it has
            // already run by the time we check.
            let mut became_dead = false;
            for _ in 0..50 {
                if !conn.is_alive() {
                    became_dead = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert!(became_dead, "connection should be marked dead after EOF");
        });
    }

    #[test]
    fn connection_caches_push_diagnostics_for_bounded_on_demand_reads() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let (client_w, _server_r) = duplex(4096);
            let (mut server_w, client_r) = duplex(4096);
            let conn = JsonRpcConnection::new(
                client_r,
                client_w,
                ClientRequestContext {
                    configuration: Value::Null,
                    workspace_folders: Value::Null,
                },
                ProgressTracker::new(),
            );
            let body = serde_json::to_vec(&json!({
                "jsonrpc": "2.0",
                "method": "textDocument/publishDiagnostics",
                "params": {
                    "uri": "file:///workspace/a.ts",
                    "version": 3,
                    "diagnostics": [{ "message": "broken" }]
                }
            }))
            .expect("serialize notification");
            server_w
                .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
                .await
                .expect("write header");
            server_w.write_all(&body).await.expect("write body");
            server_w.flush().await.expect("flush");

            let report = conn
                .wait_for_push_diagnostics("file:///workspace/a.ts", 1_000, Some(3))
                .await
                .expect("push diagnostics report");
            assert_eq!(report["kind"], "full");
            assert_eq!(report["version"], 3);
            assert_eq!(report["items"][0]["message"], "broken");

            conn.clear_push_diagnostics("file:///workspace/a.ts");
            assert!(
                conn.wait_for_push_diagnostics("file:///workspace/a.ts", 1, Some(3))
                    .await
                    .is_none()
            );
        });
    }

    #[test]
    fn push_diagnostics_rejects_an_older_document_version() {
        let store = PushDiagnosticsStore::new();
        store.record(&json!({
            "uri": "file:///workspace/a.ts",
            "version": 2,
            "diagnostics": [{ "message": "stale" }]
        }));

        assert!(store.report("file:///workspace/a.ts", Some(3)).is_none());
        assert_eq!(
            store
                .report("file:///workspace/a.ts", Some(2))
                .expect("matching version")["items"][0]["message"],
            "stale"
        );
    }

    #[test]
    fn push_diagnostics_accepts_a_versionless_report_as_current() {
        // Servers such as typescript-language-server omit `version` from
        // publishDiagnostics. Every document sync clears the cached record
        // first, so a versionless record present afterwards was published
        // after the sync and is treated as current. An explicit older version
        // is still rejected (see push_diagnostics_rejects_an_older_document_version).
        let store = PushDiagnosticsStore::new();
        store.record(&json!({
            "uri": "file:///workspace/a.ts",
            "diagnostics": [{ "message": "no-version" }]
        }));

        assert_eq!(
            store
                .report("file:///workspace/a.ts", Some(3))
                .expect("versionless report accepted for a min version")["items"][0]["message"],
            "no-version"
        );
        assert_eq!(
            store
                .report("file:///workspace/a.ts", None)
                .expect("versionless report readable without a min")["items"][0]["message"],
            "no-version"
        );
    }

    #[test]
    fn push_diagnostics_bounds_retained_bytes() {
        let store = PushDiagnosticsStore::new();
        store.record(&json!({
            "uri": "file:///workspace/a.ts",
            "version": 3,
            "diagnostics": [{
                "message": "x".repeat(MAX_PUSH_DIAGNOSTIC_BYTES_PER_DOCUMENT + 1)
            }]
        }));

        let report = store
            .report("file:///workspace/a.ts", Some(3))
            .expect("bounded report");
        assert_eq!(report["truncated"], true);
        assert_eq!(report["items"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn push_diagnostics_ignores_notifications_for_other_documents() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let store = PushDiagnosticsStore::new();
            let waiter_store = Arc::clone(&store);
            let waiter = tokio::spawn(async move {
                waiter_store
                    .wait_for("file:///workspace/a.ts", 1_000, Some(3))
                    .await
            });
            tokio::task::yield_now().await;

            store.record(&json!({
                "uri": "file:///workspace/b.ts",
                "version": 3,
                "diagnostics": [{ "message": "other" }]
            }));
            tokio::task::yield_now().await;
            assert!(!waiter.is_finished());

            store.record(&json!({
                "uri": "file:///workspace/a.ts",
                "version": 3,
                "diagnostics": [{ "message": "target" }]
            }));
            let report = waiter
                .await
                .expect("wait task")
                .expect("target diagnostics report");
            assert_eq!(report["items"][0]["message"], "target");
        });
    }

    #[test]
    fn read_loop_survives_lengthless_frame_then_routes_next_response() {
        // A blank-line / length-less frame must NOT tear down the connection;
        // a subsequent well-formed response with a string id should still route.
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
            let (tx, rx) = oneshot::channel();
            pending.lock().await.insert(7, tx);

            let (mut server, client_reader) = duplex(4096);
            // Frame 1: length-less header block (should be skipped, not fatal).
            server.write_all(b"\r\n").await.expect("write empty frame");
            // Frame 2: a real response for id 7.
            let body = br#"{"jsonrpc":"2.0","id":7,"result":{"ok":true}}"#;
            server
                .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
                .await
                .expect("write header");
            server.write_all(body).await.expect("write body");
            drop(server);

            read_loop(
                client_reader,
                Arc::clone(&pending),
                Arc::new(Mutex::new(sink())),
                ClientRequestContext {
                    configuration: Value::Null,
                    workspace_folders: Value::Null,
                },
                ProgressTracker::new(),
                Arc::new(AtomicBool::new(false)),
                PushDiagnosticsStore::new(),
                Arc::new(PartialResultStore::default()),
            )
            .await;

            let result = rx.await.expect("pending response should be completed");
            let value = result.expect("response should be Ok");
            assert_eq!(value.get("ok").and_then(Value::as_bool), Some(true));
        });
    }

    #[test]
    fn concurrent_requests_on_cloned_handle_resolve_out_of_order() {
        // Validates the structural fix in client.rs: a cloned connection handle
        // supports multiple concurrent in-flight requests. Two requests are
        // issued in parallel; the fake server answers the SECOND one first.
        // Both must resolve — proving no head-of-line blocking / serialization
        // and no deadlock once the outer connection mutex guard is dropped.
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            // client_writes: client -> server ; server_writes: server -> client
            let (client_w, mut server_r) = duplex(8192);
            let (mut server_w, client_r) = duplex(8192);

            let conn = Arc::new(JsonRpcConnection::new(
                client_r,
                client_w,
                ClientRequestContext {
                    configuration: Value::Null,
                    workspace_folders: Value::Null,
                },
                ProgressTracker::new(),
            ));

            // Fake server: read both request frames, then respond to id 2 first,
            // then id 1 — exercising out-of-order routing under concurrency.
            let server = tokio::spawn(async move {
                // Drain two request frames (headers + body) loosely by reading
                // a chunk; the exact bytes do not matter for this test.
                let mut buf = vec![0u8; 4096];
                let _ = server_r.read(&mut buf).await;
                // Small wait so both client requests are genuinely in-flight.
                tokio::time::sleep(Duration::from_millis(20)).await;
                for body in [
                    br#"{"jsonrpc":"2.0","id":2,"result":"second"}"#.to_vec(),
                    br#"{"jsonrpc":"2.0","id":1,"result":"first"}"#.to_vec(),
                ] {
                    let header = format!("Content-Length: {}\r\n\r\n", body.len());
                    server_w.write_all(header.as_bytes()).await.expect("hdr");
                    server_w.write_all(&body).await.expect("body");
                    server_w.flush().await.expect("flush");
                }
                // Keep the server end alive a moment so responses are delivered.
                tokio::time::sleep(Duration::from_millis(50)).await;
            });

            let c1 = Arc::clone(&conn);
            let c2 = Arc::clone(&conn);
            let r1 = tokio::spawn(async move { c1.request("a", Value::Null, 5_000).await });
            let r2 = tokio::spawn(async move { c2.request("b", Value::Null, 5_000).await });

            // Both tasks are already running concurrently after spawn; awaiting
            // the handles in sequence collects their results without serializing
            // the in-flight requests themselves.
            let v1 = r1.await.expect("join r1").expect("request 1 ok");
            let v2 = r2.await.expect("join r2").expect("request 2 ok");
            let mut values = [v1.as_str(), v2.as_str()];
            values.sort();
            assert_eq!(values, [Some("first"), Some("second")]);
            server.await.expect("server task");
        });
    }

    #[test]
    fn request_timeout_bounds_a_blocked_write() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let (client_w, _server_r) = duplex(1);
            let (_server_w, client_r) = duplex(8192);
            let conn = JsonRpcConnection::new(
                client_r,
                client_w,
                ClientRequestContext {
                    configuration: Value::Null,
                    workspace_folders: Value::Null,
                },
                ProgressTracker::new(),
            );
            let result = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                conn.request("blocked", Value::Null, 20),
            )
            .await
            .expect("request deadline must include writes");
            assert!(
                result
                    .expect_err("write timed out")
                    .reason
                    .contains("timed out")
            );
            assert!(conn.pending.lock().await.is_empty());
            assert!(!conn.is_alive(), "a partial frame cannot be safely reused");
        });
    }

    #[test]
    fn request_timeout_bounds_a_blocked_cancellation() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let body = serde_json::to_vec(
                &json!({"jsonrpc":"2.0","id":1,"method":"blocked","params":null}),
            )
            .unwrap();
            let frame_size = format!("Content-Length: {}\r\n\r\n", body.len()).len() + body.len();
            let (client_w, _server_r) = duplex(frame_size);
            let (_server_w, client_r) = duplex(8192);
            let conn = JsonRpcConnection::new(
                client_r,
                client_w,
                ClientRequestContext {
                    configuration: Value::Null,
                    workspace_folders: Value::Null,
                },
                ProgressTracker::new(),
            );
            let result = tokio::time::timeout(
                std::time::Duration::from_millis(500),
                conn.request("blocked", Value::Null, 20),
            )
            .await
            .expect("cancellation must not hold a timed-out request open");
            assert!(
                result
                    .expect_err("response timed out")
                    .reason
                    .contains("timed out")
            );
            assert!(conn.pending.lock().await.is_empty());
            assert!(
                !conn.is_alive(),
                "an interrupted cancellation frame cannot be reused"
            );
        });
    }

    #[test]
    fn timed_out_request_emits_cancel_request_to_server() {
        // A request whose response never arrives must (a) return a timeout error
        // and (b) send a `$/cancelRequest` for its id so the server stops working.
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            // client_w -> server_r is the client->server channel we inspect.
            let (client_w, mut server_r) = duplex(8192);
            // server_w -> client_r is never written to (server never responds).
            let (_server_w, client_r) = duplex(8192);

            let conn = Arc::new(JsonRpcConnection::new(
                client_r,
                client_w,
                ClientRequestContext {
                    configuration: Value::Null,
                    workspace_folders: Value::Null,
                },
                ProgressTracker::new(),
            ));

            // Fire a request with a short timeout; the server never answers.
            let result = conn
                .request("textDocument/definition", Value::Null, 50)
                .await;
            assert!(result.is_err(), "request should time out");
            assert!(result.unwrap_err().reason.contains("timed out"));

            // Drain what the client wrote to the server: the original request
            // (id 1) followed by a `$/cancelRequest` for that id.
            let mut buf = vec![0u8; 4096];
            let n = server_r.read(&mut buf).await.expect("read client output");
            let written = String::from_utf8_lossy(&buf[..n]);
            assert!(
                written.contains("$/cancelRequest"),
                "expected a $/cancelRequest frame, got: {written}"
            );
            assert!(
                written.contains("\"id\":1"),
                "cancel must reference the timed-out request id, got: {written}"
            );
        });
    }

    #[test]
    fn request_timeout_marks_the_connection_failed_for_pool_eviction() {
        // A request that blows its full timeout means the server is wedged (not
        // just slow — indexing waits go through wait_for_ready, not here). The
        // connection must transition to not-alive so the pool evicts/restarts it
        // instead of re-handing-out a hung server.
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            // Server never responds: client_r is never written to.
            let (client_w, mut server_r) = duplex(8192);
            let (_server_w, client_r) = duplex(8192);
            let conn = Arc::new(JsonRpcConnection::new(
                client_r,
                client_w,
                ClientRequestContext {
                    configuration: Value::Null,
                    workspace_folders: Value::Null,
                },
                ProgressTracker::new(),
            ));

            assert!(conn.is_alive(), "connection should start alive");
            let result = conn
                .request("textDocument/definition", Value::Null, 50)
                .await;
            assert!(result.is_err(), "request should time out");
            assert!(result.unwrap_err().reason.contains("timed out"));
            assert!(
                !conn.is_alive(),
                "a wedged (timed-out) connection must be marked failed for eviction"
            );

            // The cancellation still went out before the connection was retired.
            let mut buf = vec![0u8; 4096];
            let n = server_r.read(&mut buf).await.expect("read client output");
            assert!(String::from_utf8_lossy(&buf[..n]).contains("$/cancelRequest"));
        });
    }

    #[test]
    fn read_loop_routes_response_with_string_id() {
        // A server echoing the request id as a STRINGIFIED integer must still
        // resolve the matching pending request (finding: lenient id parse).
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
            let (tx, rx) = oneshot::channel();
            pending.lock().await.insert(3, tx);

            let (mut server, client_reader) = duplex(4096);
            let body = br#"{"jsonrpc":"2.0","id":"3","result":42}"#;
            server
                .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
                .await
                .expect("write header");
            server.write_all(body).await.expect("write body");
            drop(server);

            read_loop(
                client_reader,
                Arc::clone(&pending),
                Arc::new(Mutex::new(sink())),
                ClientRequestContext {
                    configuration: Value::Null,
                    workspace_folders: Value::Null,
                },
                ProgressTracker::new(),
                Arc::new(AtomicBool::new(false)),
                PushDiagnosticsStore::new(),
                Arc::new(PartialResultStore::default()),
            )
            .await;

            let result = rx.await.expect("pending response should be completed");
            let value = result.expect("response should be Ok");
            assert_eq!(value.as_u64(), Some(42));
        });
    }

    #[test]
    fn read_loop_rejects_oversized_frame_before_body_allocation() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
            let (tx, rx) = oneshot::channel();
            pending.lock().await.insert(1, tx);

            let (mut server, client_reader) = duplex(1024);
            server
                .write_all(
                    format!(
                        "Content-Length: {}\r\n\r\n",
                        MAX_JSON_RPC_CONTENT_LENGTH + 1
                    )
                    .as_bytes(),
                )
                .await
                .expect("write oversized header");
            drop(server);

            read_loop(
                client_reader,
                Arc::clone(&pending),
                Arc::new(Mutex::new(sink())),
                ClientRequestContext {
                    configuration: Value::Null,
                    workspace_folders: Value::Null,
                },
                ProgressTracker::new(),
                Arc::new(AtomicBool::new(false)),
                PushDiagnosticsStore::new(),
                Arc::new(PartialResultStore::default()),
            )
            .await;

            let result = rx.await.expect("pending response should be completed");
            let error = result.expect_err("oversized frame should fail the request");
            assert!(error.reason.contains("maximum JSON-RPC frame size"));
            assert!(pending.lock().await.is_empty());
        });
    }

    #[test]
    fn oversized_frame_marks_connection_dead() {
        // An oversized Content-Length must fail pending requests AND flip the
        // shared `failed` flag so is_alive() reports the dead connection; a
        // silent early return would leave the pool reusing a crashed client.
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
            let (tx, rx) = oneshot::channel();
            pending.lock().await.insert(1, tx);
            let failed = Arc::new(AtomicBool::new(false));

            let (mut server, client_reader) = duplex(1024);
            server
                .write_all(
                    format!(
                        "Content-Length: {}\r\n\r\n",
                        MAX_JSON_RPC_CONTENT_LENGTH + 1
                    )
                    .as_bytes(),
                )
                .await
                .expect("write oversized header");
            drop(server);

            read_loop(
                client_reader,
                Arc::clone(&pending),
                Arc::new(Mutex::new(sink())),
                ClientRequestContext {
                    configuration: Value::Null,
                    workspace_folders: Value::Null,
                },
                ProgressTracker::new(),
                Arc::clone(&failed),
                PushDiagnosticsStore::new(),
                Arc::new(PartialResultStore::default()),
            )
            .await;

            assert!(
                failed.load(Ordering::Acquire),
                "oversized frame must mark the connection failed (is_alive()==false)"
            );
            let result = rx.await.expect("pending response should be completed");
            assert!(result.is_err(), "pending request must be failed");
        });
    }

    #[test]
    fn read_headers_rejects_an_unbounded_header_line() {
        // A single header line that never terminates must be refused instead of
        // buffered without bound (OOM). We stream more than the per-line cap of
        // non-newline bytes and assert a clean error, not unbounded growth.
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let (mut server, client_reader) = duplex(64 * 1024);
            let writer = tokio::spawn(async move {
                let chunk = vec![b'A'; 16 * 1024];
                // More than MAX_HEADER_LINE_BYTES with no newline in sight.
                let _ = server.write_all(&chunk).await;
                let _ = server.flush().await;
                // Keep the stream open so the reader hits the cap, not EOF.
                tokio::time::sleep(Duration::from_millis(200)).await;
                drop(server);
            });
            let mut reader = BufReader::new(client_reader);
            let outcome = read_headers(&mut reader).await;
            assert!(
                outcome.is_err(),
                "an unterminated oversized header line must error"
            );
            let _ = writer.await;
        });
    }

    #[test]
    fn progress_tracker_caps_active_begin_tokens() {
        // A server that emits an unbounded stream of distinct begin tokens must
        // not grow the active set without limit.
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let tracker = ProgressTracker::new();
            for index in 0..(MAX_ACTIVE_PROGRESS_TOKENS + 50) {
                tracker.on_begin(format!("token-{index}")).await;
            }
            assert_eq!(*tracker.count_rx.borrow(), MAX_ACTIVE_PROGRESS_TOKENS);
        });
    }

    #[test]
    fn progress_tracker_settle_is_bounded_by_caller_timeout() {
        // No on_begin ever called. The settle window must respect a small
        // caller timeout and never block past it (previously the 100 ms settle
        // could also under-wait; here we assert the upper bound is honoured).
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let tracker = ProgressTracker::new();
            let start = Instant::now();
            let readiness = tracker.wait_until_idle(150).await;
            let elapsed = start.elapsed().as_millis();
            // Should wait ~the timeout (settle is capped at timeout_ms=150),
            // and must not run away to the full multi-second settle window.
            assert!(
                elapsed < 1_000,
                "must not exceed caller timeout, got {elapsed} ms"
            );
            // Never saw progress -> only the settle window elapsed.
            assert_eq!(readiness, Readiness::SilentServer);
        });
    }

    #[test]
    fn progress_tracker_waits_full_settle_when_no_progress_and_ample_timeout() {
        // A server that indexes WITHOUT progress events: no on_begin arrives,
        // but with an ample timeout we must NOT return after only ~100 ms —
        // we give the silent indexer the conservative settle window.
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let tracker = ProgressTracker::new();
            let start = Instant::now();
            let readiness = tracker.wait_until_idle(10_000).await;
            let elapsed = start.elapsed().as_millis();
            assert!(
                elapsed >= 1_500,
                "must not return after the old aggressive 100 ms window, got {elapsed} ms"
            );
            assert!(elapsed < 5_000, "must stay bounded, got {elapsed} ms");
            // No progress events ever arrived -> settledWithoutProgress, not progressIdle.
            assert_eq!(readiness, Readiness::SilentServer);
        });
    }

    #[test]
    fn progress_tracker_waits_for_active_token_then_returns_idle() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let tracker = ProgressTracker::new();
            let t = Arc::clone(&tracker);
            tokio::spawn(async move {
                t.on_begin("indexing".to_owned()).await;
                tokio::time::sleep(Duration::from_millis(50)).await;
                t.on_end("indexing").await;
            });
            let readiness = tracker.wait_until_idle(5_000).await;
            assert_eq!(*tracker.count_rx.borrow(), 0);
            // Saw a full progress cycle and drained it -> progressIdle.
            assert_eq!(readiness, Readiness::ProgressIdle);
        });
    }

    #[test]
    fn progress_tracker_times_out_when_token_never_ends() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let tracker = ProgressTracker::new();
            tracker.on_begin("stuck".to_owned()).await;
            let start = Instant::now();
            let readiness = tracker.wait_until_idle(300).await;
            let elapsed = start.elapsed().as_millis();
            assert!(
                elapsed >= 200,
                "must wait at least ~timeout ms, got {elapsed} ms"
            );
            assert!(elapsed < 3_000, "must not hang, got {elapsed} ms");
            // Token never ended before the deadline -> timeout.
            assert_eq!(readiness, Readiness::Timeout);
        });
    }

    #[test]
    fn progress_tracker_completed_cycle_still_requires_quiescence() {
        // A full begin/end cycle happens before wait_until_idle is called; the
        // an idle observation must still bridge subsequent progress waves.
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let tracker = ProgressTracker::new();
            tracker.on_begin("indexing".to_owned()).await;
            tracker.on_end("indexing").await;
            let started = Instant::now();
            let readiness = tracker.wait_until_idle(5_000).await;
            assert_eq!(readiness, Readiness::ProgressIdle);
            assert!(started.elapsed() >= Duration::from_millis(150));
        });
    }

    #[test]
    fn progress_tracker_cannot_confirm_idle_without_full_quiescence_budget() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let tracker = ProgressTracker::new();
            tracker.on_begin("indexing".to_owned()).await;
            tracker.on_end("indexing").await;
            assert_eq!(tracker.wait_until_idle(20).await, Readiness::Timeout);
        });
    }

    #[test]
    fn progress_tracker_quiescence_restarts_for_a_followup_wave() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let tracker = ProgressTracker::new();
            tracker.on_begin("first".to_owned()).await;
            tracker.on_end("first").await;
            let followup = Arc::clone(&tracker);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                followup.on_begin("second".to_owned()).await;
                tokio::time::sleep(Duration::from_millis(200)).await;
                followup.on_end("second").await;
            });
            let started = Instant::now();
            assert_eq!(
                tracker.wait_until_idle(2_000).await,
                Readiness::ProgressIdle
            );
            assert!(started.elapsed() >= Duration::from_millis(400));
        });
    }
}
