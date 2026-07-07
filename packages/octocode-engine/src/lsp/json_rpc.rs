use napi::{Error, Result, Status};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, watch, Mutex};
use tokio::time::{timeout, Duration, Instant};

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;
const MAX_JSON_RPC_CONTENT_LENGTH: usize = 64 * 1024 * 1024;

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
    /// Never saw any `$/progress`; only the settle window elapsed. The server
    /// does not report indexing, so we cannot confirm it has finished.
    SettledFallback,
    /// `$/progress` was still active when `timeout_ms` expired — the server is
    /// (as far as we know) still indexing.
    Timeout,
}

impl Readiness {
    /// Stable string form crossing the napi boundary into JS.
    pub fn as_str(self) -> &'static str {
        match self {
            Readiness::ProgressIdle => "progressIdle",
            Readiness::SettledFallback => "settledFallback",
            Readiness::Timeout => "timeout",
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

        // Fast path: all progress already completed before we were called.
        if self.ever_active.load(Ordering::Acquire) && *self.count_rx.borrow() == 0 {
            return Readiness::ProgressIdle;
        }

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
                return Readiness::SettledFallback;
            }
        }

        // Phase 2 -- drain + quiesce loop: repeat until we observe a full
        // QUIESCE_MS window with no active tokens and no new ones starting.
        loop {
            // 2a. Wait for count to reach zero.
            let remaining = deadline.saturating_duration_since(Instant::now());
            if tokio::time::timeout(remaining, rx.wait_for(|c| *c == 0))
                .await
                .is_err()
            {
                return Readiness::Timeout; // Deadline expired while tokens active.
            }

            // 2b. Quiesce: wait briefly to see if a new wave starts.
            let quiesce = Duration::from_millis(
                QUIESCE_MS.min(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis() as u64,
                ),
            );
            let new_wave = tokio::time::timeout(quiesce, rx.wait_for(|c| *c > 0))
                .await
                .is_ok();
            if !new_wave {
                return Readiness::ProgressIdle; // Quiescence passed -- server is idle.
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
        tokio::spawn(read_loop(
            reader,
            Arc::clone(&pending),
            Arc::clone(&writer),
            context,
            Arc::clone(&progress),
        ));
        Self {
            writer,
            next_id: AtomicU64::new(1),
            pending,
        }
    }

    pub async fn request(&self, method: &str, params: Value, timeout_ms: u32) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        let message = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        if let Err(err) = self.write_message(&message).await {
            self.pending.lock().await.remove(&id);
            return Err(err);
        }
        match timeout(Duration::from_millis(u64::from(timeout_ms)), rx).await {
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
                let _ = self.notify("$/cancelRequest", json!({ "id": id })).await;
                Err(Error::new(
                    Status::GenericFailure,
                    format!("LSP request timed out after {timeout_ms}ms"),
                ))
            }
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<()> {
        let message = json!({"jsonrpc":"2.0","method":method,"params":params});
        self.write_message(&message).await
    }

    async fn write_message(&self, message: &Value) -> Result<()> {
        write_message(&self.writer, message).await
    }
}

async fn read_loop<R, W>(
    reader: R,
    pending: PendingMap,
    writer: SharedWriter<W>,
    context: ClientRequestContext,
    progress: Arc<ProgressTracker>,
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
                handle_progress_notification(&value, &progress).await;
            }
            if let Some(id) = value.get("id").cloned() {
                let response = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": client_response_for(method, value.get("params"), &context),
                });
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

async fn handle_progress_notification(value: &Value, progress: &Arc<ProgressTracker>) {
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
        _ => {}
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

fn client_response_for(
    method: &str,
    params: Option<&Value>,
    context: &ClientRequestContext,
) -> Value {
    match method {
        "workspace/configuration" => {
            let item_count = params
                .and_then(|value| value.get("items"))
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            Value::Array(
                (0..item_count)
                    .map(|_| context.configuration.clone())
                    .collect(),
            )
        }
        "workspace/workspaceFolders" => context.workspace_folders.clone(),
        "workspace/applyEdit" => json!({ "applied": false }),
        "client/registerCapability"
        | "client/unregisterCapability"
        | "window/showMessageRequest"
        | "workDoneProgress/create" => Value::Null,
        _ => Value::Null,
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
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            return Ok(HeaderOutcome::Eof);
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
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.trim().eq_ignore_ascii_case("Content-Length") {
                content_length = value.trim().parse::<usize>().ok();
            }
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
            )
            .await;

            let result = rx.await.expect("pending response should be completed");
            let error = result.expect_err("oversized frame should fail the request");
            assert!(error.reason.contains("maximum JSON-RPC frame size"));
            assert!(pending.lock().await.is_empty());
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
            assert_eq!(readiness, Readiness::SettledFallback);
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
            // No progress events ever arrived -> settledFallback, not progressIdle.
            assert_eq!(readiness, Readiness::SettledFallback);
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
    fn progress_tracker_fast_path_reports_idle_when_cycle_completed_before_call() {
        // A full begin/end cycle happens before wait_until_idle is called; the
        // fast path must report progressIdle (not settledFallback).
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let tracker = ProgressTracker::new();
            tracker.on_begin("indexing".to_owned()).await;
            tracker.on_end("indexing").await;
            let readiness = tracker.wait_until_idle(5_000).await;
            assert_eq!(readiness, Readiness::ProgressIdle);
        });
    }
}
