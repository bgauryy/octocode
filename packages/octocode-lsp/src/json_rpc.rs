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
    pub async fn wait_until_idle(&self, timeout_ms: u64) {
        /// Wait this long for the very first `$/progress begin` after
        /// `initialized` is sent.  Servers that don't use progress return
        /// immediately after this window expires with no activity.
        const SETTLE_MS: u64 = 100;
        /// After count reaches 0, wait this long for any follow-up wave before
        /// declaring the server idle.  Sized to bridge the typical gap between
        /// rust-analyzer progress waves (~10-100 ms in practice).
        const QUIESCE_MS: u64 = 200;

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);

        // Fast path: all progress already completed before we were called.
        if self.ever_active.load(Ordering::Acquire) && *self.count_rx.borrow() == 0 {
            return;
        }

        let mut rx = self.count_rx.clone();

        // Phase 1 -- settle: wait briefly for the first $/progress begin.
        if *rx.borrow() == 0 && !self.ever_active.load(Ordering::Acquire) {
            let settle = Duration::from_millis(SETTLE_MS.min(timeout_ms));
            let became_active = tokio::time::timeout(settle, rx.wait_for(|c| *c > 0))
                .await
                .is_ok();
            if !became_active {
                return; // Server does not use progress -- consider it ready.
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
                return; // Deadline expired while tokens were still active.
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
                return; // Quiescence passed -- server is idle.
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
        let Ok(Some(content_length)) = read_headers(&mut reader).await else {
            break;
        };
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
        let Some(id) = value.get("id").and_then(Value::as_u64) else {
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

async fn read_headers<R>(reader: &mut BufReader<R>) -> std::io::Result<Option<usize>>
where
    R: AsyncRead + Unpin,
{
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            return Ok(content_length);
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
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
    fn progress_tracker_returns_immediately_when_never_active() {
        // No on_begin ever called — wait_until_idle must return within
        // the settle window (100 ms), not spin for the full timeout.
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let tracker = ProgressTracker::new();
            let start = Instant::now();
            tracker.wait_until_idle(2_000).await;
            assert!(start.elapsed().as_millis() < 1_000);
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
            tracker.wait_until_idle(5_000).await;
            assert_eq!(*tracker.count_rx.borrow(), 0);
        });
    }

    #[test]
    fn progress_tracker_times_out_when_token_never_ends() {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async {
            let tracker = ProgressTracker::new();
            tracker.on_begin("stuck".to_owned()).await;
            let start = Instant::now();
            tracker.wait_until_idle(300).await;
            let elapsed = start.elapsed().as_millis();
            assert!(elapsed >= 200, "must wait at least ~timeout ms, got {elapsed} ms");
            assert!(elapsed < 3_000, "must not hang, got {elapsed} ms");
        });
    }
}
