//! Starting a server. The client launches `octocode serve` with a private
//! startup channel, the child's stdout, and keeps the child's handle and
//! stderr while it starts. The server announces one [`Startup`] outcome on
//! that channel before it serves, then points its stdio at `/dev/null`
//! ([`announce`]). The wait ends on that outcome, on the
//! channel closing without one (the child failed; its stderr says why), or
//! on Ctrl-C. No interval decides: a start that is slow but alive is waited
//! for, and a child that failed ends the wait at once.
//!
//! Cancelling closes the channel. A server that then cannot announce that
//! it is ready exits before it takes a call ([`announce`]); one that did
//! announce serves as any other.

use super::client::MAX_RESPONSE_BYTES;
use super::read_line_bounded;
use serde_json::{Value, json};
use std::path::Path;

/// The key of the startup outcome line.
const STARTUP: &str = "startup";

/// What a starting server announces on its startup channel, once, before
/// it serves.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Startup {
    /// It owns the socket, restricted to its user, and accepts calls.
    Ready,
    /// Another server answers on the socket: the client calls that one,
    /// which checks the call's identity as always.
    Existing,
    /// It cannot serve, and why.
    Unavailable(String),
}

impl Startup {
    /// The outcome as one channel line.
    pub(super) fn line(&self) -> String {
        let value = match self {
            Self::Ready => json!({ STARTUP: "ready" }),
            Self::Existing => json!({ STARTUP: "existing" }),
            Self::Unavailable(reason) => json!({ STARTUP: "unavailable", "reason": reason }),
        };
        format!("{value}\n")
    }

    /// The outcome a channel line announces, or `None` when it is not one.
    pub(super) fn parse(line: &str) -> Option<Self> {
        let value = serde_json::from_str::<Value>(line).ok()?;
        match value.get(STARTUP)?.as_str()? {
            "ready" => Some(Self::Ready),
            "existing" => Some(Self::Existing),
            "unavailable" => Some(Self::Unavailable(value.get("reason")?.as_str()?.to_owned())),
            _ => None,
        }
    }
}

/// Server side: announce `outcome` on the startup channel (stdout), and
/// once ready release the startup pipes ([`release_startup_stdio`]).
/// `false` when nobody took it: the client was cancelled or is gone, so a
/// server that could not say it is ready must not serve.
pub(super) fn announce(outcome: &Startup) -> bool {
    use std::io::Write as _;
    let mut out = std::io::stdout().lock();
    let announced = out
        .write_all(outcome.line().as_bytes())
        .and_then(|()| out.flush())
        .is_ok();
    drop(out);
    announced && (*outcome != Startup::Ready || release_startup_stdio().is_ok())
}

/// Server side, once ready: point stdout and stderr at `/dev/null`, so the
/// client's startup pipes close and nothing the server writes later fails
/// or blocks on them.
fn release_startup_stdio() -> std::io::Result<()> {
    use std::os::fd::AsRawFd as _;
    let null = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/null")?;
    for fd in [libc::STDOUT_FILENO, libc::STDERR_FILENO] {
        // SAFETY: both descriptors are open; dup2 replaces `fd` atomically.
        if unsafe { libc::dup2(null.as_raw_fd(), fd) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Why no server took a call.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Unstarted {
    /// No server can serve it, and why: the call runs in process.
    Unavailable(String),
    /// Ctrl-C while the server started.
    Cancelled,
}

/// Start the server for `socket` in `workspace` and connect to it, or to
/// the server that already answers there; `cancel` stops the wait.
pub(super) async fn start(
    socket: &Path,
    workspace: &Path,
    cancel: impl std::future::Future<Output = ()>,
) -> Result<tokio::net::UnixStream, Unstarted> {
    let mut child = spawn_server(socket, workspace)
        .map_err(|error| Unstarted::Unavailable(format!("it did not launch: {error}")))?;
    // The child is not waited for once it answered: a ready server lives on
    // in its own session, and one that ended is reaped in the background.
    match await_startup(&mut child, cancel).await {
        None => Err(Unstarted::Cancelled),
        Some(Startup::Unavailable(reason)) => Err(Unstarted::Unavailable(reason)),
        Some(Startup::Ready | Startup::Existing) => {
            super::client::connect(socket).await.ok_or_else(|| {
                Unstarted::Unavailable("its socket refused the connection once started".into())
            })
        }
    }
}

/// Launch `octocode serve --socket <path>` in `workspace`, in its own
/// session. Its stdout is the startup channel and its stderr says why it
/// failed; both are this process's pipes, never the caller's, so a caller
/// capturing our output does not wait for the server. It inherits this
/// process's environment, as the in-process call would have run with.
pub(super) fn spawn_server(
    socket: &Path,
    workspace: &Path,
) -> std::io::Result<tokio::process::Child> {
    use std::os::unix::process::CommandExt;
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .arg("serve")
        .arg("--socket")
        .arg(socket)
        .current_dir(workspace)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    // SAFETY: setsid is async-signal-safe and touches only the child.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    tokio::process::Command::from(command).spawn()
}

/// The outcome `child` announces on its startup channel; when the channel
/// closes without one, [`Startup::Unavailable`] with the child's exit
/// status and all it said on stderr. `None` when `cancel` came first: the
/// caller drops the child's pipes, which closes the channel.
pub(super) async fn await_startup(
    child: &mut tokio::process::Child,
    cancel: impl std::future::Future<Output = ()>,
) -> Option<Startup> {
    use tokio::io::AsyncReadExt as _;
    let Some(stdout) = child.stdout.take() else {
        return Some(Startup::Unavailable("it has no startup channel".into()));
    };
    let mut stderr = child.stderr.take();
    let mut channel = tokio::io::BufReader::new(stdout);
    let mut said = Vec::new();
    let mut chunk = [0u8; 8 * 1024];
    tokio::pin!(cancel);
    let announced = read_line_bounded(&mut channel, MAX_RESPONSE_BYTES);
    tokio::pin!(announced);
    // Stderr is read while the channel is: a child that writes much there
    // must not block on a full pipe before it announces.
    let line = loop {
        tokio::select! {
            line = &mut announced => break line,
            read = async {
                match stderr.as_mut() {
                    Some(stderr) => stderr.read(&mut chunk).await.unwrap_or(0),
                    None => 0,
                }
            }, if stderr.is_some() => match read {
                0 => stderr = None,
                read => said.extend_from_slice(&chunk[..read]),
            },
            () = &mut cancel => return None,
        }
    };
    if let Some(line) = line {
        return Some(Startup::parse(&line).unwrap_or_else(|| {
            Startup::Unavailable(format!(
                "it announced {:?} instead of its startup outcome",
                line.trim_end()
            ))
        }));
    }
    // The channel closed without an outcome: the child is ending. All it
    // said on stderr, and how it ended, is the reason.
    let ended = async {
        if let Some(mut stderr) = stderr {
            let _ = stderr.read_to_end(&mut said).await;
        }
        child.wait().await
    };
    let status = tokio::select! {
        status = ended => status,
        () = &mut cancel => return None,
    };
    Some(Startup::Unavailable(ended_reason(status, &said)))
}

/// How a child that closed its startup channel ended, and what it said on
/// stderr, on one line.
fn ended_reason(status: std::io::Result<std::process::ExitStatus>, said: &[u8]) -> String {
    let ended = match status {
        Ok(status) => format!("it exited ({status}) before it was ready"),
        Err(_) => "it closed its startup channel before it was ready".to_owned(),
    };
    let said = String::from_utf8_lossy(said);
    let said: Vec<&str> = said
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if said.is_empty() {
        ended
    } else {
        format!("{ended}: {}", said.join(" "))
    }
}
