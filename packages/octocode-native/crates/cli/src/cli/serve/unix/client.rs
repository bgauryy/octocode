//! The client side of a warm call: resolve the server for this call, start
//! it when none answers, send the request and print the reply.

use super::identity::{call_identity, exe_identity, served_env, server_key, workspace_of};
use super::sockets::socket_in;
use super::{read_line_bounded, same_dir_form};
use crate::cli::Printed;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Largest reply the client accepts from a server; a longer one falls back to
/// running the tool in this process, like any other failed exchange.
pub(super) const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;

/// How long a client waits for the reply: the server's own request budget
/// plus a margin, so a server that ran out of time has said so first. Past
/// it the call is over; running it again in process would double the wait.
pub(super) const REPLY_DEADLINE: std::time::Duration =
    octocode_native::execution::DEFAULT_TIMEOUT.saturating_add(std::time::Duration::from_secs(15));

/// Why an exchange with a server produced no printed call.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Failed {
    /// The server closed, refused, or sent no reply line: the call has not
    /// run here, so it runs in process.
    Fallback(String),
    /// No reply within the deadline.
    Deadline,
    /// Ctrl-C: the server was told to cancel, then drained or abandoned.
    Interrupted(crate::cli::Interrupted),
}

/// Run `tool` through the workspace's server when one is reachable or can
/// be started, and print it: the exit code. `None` means the caller runs it
/// in process.
pub(in crate::cli) async fn call(
    tool: &str,
    input: &Value,
    mode: crate::cli::OutputMode,
) -> Option<u8> {
    let json_out = mode.json;
    let target = target()?;
    let mut stream = match connect(&target.socket).await {
        Some(stream) => stream,
        None => {
            spawn_server(&target.socket, &target.workspace)?;
            wait_for(&target.socket).await?
        }
    };
    let request = Request {
        tool,
        input,
        json_out,
        cwd: &target.cwd,
        env: &target.env,
        identity: &target.identity,
    };
    // An in-process call prints its configuration warnings at start.
    let warn = || octocode_native::runtime::ToolRuntime::print_startup_diagnostics(&target.config);
    let interrupted = || {
        warn();
        eprintln!("{}", crate::cli::CANCELLING);
    };
    let first = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    let exchanged = exchange(
        &mut stream,
        &request,
        REPLY_DEADLINE,
        first,
        tokio::signal::ctrl_c(),
        interrupted,
    )
    .await;
    let (printed, notices) = match exchanged {
        Ok(reply) => (reply.printed, reply.notices),
        Err(Failed::Fallback(reason)) => {
            eprintln!("octocode: warm server failed ({reason}); running {tool} in process.");
            return None;
        }
        Err(Failed::Interrupted(_)) => return Some(130),
        Err(Failed::Deadline) => (
            Printed::finish(
                tool,
                Err(octocode_native::runtime::RuntimeError {
                    code: "timeout".into(),
                    message: format!(
                        "The warm server sent no {tool} reply within {} s; the call was not run again in process.",
                        REPLY_DEADLINE.as_secs()
                    ),
                    payload: None,
                    validation_issues: None,
                }),
                json_out,
                &serde_json::Value::Null,
            ),
            Vec::new(),
        ),
    };
    warn();
    // What the call said on stderr while it ran, as in process.
    for notice in notices {
        eprintln!("{notice}");
    }
    Some(printed.print(mode))
}

/// A server's answer: the call as printed, and the notices it raised
/// ([`octocode_io::notice`]), which an in-process call prints on stderr.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Reply {
    pub(super) printed: Printed,
    pub(super) notices: Vec<String>,
}

/// One call as sent to a server.
pub(super) struct Request<'a> {
    pub(super) tool: &'a str,
    pub(super) input: &'a Value,
    /// The output mode the server renders in.
    pub(super) json_out: bool,
    /// The client's working directory: the server runs the call as a CLI
    /// started there.
    pub(super) cwd: &'a Path,
    /// The client's environment ([`served_env`]): the call runs with it.
    /// The socket is the owner's only, so its secrets stay with the user.
    pub(super) env: &'a BTreeMap<String, String>,
    /// The client's [`call_identity`]: the server answers only under it.
    pub(super) identity: &'a str,
}

/// Where and as what a call goes to a server.
pub(super) struct Target {
    pub(super) socket: PathBuf,
    /// The working directory, in its symlink-resolved form.
    pub(super) cwd: PathBuf,
    /// The directory a newly started server runs in.
    pub(super) workspace: PathBuf,
    pub(super) env: BTreeMap<String, String>,
    pub(super) identity: String,
    /// The configuration this call resolved, for its warnings.
    pub(super) config: octocode_native::config::ConfigOutput,
}

/// The server for this call, or `None` when it must not be used: memory
/// storage (by choice, silent), or a location that is unsafe or too long
/// for a socket (stated on stderr, then in process).
pub(super) fn target() -> Option<Target> {
    use octocode_native::config::{
        RuntimeSurface, acquire_config_input, is_persistent_storage_enabled, resolve_config,
    };
    let cwd = same_dir_form(&std::env::current_dir().ok()?);
    let os_home = octocode_io::env::home_dir()?;
    let env = served_env(octocode_io::env::vars_lossy());
    let input = acquire_config_input(env.clone(), cwd.clone(), os_home, RuntimeSurface::Cli);
    let config = resolve_config(&input);
    if !is_persistent_storage_enabled(&config.resolved) {
        return None;
    }
    let identity = call_identity(&config, &env);
    let workspace = workspace_of(&cwd);
    let key = server_key(&exe_identity()?, &workspace, &identity);
    let socket = socket_in(&std::env::temp_dir(), &key)
        .map_err(|reason| {
            eprintln!("octocode: warm server unavailable ({reason}); running in process.");
        })
        .ok()?;
    Some(Target {
        socket,
        cwd,
        workspace,
        env,
        identity,
        config,
    })
}

/// Send one request and read the printed call back within `deadline`.
/// `first` is the first interrupt: it calls `interrupted`, closes the
/// request side (the server cancels), then waits for the server to finish
/// unless `second` arrives first.
pub(super) async fn exchange(
    stream: &mut tokio::net::UnixStream,
    request: &Request<'_>,
    deadline: std::time::Duration,
    first: impl std::future::Future<Output = ()>,
    second: impl std::future::Future,
    interrupted: impl FnOnce(),
) -> Result<Reply, Failed> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
    let fallback = |reason: &str| Failed::Fallback(reason.to_owned());
    let mut line = json!({
        "tool": request.tool,
        "input": request.input,
        "json": request.json_out,
        "cwd": request.cwd,
        "env": request.env,
        "identity": request.identity,
    })
    .to_string();
    line.push('\n');
    let (read, mut write) = stream.split();
    write
        .write_all(line.as_bytes())
        .await
        .map_err(|error| Failed::Fallback(format!("request not sent: {error}")))?;
    let mut reader = BufReader::new(read);
    let response = tokio::select! {
        response = tokio::time::timeout(
            deadline,
            read_line_bounded(&mut reader, MAX_RESPONSE_BYTES),
        ) => response.map_err(|_| Failed::Deadline)?.ok_or_else(|| {
            fallback("it closed without a reply, or the reply was over the size limit")
        })?,
        () = first => {
            interrupted();
            let _ = write.shutdown().await;
            let finished = async {
                let mut rest = (&mut reader).take(MAX_RESPONSE_BYTES);
                let _ = tokio::io::copy(&mut rest, &mut tokio::io::sink()).await;
            };
            return Err(Failed::Interrupted(
                crate::cli::drain_after_interrupt(finished, second).await,
            ));
        }
    };
    let reply =
        serde_json::from_str::<Value>(&response).map_err(|_| fallback("the reply was not JSON"))?;
    if let Some(reason) = reply.get("fallback").and_then(Value::as_str) {
        return Err(fallback(reason));
    }
    let notices = reply["notices"]
        .as_array()
        .map(|notices| {
            notices
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let printed =
        Printed::from_json(reply).ok_or_else(|| fallback("the reply was not a printed call"))?;
    Ok(Reply { printed, notices })
}

pub(super) async fn connect(socket: &Path) -> Option<tokio::net::UnixStream> {
    tokio::net::UnixStream::connect(socket).await.ok()
}

/// Wait up to two seconds for a just-started server to bind its socket.
pub(super) async fn wait_for(socket: &Path) -> Option<tokio::net::UnixStream> {
    for _ in 0..400 {
        if let Some(stream) = connect(socket).await {
            return Some(stream);
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    None
}

/// Start `octocode serve --socket <path>` in `workspace`, detached from this
/// process: its own session, no inherited stdio (a caller capturing our
/// output must not wait for the server). It inherits this process's
/// environment, as the in-process call would have run with.
pub(super) fn spawn_server(socket: &Path, workspace: &Path) -> Option<()> {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe().ok()?;
    let mut command = std::process::Command::new(exe);
    command
        .arg("serve")
        .arg("--socket")
        .arg(socket)
        .current_dir(workspace)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // SAFETY: setsid is async-signal-safe and touches only the child.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    command.spawn().ok().map(|_| ())
}
