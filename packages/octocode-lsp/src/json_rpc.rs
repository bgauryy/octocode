use napi::{Error, Result, Status};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Mutex};
use tokio::time::{timeout, Duration};

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;

pub struct JsonRpcConnection<W>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    writer: Arc<Mutex<W>>,
    next_id: AtomicU64,
    pending: PendingMap,
}

impl<W> JsonRpcConnection<W>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    pub fn new<R>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        tokio::spawn(read_loop(reader, Arc::clone(&pending)));
        Self {
            writer: Arc::new(Mutex::new(writer)),
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
        let body = serde_json::to_vec(message).map_err(|err| {
            Error::new(
                Status::GenericFailure,
                format!("Serialize JSON-RPC failed: {err}"),
            )
        })?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        let mut writer = self.writer.lock().await;
        writer
            .write_all(header.as_bytes())
            .await
            .map_err(io_error)?;
        writer.write_all(&body).await.map_err(io_error)?;
        writer.flush().await.map_err(io_error)
    }
}

async fn read_loop<R>(reader: R, pending: PendingMap)
where
    R: AsyncRead + Unpin,
{
    let mut reader = BufReader::new(reader);
    loop {
        let Ok(Some(content_length)) = read_headers(&mut reader).await else {
            break;
        };
        let mut body = vec![0u8; content_length];
        if reader.read_exact(&mut body).await.is_err() {
            break;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&body) else {
            continue;
        };
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

fn io_error(err: std::io::Error) -> Error {
    Error::new(Status::GenericFailure, err.to_string())
}
