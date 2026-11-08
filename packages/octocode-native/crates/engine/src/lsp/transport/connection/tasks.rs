//! The connection's two tasks: the single writer that owns the server's stdin
//! and the reader that owns stdout and routes every inbound message.

use super::super::codec::{Frame, encode_frame, read_frame};
use super::super::progress::{HealthLevel, ProgressTracker, ServerHealth};
use super::super::server_requests::{
    ClientRequestContext, client_response_for, declined_build_import,
};
use super::NOTIFY_BASE_DEADLINE_MS;
use super::shared::{FailOnExit, Outbound, Shared};
use crate::error::{Error, RpcError};
use serde_json::{Map, Value};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

/// How long the reader may wait for writer-queue space to reply to a server
/// request before declaring the server wedged (it is not reading stdin).
const REPLY_ENQUEUE_DEADLINE_MS: u64 = 1_000;

pub(super) async fn write_loop<W>(
    mut writer: W,
    mut frames: mpsc::Receiver<Outbound>,
    mut exit: FailOnExit,
) where
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

pub(super) async fn read_loop<R>(
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

/// One jdtls report on loading the project.
enum ProjectLoad {
    /// It could not import a project, or reported an `Error` status.
    Failed(ServerHealth),
    /// Its project status is `OK` again.
    Recovered,
}

impl ProjectLoad {
    /// The server's health this report leaves.
    fn health(self) -> Option<ServerHealth> {
        match self {
            Self::Failed(health) => Some(health),
            Self::Recovered => None,
        }
    }
}

/// What jdtls says about loading the project: [`ProjectLoad::Failed`] when
/// it could not import a project (scoped to that project's directory; a
/// `language/eventNotification` naming the project and why, sent when
/// Buildship cannot run its Gradle with this JDK) or reports an `Error`
/// status; [`ProjectLoad::Recovered`] when its project status is `OK` again;
/// `None` for anything else, which leaves the last report standing (a
/// `WARNING` project status follows the failed import and says nothing new).
fn project_load_report(method: &str, params: Option<&Value>) -> Option<ProjectLoad> {
    let params = params?;
    let text = |field: &str| params.get(field).and_then(Value::as_str);
    if method == "language/eventNotification" {
        let data = params.get("data")?;
        let message = data.get("message").and_then(Value::as_str)?;
        let project = data.get("projectUri").and_then(Value::as_str)?;
        let project = crate::lsp::uri::uri_to_path(project).unwrap_or_else(|_| project.to_owned());
        return Some(ProjectLoad::Failed(ServerHealth {
            level: HealthLevel::Error,
            message: Some(format!(
                "The server could not import the project at {project}: {message} Its answers for that project may be empty or incomplete."
            )),
            project: Some(project),
        }));
    }
    match (text("type")?, text("message")) {
        ("ProjectStatus", Some("OK")) => Some(ProjectLoad::Recovered),
        ("Error", message) => Some(ProjectLoad::Failed(ServerHealth {
            level: HealthLevel::Error,
            message: message.map(str::to_owned),
            project: None,
        })),
        _ => None,
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
                note_watch_registration(&method, params.as_ref(), shared);
                if let Some(cost) = declined_build_import(&method, params.as_ref()) {
                    progress.set_health(Some(ServerHealth {
                        level: HealthLevel::Warning,
                        message: Some(cost.to_owned()),
                        project: None,
                    }));
                }
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
                let field = |name| params.as_ref().and_then(|params| params.get(name));
                // Before the end of a load is seen: a waiter it wakes reads this.
                if let Some(level) = field("health").and_then(Value::as_str) {
                    let message = field("message").and_then(Value::as_str);
                    progress.on_server_health(level, message);
                }
                if let Some(quiescent) = field("quiescent").and_then(Value::as_bool) {
                    progress.on_server_status(quiescent).await;
                }
            }
            (None, "language/eventNotification" | "language/status") => {
                if let Some(report) = project_load_report(&method, params.as_ref()) {
                    progress.set_health(report.health());
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
        // The server answered a live request: it is slow at worst, not wedged.
        shared.consecutive_timeouts.store(0, Ordering::Release);
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
        Some("report") => progress.on_report(),
        _ => {}
    }
}

/// Record whether the server (un)registered for
/// `workspace/didChangeWatchedFiles`: the client then sends file events
/// for files the server has not opened (see
/// `NativeLspClient::watched_files_diff`).
fn note_watch_registration(method: &str, params: Option<&Value>, shared: &Shared) {
    const WATCHED_FILES: &str = "workspace/didChangeWatchedFiles";
    let (key, registers) = match method {
        "client/registerCapability" => ("registrations", true),
        "client/unregisterCapability" => ("unregisterations", false),
        _ => return,
    };
    let named = params
        .and_then(|params| params.get(key))
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item.get("method").and_then(Value::as_str) == Some(WATCHED_FILES))
        });
    if named {
        shared.watches_files.store(registers, Ordering::Release);
    }
}
