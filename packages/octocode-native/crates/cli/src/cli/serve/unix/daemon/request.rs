//! One connection: read the request line, run it, write the reply.

use super::{Daemon, MAX_REQUEST_BYTES};
use crate::cli::output::Printed;
use crate::cli::serve::runs_on_server;
use crate::cli::serve::unix::frames::{Backlog, Frames, Gone, progress_scope};
use crate::cli::serve::unix::read_line_bounded;
use crate::cli::serve::unix::served::Served;
use crate::cli::serve::unix::sockets::touch;
use octocode_native::id::ToolId;
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

/// The reply to `request`: the call as printed with the notices it raised,
/// or a `fallback`; `None` when the client left before it.
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
        let (served, tool, json_out, input) = match served(daemon, request).await {
            Ok(served) => served,
            Err(reason) => return Some(json!({ "fallback": reason })),
        };
        // A client that left while its runtime was built asked for nothing.
        if left.get() {
            return None;
        }
        let query = input.clone();
        // The client's directory, which `served` read.
        let cwd = request["cwd"].as_str().map(PathBuf::from);
        let runtime = &served.runtime;
        let printed = match runtime.admit(id.to_owned()) {
            Err(error) => Printed::finish(&tool, Err(error), json_out, &query, None),
            Ok(admission) => {
                *running.borrow_mut() = Some(std::rc::Rc::clone(&served));
                let call = runtime.execute(
                    admission,
                    tool.clone(),
                    input,
                    octocode_native::runtime::Channel::Cli,
                );
                let result = notices.run(call).await;
                Printed::finish(&tool, result, json_out, &query, cwd.as_deref())
            }
        };
        let mut reply = printed.to_json();
        reply["notices"] = json!(notices.notices());
        Some(reply)
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

/// The runtime, tool, output mode and input of `request`, or why this
/// server does not run it.
async fn served(
    daemon: &Daemon,
    request: &mut Value,
) -> Result<(std::rc::Rc<Served>, String, bool, Value), String> {
    let (Some(tool), Some(json_out), Some(cwd), Some(identity), Ok(env)) = (
        request["tool"].as_str().map(str::to_owned),
        request["json"].as_bool(),
        request["cwd"].as_str().map(PathBuf::from),
        request["identity"].as_str().map(str::to_owned),
        serde_json::from_value::<BTreeMap<String, String>>(request["env"].take()),
    ) else {
        return Err("the request was not one this server reads".into());
    };
    if !ToolId::from_name(&tool).is_some_and(runs_on_server) {
        return Err(format!("it does not run {tool}"));
    }
    let served = daemon.runtime_for(&cwd, env, &identity).await?;
    Ok((served, tool, json_out, request["input"].take()))
}
