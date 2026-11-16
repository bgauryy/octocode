//! One connection: read the request line, run it, write the reply.

use super::{Daemon, MAX_REQUEST_BYTES};
use crate::cli::output::Printed;
use crate::cli::serve::runs_on_server;
use crate::cli::serve::unix::frames::{
    Backlog, Frames, Gone, MCP, MCP_ERROR, PREWARM, progress_scope,
};
use crate::cli::serve::unix::identity::own_process_identity;
use crate::cli::serve::unix::read_line_bounded;
use crate::cli::serve::unix::served::Served;
use crate::cli::serve::unix::sockets::touch;
use crate::cli::serve::unix::surface::Surface;
use octocode_native::id::ToolId;
use octocode_native::runtime::RuntimeError;
use octocode_native::runtime::error::{McpResult, mcp_result};
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Await `work` while its progress and heartbeats go to the client as
/// frames, unless the client leaves first: it never writes after its
/// request, so a read that returns (end of stream on Ctrl-C or exit) means
/// it left, as does a client that takes no byte for the frames' silence.
/// Then the frames still queued are dropped, `cancel` runs, the cancelled
/// work drains, and the result is `None`. A result is followed by the
/// progress the client was not yet sent, so the caller's reply comes last.
pub(in crate::cli::serve::unix) async fn attended<T>(
    work: impl std::future::Future<Output = T>,
    client: &mut (impl tokio::io::AsyncRead + Unpin),
    frames: &mut Frames<impl tokio::io::AsyncWrite + Unpin>,
    backlog: &Backlog,
    cancel: impl FnOnce(),
) -> Option<T> {
    use tokio::io::AsyncReadExt;
    tokio::pin!(work);
    let mut probe = [0u8; 1];
    let ran = tokio::select! {
        ran = frames.run(work.as_mut(), backlog) => ran,
        _ = client.read(&mut probe) => Err(Gone),
    };
    match ran {
        Ok(result) => frames.drain(backlog).await.ok().map(|()| result),
        Err(Gone) => {
            cancel();
            let _ = work.await;
            None
        }
    }
}

/// One connection: read the request line, run it on the runtime of the
/// client's directory and environment in the client's output mode, and
/// write the printed call back. A request this server cannot answer as the
/// client would run it gets a `fallback` reply, so the client runs it in
/// process. A [`STOP`](super::sockets::STOP) request stops the server.
/// `asked` runs once the request line is read. Whether the client asked
/// anything: `false` for a connection that closed without a request line,
/// or sent none before the server began to leave.
pub(in crate::cli::serve::unix) async fn handle(
    daemon: &Daemon,
    stream: tokio::net::UnixStream,
    id: String,
    asked: impl FnOnce(),
) -> bool {
    use tokio::io::BufReader;
    let (read, write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let line = tokio::select! {
        line = read_line_bounded(&mut reader, MAX_REQUEST_BYTES) => line,
        () = daemon.leaving() => None,
    };
    let Some(line) = line else {
        return false;
    };
    asked();
    touch(&daemon.socket);
    let request = serde_json::from_str::<Value>(&line);
    if let Ok(request) = &request
        && request["stop"] == true
    {
        if request["drain"] == true {
            daemon.wait_for_stop(write);
        }
        daemon.stop.notify_one();
        return true;
    }
    let mut frames = Frames::new(write, daemon.pace);
    let reply = match request {
        Ok(mut request) => {
            let answered = answer(daemon, &mut request, &id, &mut reader, &mut frames).await;
            let Some(reply) = answered else {
                return true;
            };
            reply
        }
        Err(_) => json!({ "fallback": "the request was not JSON" }),
    };
    let _ = frames.send(&reply).await;
    true
}

/// What a request asks of the runtime it names.
enum Asked {
    /// Run `tool` on `input`; the CLI surface also names its output mode.
    Tool {
        tool: String,
        json_out: bool,
        input: Value,
    },
    /// Warm the workspace's language servers ([`PREWARM`]).
    Prewarm,
}

/// The reply to `request`: the call as printed with the notices it raised
/// (CLI), or as an MCP result (MCP), or a `fallback`; `None` when the
/// client left before it.
async fn answer(
    daemon: &Daemon,
    request: &mut Value,
    id: &str,
    client: &mut (impl tokio::io::AsyncRead + Unpin),
    frames: &mut Frames<impl tokio::io::AsyncWrite + Unpin>,
) -> Option<Value> {
    // Its progress goes to the client as frames while it runs; the other
    // notices it raises, and only those, go in its reply.
    let (notices, backlog) = progress_scope();
    let left = std::cell::Cell::new(false);
    let running = std::cell::RefCell::new(None::<std::rc::Rc<Served>>);
    let work = async {
        let (served, surface, asked) = match served(daemon, request).await {
            Ok(served) => served,
            Err(reason) => return Some(json!({ "fallback": reason })),
        };
        // A client that left while its runtime was built asked for nothing.
        if left.get() {
            return None;
        }
        let Asked::Tool {
            tool,
            json_out,
            input,
        } = asked
        else {
            served
                .runtime
                .prewarm_lsp_workspace(&tokio::runtime::Handle::current());
            return Some(json!({ "prewarmed": true }));
        };
        let query = input.clone();
        // The client's directory, which `served` read.
        let cwd = request["cwd"].as_str().map(PathBuf::from);
        let runtime = &served.runtime;
        let admitted = match surface {
            Surface::Cli => runtime.admit(id.to_owned()),
            Surface::Mcp => runtime.admit_with(id.to_owned(), None),
        };
        let result = match admitted {
            Err(error) => Err(error),
            Ok(admission) => {
                *running.borrow_mut() = Some(std::rc::Rc::clone(&served));
                let channel = match surface {
                    Surface::Cli => octocode_native::runtime::Channel::Cli,
                    Surface::Mcp => octocode_native::runtime::Channel::Mcp,
                };
                let call = runtime.execute(admission, tool.clone(), input, channel);
                notices.run(call).await
            }
        };
        Some(match surface {
            Surface::Cli => {
                let printed = Printed::finish(&tool, result, json_out, &query, cwd.as_deref());
                let mut reply = printed.to_json();
                reply["notices"] = json!(notices.notices());
                reply
            }
            // The text the in-process MCP server sends, from the same
            // function; its notices stay on the server.
            Surface::Mcp => mcp_reply(mcp_result(&tool, result)),
        })
    };
    let cancel = || {
        left.set(true);
        if let Some(served) = running.borrow().as_ref() {
            served.runtime.requests.cancel(id);
        }
    };
    attended(work, client, frames, &backlog, cancel)
        .await
        .flatten()
}

/// An MCP call's result as its reply line: `{"mcp":{"text","isError"}}`, or
/// `{"mcpError":{"code","message"}}` for the calls MCP answers as protocol
/// errors (cancelled, closing).
fn mcp_reply(result: Result<McpResult, RuntimeError>) -> Value {
    match result {
        Ok(McpResult { text, is_error }) => {
            json!({ MCP: { "text": text, "isError": is_error } })
        }
        Err(error) => {
            json!({ MCP_ERROR: { "code": error.code, "message": error.message } })
        }
    }
}

/// The runtime, surface and ask of `request`, or why this server does not
/// run it.
async fn served(
    daemon: &Daemon,
    request: &mut Value,
) -> Result<(std::rc::Rc<Served>, Surface, Asked), String> {
    let (Some(surface), Some(cwd), Some(identity), Ok(env)) = (
        request["surface"].as_str().and_then(Surface::parse),
        request["cwd"].as_str().map(PathBuf::from),
        request["identity"].as_str().map(str::to_owned),
        serde_json::from_value::<BTreeMap<String, String>>(request["env"].take()),
    ) else {
        return Err("the request was not one this server reads".into());
    };
    let asked = if request[PREWARM] == true {
        if surface != Surface::Mcp {
            return Err("only an MCP session asks to prewarm".into());
        }
        Asked::Prewarm
    } else {
        let (Some(tool), Some(json_out)) = (
            request["tool"].as_str().map(str::to_owned),
            request["json"].as_bool(),
        ) else {
            return Err("the request was not one this server reads".into());
        };
        if !ToolId::from_name(&tool).is_some_and(runs_on_server) {
            return Err(format!("it does not run {tool}"));
        }
        Asked::Tool {
            tool,
            json_out,
            input: request["input"].take(),
        }
    };
    if request["process"].as_str() != Some(own_process_identity().as_str()) {
        return Err("it runs under another process environment than this call".into());
    }
    let served = daemon.runtime_for(surface, &cwd, env, &identity).await?;
    Ok((served, surface, asked))
}
