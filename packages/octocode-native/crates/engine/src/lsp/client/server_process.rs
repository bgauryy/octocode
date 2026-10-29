//! The language server's OS process: spawn program, environment, CPU clock,
//! memory watchdog, and teardown.

use crate::error::{Error, Result};
use crate::lsp::spawn_limits;
use crate::lsp::transport::{CpuClock, JsonRpcConnection};
use std::sync::{Arc, atomic::AtomicU64};
use tokio::process::Child;
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};

/// Bound on how long `stop()` waits for the server to exit on its own after
/// `exit` before escalating to a hard kill. Keeps the common (graceful) path
/// from discarding the exit status while still bounding worst-case shutdown
/// latency — a server that never exits degrades to exactly today's
/// unconditional-kill behavior, never worse.
pub(super) const GRACEFUL_EXIT_TIMEOUT_MS: u64 = 2_000;

/// Minimal, secret-free environment variables forwarded to spawned language
/// servers after `env_clear()`. Deliberately excludes everything octocode-
/// specific or credential-bearing (OCTOCODE_*, GITHUB_TOKEN, AWS_*, …). PATH is
/// required to resolve/launch servers; HOME (and its Windows equivalents) lets
/// servers find their per-user caches/toolchains. Rust's explicit toolchain
/// homes and selection must match discovery even when HOME is isolated.
/// Any additional environment must be requested explicitly via the server config.
pub(super) const LSP_SERVER_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "RUSTUP_HOME",
    "CARGO_HOME",
    "RUSTUP_TOOLCHAIN",
    "HOME",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "SYSTEMROOT",
    "SystemRoot",
    // Toolchain selection the servers of other languages read: a Python
    // interpreter and its import paths (pyright), the Go module cache,
    // flags and workspace file (gopls), the JVM (jdtls, Metals), locale and
    // scratch space.
    "VIRTUAL_ENV",
    "CONDA_PREFIX",
    "PYTHONPATH",
    "GOPATH",
    "GOMODCACHE",
    "GOFLAGS",
    "GOWORK",
    "GOPROXY",
    "GOPRIVATE",
    "GONOSUMDB",
    "JAVA_HOME",
    "XDG_CONFIG_HOME",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "TEMP",
    "TMP",
];
// Read from the process, so declared there (`crate::process_env`).
const _: () = {
    let mut index = 0;
    while index < LSP_SERVER_ENV_ALLOWLIST.len() {
        assert!(crate::process_env::declares(
            LSP_SERVER_ENV_ALLOWLIST[index]
        ));
        index += 1;
    }
};

/// Give a spawned child process a bounded window to exit on its own (e.g.
/// after an LSP `exit` notification) before escalating to a hard kill.
/// Returns `true` if the process exited within `timeout_duration` without
/// needing to be killed.
pub(super) async fn wait_for_graceful_exit(child: &mut Child, timeout_duration: Duration) -> bool {
    end_child(child, timeout_duration, crate::process::kill_tree_blocking).await
}

/// Ends `child` and sweeps its process group with `sweep(pid)` on both paths:
/// descendants (proc-macro-srv, cargo/build scripts, clangd workers) must die
/// on a clean exit too, not only on timeout. The sweep signals `-pid`, so it
/// fires while the leader is still unreaped (alive, or a zombie that keeps its
/// pid reserved); after the reap the pid could be recycled by an unrelated
/// process leading its own group. The leader is reaped last, and hard-killed
/// only when it outlived the window.
pub(super) async fn end_child<F: std::future::Future<Output = ()>>(
    child: &mut Child,
    timeout_duration: Duration,
    sweep: impl FnOnce(u32) -> F,
) -> bool {
    let exited = exited_unreaped(child, timeout_duration).await;
    if let Some(pid) = child.id() {
        sweep(pid).await;
    }
    if exited {
        let _ = child.wait().await;
    } else {
        let _ = child.kill().await;
    }
    exited
}

/// Waits up to `limit` for the leader to exit without reaping it, so its pid
/// stays reserved for the group sweep. A blocking `waitid(WNOWAIT)` on the
/// blocking pool returns when the leader exits; on timeout the caller kills
/// the leader, which releases that wait. `true` when it exited in time (or
/// was already reaped).
#[cfg(unix)]
async fn exited_unreaped(child: &mut Child, limit: Duration) -> bool {
    let Some(pid) = child.id() else {
        return true;
    };
    let exit =
        tokio::task::spawn_blocking(move || crate::process::wait_for_exit_without_reaping(pid));
    timeout(limit, exit).await.is_ok()
}

#[cfg(not(unix))]
async fn exited_unreaped(child: &mut Child, limit: Duration) -> bool {
    // No pid-addressed group sweep off Unix (the Job Object tears the tree
    // down), so reaping here cannot misdirect a signal.
    timeout(limit, child.wait()).await.is_ok()
}

pub(super) async fn lsp_spawn_program(
    validated_command: &str,
    args: &mut Vec<String>,
) -> Result<String> {
    if executable_has_node_shebang(validated_command).await? {
        let node = tokio::task::spawn_blocking(crate::lsp::config::current_node_command)
            .await
            .map_err(|err| Error::new(format!("Failed to resolve Node executable: {err}")))?
            .ok_or_else(|| {
                Error::new("Node executable is required to start this language server")
            })?;
        args.insert(0, validated_command.to_owned());
        return Ok(node);
    }
    Ok(validated_command.to_owned())
}

/// Async so the one-file inspection cannot stall an executor thread inside
/// `start()` (the only caller) on slow filesystems.
async fn executable_has_node_shebang(path: &str) -> Result<bool> {
    let mut file = tokio::fs::File::open(path).await.map_err(|err| {
        Error::new(format!(
            "Failed to inspect language server executable {path}: {err}"
        ))
    })?;
    let mut buf = [0_u8; 128];
    let read = tokio::io::AsyncReadExt::read(&mut file, &mut buf)
        .await
        .map_err(|err| {
            Error::new(format!(
                "Failed to inspect language server executable {path}: {err}"
            ))
        })?;
    let first_line = std::str::from_utf8(&buf[..read])
        .ok()
        .and_then(|text| text.lines().next())
        .unwrap_or_default();
    Ok(first_line.starts_with("#!") && first_line.contains("node"))
}

/// The CPU clock of a freshly spawned server tree, which lets a request
/// outwait a server that is starved but progressing (see
/// `transport::patience`). `None` on platforms with no CPU reader, or
/// when the child already exited.
pub(super) fn cpu_clock_for(child: &Child) -> Option<CpuClock> {
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    {
        let clock = crate::process::TreeCpuClock::new(child.id()?);
        Some(Arc::new(move || clock.read()))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = child;
        None
    }
}

/// Arm the RSS watchdog for a freshly spawned server on macOS and Linux (see
/// [`spawn_limits`]). Over the cap, the connection fails with a "language
/// server exceeded memory cap" error and the server tree is killed. `None`
/// when no cap is configured, on other platforms (the pre-spawn `RLIMIT_AS` /
/// Job Object caps apply there), or when the child already exited.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(super) fn memory_watchdog_for(
    child: &Child,
    memory_cap: Option<u64>,
    connection: &Arc<JsonRpcConnection>,
    resident: Arc<AtomicU64>,
) -> Option<spawn_limits::AbortOnDrop> {
    let (cap_bytes, pid) = memory_cap.zip(child.id())?;
    let alive = Arc::downgrade(connection);
    let failed = Arc::downgrade(connection);
    Some(spawn_limits::AbortOnDrop(tokio::spawn(
        spawn_limits::watch_memory(
            pid,
            cap_bytes,
            spawn_limits::MEMORY_WATCHDOG_INTERVAL,
            resident,
            move || {
                alive
                    .upgrade()
                    .is_some_and(|connection| connection.is_alive())
            },
            move |rss| {
                if let Some(connection) = failed.upgrade() {
                    connection.fail_resource_exhausted(&spawn_limits::memory_cap_exceeded_message(
                        rss, cap_bytes,
                    ));
                }
            },
        ),
    )))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn memory_watchdog_for(
    _child: &Child,
    _memory_cap: Option<u64>,
    _connection: &Arc<JsonRpcConnection>,
    _resident: Arc<AtomicU64>,
) -> Option<spawn_limits::AbortOnDrop> {
    None
}

pub(super) async fn cleanup_failed_start(child: &mut Child, stderr_task: Option<JoinHandle<()>>) {
    if let Some(pid) = child.id() {
        crate::process::kill_tree_blocking(pid).await;
    }
    let _ = child.kill().await;
    if let Some(task) = stderr_task {
        task.abort();
    }
}
