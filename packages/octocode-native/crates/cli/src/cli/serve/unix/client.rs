//! The client side of a warm call: resolve the server for this call, start
//! it when none answers, send the request and print the reply.

use super::daemon::{remove_if_ours, socket_identity};
use super::frames::{ALIVE, MCP, MCP_ERROR, PREWARM, PROGRESS};
use super::identity::{
    call_identity, exe_identity, process_identity, served_env, server_key, workspace_of,
};
use super::sockets::socket_in;
use super::startup::{Unstarted, start};
use super::surface::Surface;
use super::{read_line_bounded, same_dir_form};
use crate::cli::output::Printed;
use octocode_contract_types::ErrorCode;
use octocode_io::http::READ_STALL;
use octocode_native::runtime::RuntimeError;
use octocode_native::runtime::error::McpResult;
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Largest reply the client accepts from a server; a longer one falls back to
/// running the tool in this process, like any other failed exchange.
pub(super) const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;

/// What a client waits past the server's request budget, so a server that
/// ran out of time has said so first.
const REPLY_MARGIN: std::time::Duration = std::time::Duration::from_secs(15);

/// How long a client waits for the reply: the budget of the server's
/// runtimes, which take the default request limits. Past it the call is
/// over; running it again in process would double the wait. Without a
/// budget, liveness alone bounds the wait: a server sends a frame at least
/// each [`HEARTBEAT`](super::frames::HEARTBEAT) while a call runs, and one
/// that sends nothing for [`READ_STALL`] is gone ([`exchange`]).
pub(super) fn reply_deadline() -> Option<std::time::Duration> {
    reply_deadline_for(octocode_native::execution::RequestLimits::default().timeout)
}

/// The reply wait for a server whose request budget is `budget`: past it by
/// [`REPLY_MARGIN`], or none when the server has none (the reply, the
/// server closing, or Ctrl-C ends the wait).
pub(super) fn reply_deadline_for(
    budget: Option<std::time::Duration>,
) -> Option<std::time::Duration> {
    budget.map(|budget| budget.saturating_add(REPLY_MARGIN))
}

/// Why an exchange with a server produced no printed call.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Failed {
    /// The server closed, refused, or sent no reply line: the call has not
    /// run here, so it runs in process.
    Fallback(String),
    /// The server accepted the request and then sent nothing, not even a
    /// heartbeat, for this long: it is stopped or hung. The call runs in
    /// process, and the server is left behind ([`discard_stalled`]).
    Stalled(std::time::Duration),
    /// No reply within the deadline.
    Deadline,
    /// Ctrl-C: the server was told to cancel, then drained or abandoned.
    Interrupted(crate::cli::tool_call::Interrupted),
}

/// Run `tool` through the workspace's server when one is reachable or can
/// be started, and print it: the exit code. `None` means the caller runs it
/// in process, with `loaded` when no server was tried, so it reads the config
/// files once.
pub(in crate::cli) async fn call(
    tool: &str,
    input: &Value,
    mode: crate::cli::OutputMode,
    loaded: &mut Option<octocode_native::config::ConfigInput>,
) -> Option<u8> {
    let json_out = mode.json;
    let remote = Remote::resolve(Surface::Cli, loaded)?;
    let target = &remote.target;
    let cancel = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    let mut stream = match remote.connect_or_start(cancel).await {
        Ok(stream) => stream,
        Err(Unstarted::Cancelled) => return Some(130),
        Err(Unstarted::Unavailable(reason)) => {
            eprintln!(
                "# octocode: warm server did not start ({reason}); running {tool} in process."
            );
            return None;
        }
    };
    let bound = socket_identity(&target.socket);
    let request = Request {
        tool,
        input,
        json_out,
        cwd: &target.cwd,
        env: &target.env,
        identity: &target.identity,
        process: &target.process,
        surface: Surface::Cli,
    };
    // An in-process call prints its configuration warnings at start.
    let warn = || octocode_native::runtime::ToolRuntime::print_startup_diagnostics(&target.config);
    let interrupted = || {
        warn();
        eprintln!(
            "{}",
            octocode_io::notice::as_note(crate::cli::tool_call::CANCELLING)
        );
    };
    let first = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    let deadline = reply_deadline();
    let exchanged = exchange(
        &mut stream,
        &request,
        deadline,
        READ_STALL,
        first,
        tokio::signal::ctrl_c(),
        interrupted,
        {
            let print = crate::cli::tool_call::progress_printer(
                crate::cli::tool_call::stderr_is_terminal(),
            );
            move |line: &str| {
                if let Some(print) = &print {
                    print(line);
                }
            }
        },
    )
    .await;
    let (printed, notices) = match exchanged {
        Ok(reply) => (reply.printed, reply.notices),
        Err(Failed::Fallback(reason)) => {
            eprintln!("# octocode: warm server failed ({reason}); running {tool} in process.");
            return None;
        }
        Err(Failed::Stalled(silence)) => {
            discard_stalled(&target.socket, bound);
            eprintln!(
                "# octocode: warm server failed ({}); running {tool} in process.",
                stalled_reason(silence)
            );
            return None;
        }
        Err(Failed::Interrupted(_)) => return Some(130),
        Err(Failed::Deadline) => (
            Printed::finish(
                tool,
                Err(octocode_native::runtime::RuntimeError {
                    code: ErrorCode::Timeout,
                    message: format!(
                        "The warm server sent no {tool} reply within {} s; the call was not run again in process.",
                        deadline.unwrap_or_default().as_secs()
                    ),
                    payload: None,
                    validation_issues: None,
                }),
                json_out,
                &serde_json::Value::Null,
                None,
            ),
            Vec::new(),
        ),
    };
    warn();
    // What the call said on stderr while it ran, as in process.
    for notice in notices {
        eprintln!("{}", octocode_io::notice::as_note(&notice));
    }
    Some(printed.print(mode))
}

/// Leave a server that went silent behind: its socket is unlinked, so the
/// next call finds nothing there and starts a fresh server instead of
/// waiting out the same silence again. The process is not touched (it may
/// be stopped, not dead, and is not this client's to kill); a successor that
/// already rebound the path keeps it ([`remove_if_ours`]). `bound` is the
/// socket's identity taken when this client connected.
pub(super) fn discard_stalled(socket: &Path, bound: Option<(u64, u64)>) {
    remove_if_ours(socket, bound);
}

/// The reason a stalled server's call runs in process.
fn stalled_reason(silence: std::time::Duration) -> String {
    format!(
        "it sent nothing for {} s; its socket was dropped, so the next call starts a new server",
        silence.as_secs()
    )
}

/// A resolved way to reach the warm server of one process: where it is,
/// and the identity every call and request of this process sends. The CLI
/// makes one for its one call; an MCP server keeps one for its session, so
/// its calls and its prewarm share one server and one configuration.
pub(in crate::cli) struct Remote {
    target: Target,
    /// Starts of the server are one at a time: the calls that wait find it
    /// started instead of each launching another.
    starting: tokio::sync::Mutex<()>,
    /// A start failed: this process runs in process from now on, so no call
    /// launches a server that cannot serve again.
    unserved: std::sync::atomic::AtomicBool,
}

/// What a server made of an MCP call.
pub(in crate::cli) enum Warm {
    /// The call's MCP result, as the in-process server answers it.
    Answered(McpResult),
    /// The call is a protocol error (cancelled, closing), as in process.
    Ended(RuntimeError),
    /// No server answered: the caller runs the call in process, and states
    /// the reason when there is one.
    InProcess(Option<String>),
}

impl Remote {
    /// The server reachable from this process for `surface`, or `None`
    /// when it must not be used ([`target`]).
    pub(in crate::cli) fn resolve(
        surface: Surface,
        loaded: &mut Option<octocode_native::config::ConfigInput>,
    ) -> Option<Self> {
        Some(Self {
            target: target(surface, loaded)?,
            starting: tokio::sync::Mutex::new(()),
            unserved: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// A connection to the server, started when none answers; `cancel`
    /// stops the wait for a start.
    async fn connect_or_start(
        &self,
        cancel: impl std::future::Future<Output = ()>,
    ) -> Result<tokio::net::UnixStream, Unstarted> {
        if let Some(stream) = connect(&self.target.socket).await {
            return Ok(stream);
        }
        let _starting = self.starting.lock().await;
        if let Some(stream) = connect(&self.target.socket).await {
            return Ok(stream);
        }
        start(&self.target.socket, &self.target.workspace, cancel).await
    }

    /// Run the MCP call `tool` on the server. `cancelled` cancels it there;
    /// the progress it raises goes to the listener of the notice scope the
    /// caller runs in ([`octocode_io::notice::Scope::progress`]).
    pub(in crate::cli) async fn call_mcp(
        &self,
        tool: &str,
        input: &Value,
        cancelled: &tokio_util::sync::CancellationToken,
    ) -> Warm {
        use std::sync::atomic::Ordering::Relaxed;
        if self.unserved.load(Relaxed) {
            return Warm::InProcess(None);
        }
        let mut stream = match self.connect_or_start(std::future::pending::<()>()).await {
            Ok(stream) => stream,
            Err(Unstarted::Unavailable(reason)) => {
                self.unserved.store(true, Relaxed);
                return Warm::InProcess(Some(format!(
                    "warm server did not start ({reason}); running {tool} in process"
                )));
            }
            Err(Unstarted::Cancelled) => return Warm::InProcess(None),
        };
        let target = &self.target;
        let bound = socket_identity(&target.socket);
        let request = Request {
            tool,
            input,
            json_out: true,
            cwd: &target.cwd,
            env: &target.env,
            identity: &target.identity,
            process: &target.process,
            surface: Surface::Mcp,
        };
        let listener = octocode_io::notice::Scope::current().and_then(|scope| scope.progress());
        let exchanged = exchange_value(
            &mut stream,
            &request.line(),
            reply_deadline(),
            READ_STALL,
            cancelled.cancelled(),
            std::future::pending::<()>(),
            || {},
            move |line: &str| {
                if let Some(listener) = &listener {
                    listener(line);
                }
            },
        )
        .await;
        match exchanged {
            Ok(reply) => mcp_answer(&reply).unwrap_or_else(|| {
                Warm::InProcess(Some(format!(
                    "warm server sent no MCP result for {tool}; running it in process"
                )))
            }),
            Err(Failed::Interrupted(_)) => Warm::Ended(RuntimeError {
                code: ErrorCode::Cancelled,
                message: "Request cancelled".to_owned(),
                payload: None,
                validation_issues: None,
            }),
            Err(Failed::Fallback(reason)) => Warm::InProcess(Some(format!(
                "warm server failed ({reason}); running {tool} in process"
            ))),
            Err(Failed::Stalled(silence)) => {
                discard_stalled(&target.socket, bound);
                Warm::InProcess(Some(format!(
                    "warm server failed ({}); running {tool} in process",
                    stalled_reason(silence)
                )))
            }
            // No budget is set, so no deadline passes; the reply wait is
            // bounded by the server's liveness alone.
            Err(Failed::Deadline) => Warm::InProcess(Some(format!(
                "warm server sent no {tool} reply in time; running it in process"
            ))),
        }
    }

    /// Ask the server to warm this session's workspace (a language-server
    /// start per top language, in the shared pool). `Err` (the reason): no
    /// server did it, so the caller warms its own.
    pub(in crate::cli) async fn prewarm(&self) -> Result<(), String> {
        use std::sync::atomic::Ordering::Relaxed;
        if self.unserved.load(Relaxed) {
            return Err("no warm server".into());
        }
        let mut stream = match self.connect_or_start(std::future::pending::<()>()).await {
            Ok(stream) => stream,
            Err(Unstarted::Unavailable(reason)) => {
                self.unserved.store(true, Relaxed);
                return Err(format!("warm server did not start ({reason})"));
            }
            Err(Unstarted::Cancelled) => return Err("cancelled".into()),
        };
        let target = &self.target;
        let bound = socket_identity(&target.socket);
        let line = request_line(&json!({
            PREWARM: true,
            "surface": Surface::Mcp.as_str(),
            "cwd": target.cwd,
            "env": target.env,
            "identity": target.identity,
            "process": target.process,
        }));
        exchange_value(
            &mut stream,
            &line,
            reply_deadline(),
            READ_STALL,
            std::future::pending::<()>(),
            std::future::pending::<()>(),
            || {},
            |_| {},
        )
        .await
        .map(|_| ())
        .map_err(|failed| {
            if let Failed::Stalled(silence) = failed {
                discard_stalled(&target.socket, bound);
                return stalled_reason(silence);
            }
            format!("{failed:?}")
        })
    }
}

/// An MCP reply line as the call's answer; `None` when it is neither a
/// result nor a protocol error.
fn mcp_answer(reply: &Value) -> Option<Warm> {
    if let Some(result) = reply.get(MCP) {
        return Some(Warm::Answered(McpResult {
            text: result.get("text")?.as_str()?.to_owned(),
            is_error: result.get("isError")?.as_bool()?,
        }));
    }
    let error = reply.get(MCP_ERROR)?;
    Some(Warm::Ended(RuntimeError {
        code: serde_json::from_value(error.get("code")?.clone()).ok()?,
        message: error.get("message")?.as_str()?.to_owned(),
        payload: None,
        validation_issues: None,
    }))
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
    /// The client's [`process_identity`]: the server answers only when its
    /// own process environment is the same.
    pub(super) process: &'a str,
    /// The surface whose runtime answers, and in whose reply form.
    pub(super) surface: Surface,
}

impl Request<'_> {
    /// The request as its one line.
    pub(super) fn line(&self) -> String {
        request_line(&json!({
            "tool": self.tool,
            "input": self.input,
            "json": self.json_out,
            "cwd": self.cwd,
            "env": self.env,
            "identity": self.identity,
            "process": self.process,
            "surface": self.surface.as_str(),
        }))
    }
}

/// `request` as the one line a server reads.
fn request_line(request: &Value) -> String {
    let mut line = request.to_string();
    line.push('\n');
    line
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
    pub(super) process: String,
    /// The configuration this call resolved, for its warnings.
    pub(super) config: octocode_native::config::ConfigOutput,
}

/// The server for this call, or `None` when it must not be used: memory
/// storage (by choice, silent), or a location that is unsafe or too long
/// for a socket (stated on stderr, then in process). `loaded` then holds the
/// config files read for it, unless the working directory was reached through
/// a symbolic link: the in-process runtime keys files by the directory as given.
pub(super) fn target(
    surface: Surface,
    loaded: &mut Option<octocode_native::config::ConfigInput>,
) -> Option<Target> {
    use octocode_native::config::{
        acquire_config_input, is_persistent_storage_enabled, resolve_config,
    };
    let given = std::env::current_dir().ok()?;
    let cwd = same_dir_form(&given);
    let os_home = std::env::home_dir()?;
    let env = served_env(octocode_io::env::vars_lossy());
    let input = acquire_config_input(env.clone(), cwd.clone(), os_home, surface.runtime());
    let config = resolve_config(&input);
    if !is_persistent_storage_enabled(&config.resolved) {
        *loaded = (given == cwd).then_some(input);
        return None;
    }
    let identity = call_identity(&config);
    let process = process_identity(&env);
    let workspace = workspace_of(&cwd);
    let key = server_key(&exe_identity()?, &workspace, &process);
    let socket = match socket_in(&std::env::temp_dir(), &key) {
        Ok(socket) => socket,
        Err(reason) => {
            eprintln!("octocode: warm server unavailable ({reason}); running in process.");
            *loaded = (given == cwd).then_some(input);
            return None;
        }
    };
    Some(Target {
        socket,
        cwd,
        workspace,
        env,
        identity,
        process,
        config,
    })
}

/// `awaited`, or [`Failed::Deadline`] once `deadline` passes; with none,
/// `awaited` alone decides.
async fn within<T>(
    deadline: Option<std::time::Duration>,
    awaited: impl std::future::Future<Output = Result<T, Failed>>,
) -> Result<T, Failed> {
    match deadline {
        Some(deadline) => tokio::time::timeout(deadline, awaited)
            .await
            .map_err(|_| Failed::Deadline)?,
        None => awaited.await,
    }
}

/// Send one request and read the printed call back: [`exchange_value`]
/// decoded as a CLI reply.
pub(super) async fn exchange(
    stream: &mut tokio::net::UnixStream,
    request: &Request<'_>,
    deadline: Option<std::time::Duration>,
    silence: std::time::Duration,
    first: impl std::future::Future<Output = ()>,
    second: impl std::future::Future,
    interrupted: impl FnOnce(),
    progress: impl FnMut(&str),
) -> Result<Reply, Failed> {
    let reply = exchange_value(
        stream,
        &request.line(),
        deadline,
        silence,
        first,
        second,
        interrupted,
        progress,
    )
    .await?;
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
    let printed = Printed::from_json(reply)
        .ok_or_else(|| Failed::Fallback("the reply was not a printed call".to_owned()))?;
    Ok(Reply { printed, notices })
}

/// Send the request `line` and read the reply line back within `deadline`,
/// or with none for as long as the server keeps the connection and says
/// something at least each `silence`: progress and heartbeat frames count,
/// so a long call is waited for and a server that went silent (stopped,
/// hung) is a fallback. `first` is the first interrupt (a Ctrl-C, an MCP
/// cancel): it calls `interrupted`, closes the request side (the server
/// cancels), then waits for the server to finish unless `second` arrives
/// first. Each progress frame the server sends while the call runs goes to
/// `progress` as it arrives. A `fallback` reply is [`Failed::Fallback`].
pub(super) async fn exchange_value(
    stream: &mut tokio::net::UnixStream,
    line: &str,
    deadline: Option<std::time::Duration>,
    silence: std::time::Duration,
    first: impl std::future::Future<Output = ()>,
    second: impl std::future::Future,
    interrupted: impl FnOnce(),
    mut progress: impl FnMut(&str),
) -> Result<Value, Failed> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
    let fallback = |reason: &str| Failed::Fallback(reason.to_owned());
    let (read, mut write) = stream.split();
    write
        .write_all(line.as_bytes())
        .await
        .map_err(|error| Failed::Fallback(format!("request not sent: {error}")))?;
    let mut reader = BufReader::new(read);
    // Frames come first, one per line; the line that is not one is the
    // reply. The deadline covers the whole wait, frames included; the
    // silence, each line.
    let awaited = async {
        loop {
            let next = read_line_bounded(&mut reader, MAX_RESPONSE_BYTES);
            let line = tokio::time::timeout_at(tokio::time::Instant::now() + silence, next)
                .await
                .map_err(|_| Failed::Stalled(silence))?
                .ok_or_else(|| {
                    fallback("it closed without a reply, or the reply was over the size limit")
                })?;
            let parsed = serde_json::from_str::<Value>(&line)
                .map_err(|_| fallback("the reply was not JSON"))?;
            if let Some(text) = parsed.get(PROGRESS).and_then(Value::as_str) {
                progress(text);
            } else if parsed.get(ALIVE).is_none() {
                break Ok::<Value, Failed>(parsed);
            }
        }
    };
    let reply = tokio::select! {
        awaited = within(deadline, awaited) => awaited?,
        () = first => {
            interrupted();
            let _ = write.shutdown().await;
            let finished = async {
                let mut rest = (&mut reader).take(MAX_RESPONSE_BYTES);
                let _ = tokio::io::copy(&mut rest, &mut tokio::io::sink()).await;
            };
            return Err(Failed::Interrupted(
                crate::cli::tool_call::drain_after_interrupt(finished, second).await,
            ));
        }
    };
    if let Some(reason) = reply.get("fallback").and_then(Value::as_str) {
        return Err(fallback(reason));
    }
    Ok(reply)
}

pub(super) async fn connect(socket: &Path) -> Option<tokio::net::UnixStream> {
    tokio::net::UnixStream::connect(socket).await.ok()
}
