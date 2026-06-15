use napi::{Error, Result, Status};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{oneshot, Mutex};
use tokio::time::{timeout, Duration};

type PendingMap = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;
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
    pub fn new<R>(reader: R, writer: W, context: ClientRequestContext) -> Self
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
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
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
        if let Some(method) = value.get("method").and_then(Value::as_str) {
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
