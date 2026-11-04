//! One connection: read the request line, run it, write the reply.

use super::{Daemon, MAX_REQUEST_BYTES};
use crate::cli::Printed;
use crate::cli::serve::runs_on_server;
use crate::cli::serve::unix::frames::{forward, progress_scope};
use crate::cli::serve::unix::read_line_bounded;
use crate::cli::serve::unix::served::Served;
use crate::cli::serve::unix::sockets::touch;
use octocode_native::id::ToolId;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Await `execution` unless the client leaves first: it never writes after
/// its request, so a read that returns (end of stream on Ctrl-C or exit)
/// means it left. Then `cancel` runs, the cancelled work drains, and the
/// result is `None`.
pub(in crate::cli::serve::unix) async fn unless_left<T>(
    execution: impl std::future::Future<Output = T>,
    client: &mut (impl tokio::io::AsyncRead + Unpin),
    cancel: impl FnOnce(),
) -> Option<T> {
    use tokio::io::AsyncReadExt;
    tokio::pin!(execution);
    let mut probe = [0u8; 1];
    tokio::select! {
        result = &mut execution => Some(result),
        _ = client.read(&mut probe) => {
            cancel();
            let _ = execution.await;
            None
        }
    }
}

/// One connection: read the request line, run it on the runtime of the
/// client's directory and environment in the client's output mode, and
/// write the printed call back. A request this server cannot answer as the
/// client would run it gets a `fallback` reply, so the client runs it in
/// process. A [`STOP`](super::sockets::STOP) request stops the server.
/// Whether the client asked anything: `false` for a connection that closed
/// without a request line.
pub(in crate::cli::serve::unix) async fn handle(
    daemon: &Daemon,
    stream: tokio::net::UnixStream,
    id: String,
) -> bool {
    use tokio::io::{AsyncWriteExt, BufReader};
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let Some(line) = read_line_bounded(&mut reader, MAX_REQUEST_BYTES).await else {
        return false;
    };
    touch(&daemon.socket);
    let reply = match serde_json::from_str::<Value>(&line) {
        Ok(request) if request["stop"] == true => {
            if request["drain"] == true {
                daemon.stopped.borrow_mut().push(write);
            }
            daemon.stop.notify_one();
            return true;
        }
        Ok(mut request) => match served(daemon, &mut request).await {
            Ok((served, tool, json_out, input)) => {
                let runtime = &served.runtime;
                // Its progress goes to the client as frames while it runs;
                // the other notices it raises, and only those, go in its reply.
                let (notices, mut progress) = progress_scope();
                let query = input.clone();
                // The client's directory, which `served` read.
                let cwd = request["cwd"].as_str().map(PathBuf::from);
                let printed = match runtime.admit(id.clone()) {
                    Err(error) => Printed::finish(&tool, Err(error), json_out, &query, None),
                    Ok(admission) => {
                        let call = notices.run(runtime.execute(
                            admission,
                            tool.clone(),
                            input,
                            octocode_native::runtime::Channel::Cli,
                        ));
                        let execution = forward(call, &mut progress, &mut write);
                        let cancel = || {
                            runtime.requests.cancel(&id);
                        };
                        let Some(result) = unless_left(execution, &mut reader, cancel).await else {
                            return true;
                        };
                        Printed::finish(&tool, result, json_out, &query, cwd.as_deref())
                    }
                };
                let mut reply = printed.to_json();
                reply["notices"] = json!(notices.notices());
                reply
            }
            Err(reason) => json!({ "fallback": reason }),
        },
        Err(_) => json!({ "fallback": "the request was not JSON" }),
    };
    let mut line = reply.to_string();
    line.push('\n');
    let _ = write.write_all(line.as_bytes()).await;
    true
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
