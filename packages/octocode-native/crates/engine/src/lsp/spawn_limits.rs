//! OS-level memory containment for spawned language servers.
//!
//! Every internal buffer in the LSP path is bounded, but internal bounds do
//! not stop a runaway *server* from exhausting host memory. The cap bounds
//! memory the server tree actually uses, never address space it only reserves:
//!
//! - macOS and Linux: an RSS watchdog ([`watch_memory`]) samples the whole
//!   server tree's resident memory every [`MEMORY_WATCHDOG_INTERVAL`]; a tree
//!   over the cap is killed and its connection failed with a clear error.
//! - Windows: a Job Object with `JOB_OBJECT_LIMIT_JOB_MEMORY` (committed
//!   memory of the whole job).
//! - Other Unix targets, which have no watchdog: `setrlimit(RLIMIT_AS)` in
//!   `pre_exec`, the only bound available there.
//!
//! `RLIMIT_AS` is not used where the watchdog runs. It limits reserved virtual
//! address space, and servers routinely reserve far more than they touch: a
//! JVM (jdtls) reserves its maximum heap (a quarter of host RAM by default)
//! plus class space and code cache, V8 reserves WebAssembly memory cages, Go
//! and glibc reserve arenas. A 4 GiB address-space cap makes such servers fail
//! to start on large hosts while they use a fraction of it. On macOS it cannot
//! be lowered at all (inherited mappings make every spawn fail with `EINVAL`).

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::sync::Arc;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::time::Duration;
use tokio::process::Command;

/// How often the RSS watchdog samples a language-server tree.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) const MEMORY_WATCHDOG_INTERVAL: Duration = Duration::from_secs(2);

/// Floor of the default cap (4 GiB): large enough for heavyweight servers
/// such as rust-analyzer on sizable workspaces.
pub(crate) const MIN_DEFAULT_LSP_MEMORY_CAP_MB: u32 = 4_096;
/// Ceiling of the default cap (8 GiB): what a huge monorepo's TypeScript
/// program needs on a large host, still bounded for the machine.
pub(crate) const MAX_DEFAULT_LSP_MEMORY_CAP_MB: u32 = 8_192;

/// The default cap: a quarter of host memory, within
/// [`MIN_DEFAULT_LSP_MEMORY_CAP_MB`]..=[`MAX_DEFAULT_LSP_MEMORY_CAP_MB`]. A
/// host whose memory cannot be read keeps the floor.
pub(crate) fn default_memory_cap_mb(host_memory_mb: Option<u64>) -> u32 {
    host_memory_mb.map_or(MIN_DEFAULT_LSP_MEMORY_CAP_MB, |host| {
        u32::try_from(host / 4)
            .unwrap_or(u32::MAX)
            .clamp(MIN_DEFAULT_LSP_MEMORY_CAP_MB, MAX_DEFAULT_LSP_MEMORY_CAP_MB)
    })
}

/// Physical memory of this host in MiB, `None` where it cannot be read.
fn host_memory_mb() -> Option<u64> {
    static HOST: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *HOST.get_or_init(read_host_memory_mb)
}

#[cfg(target_os = "macos")]
fn read_host_memory_mb() -> Option<u64> {
    let mut bytes: u64 = 0;
    let mut size = std::mem::size_of::<u64>();
    // SAFETY: `hw.memsize` is a NUL-terminated name and `bytes`/`size` are a
    // valid out-buffer of `size` bytes.
    let status = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            std::ptr::from_mut(&mut bytes).cast(),
            &raw mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    (status == 0 && bytes > 0).then_some(bytes / (1024 * 1024))
}

#[cfg(target_os = "linux")]
fn read_host_memory_mb() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kib = meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    Some(kib / 1024)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn read_host_memory_mb() -> Option<u64> {
    None
}

/// The cap in MiB `max_memory_mb` resolves to: unset takes the host-derived
/// default, `Some(0)` disables the cap (`None`).
pub(crate) fn effective_memory_cap_mb(max_memory_mb: Option<u32>) -> Option<u32> {
    match max_memory_mb.unwrap_or_else(|| default_memory_cap_mb(host_memory_mb())) {
        0 => None,
        mb => Some(mb),
    }
}

/// Resolve the configured cap to bytes (see [`effective_memory_cap_mb`]).
pub(crate) fn memory_cap_bytes(max_memory_mb: Option<u32>) -> Option<u64> {
    effective_memory_cap_mb(max_memory_mb).map(|mb| u64::from(mb) * 1024 * 1024)
}

/// What all pooled servers may hold together before the pool stops idle
/// ones (`LspPoolOptions::max_resident_bytes`): half the host's memory,
/// never under one server's default cap (a single server must fit) and
/// never over `max_entries` such caps (more than the pool holds). A host
/// whose memory cannot be read keeps one cap.
pub(crate) fn default_pool_memory_budget_bytes(max_entries: usize) -> u64 {
    pool_memory_budget_bytes(host_memory_mb(), max_entries)
}

fn pool_memory_budget_bytes(host_memory_mb: Option<u64>, max_entries: usize) -> u64 {
    let per_server = u64::from(default_memory_cap_mb(host_memory_mb)) * 1024 * 1024;
    let ceiling = per_server.saturating_mul(max_entries.max(1) as u64);
    host_memory_mb.map_or(per_server, |host| {
        (host * 1024 * 1024 / 2).clamp(per_server, ceiling)
    })
}

/// Unix targets without the RSS watchdog: cap the child's address space
/// before `exec`, the only bound available there. Must be called before
/// `Command::spawn`. macOS and Linux bound resident memory with
/// [`watch_memory`] instead (see the module docs), so there this is a no-op.
#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
pub(crate) fn apply_pre_spawn_cap(command: &mut Command, cap_bytes: u64) {
    let limit = libc::rlimit {
        rlim_cur: cap_bytes as libc::rlim_t,
        rlim_max: cap_bytes as libc::rlim_t,
    };
    // SAFETY: the closure runs between fork and exec, so it must be
    // async-signal-safe: it performs no allocation and only calls setrlimit
    // on a value captured by copy before the fork.
    unsafe {
        command.pre_exec(move || {
            if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(any(not(unix), target_os = "macos", target_os = "linux"))]
pub(crate) fn apply_pre_spawn_cap(_command: &mut Command, _cap_bytes: u64) {}

/// Error text for a server killed by the memory watchdog.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn memory_cap_exceeded_message(rss_bytes: u64, cap_bytes: u64) -> String {
    format!(
        "language server exceeded memory cap ({} MiB resident > {} MiB); it was killed.",
        rss_bytes / (1024 * 1024),
        cap_bytes / (1024 * 1024)
    )
}

/// RSS watchdog for a spawned server tree (the leader `pid` plus every
/// descendant). Samples every `interval` while `alive()` holds, publishing the
/// latest sample in `resident` for the pool's aggregate budget; when the
/// tree's resident memory exceeds `cap_bytes`, calls `on_exceed(rss)` (which
/// fails the connection with a clear error) and then SIGKILLs the whole tree.
/// Returns when the leader is gone, `alive()` turns false, or after a kill.
/// The caller aborts the task before reaping the leader, so the pid being
/// sampled can never have been recycled.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) async fn watch_memory<A, E>(
    pid: u32,
    cap_bytes: u64,
    interval: Duration,
    resident: Arc<AtomicU64>,
    alive: A,
    on_exceed: E,
) where
    A: Fn() -> bool,
    E: FnOnce(u64),
{
    use crate::process;
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        if !alive() {
            return;
        }
        let sample = tokio::task::spawn_blocking(move || process::tree_rss_bytes(pid)).await;
        let Ok(Some(rss)) = sample else {
            return;
        };
        resident.store(rss, Ordering::Relaxed);
        if rss > cap_bytes {
            on_exceed(rss);
            process::kill_tree_blocking(pid).await;
            return;
        }
    }
}

/// Aborts the wrapped task when dropped (the watchdog must not outlive the
/// client state it guards).
#[derive(Debug)]
pub(crate) struct AbortOnDrop(pub(crate) tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[cfg(test)]
mod tests;
