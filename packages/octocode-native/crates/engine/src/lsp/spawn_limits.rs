//! OS-level memory containment for spawned language servers.
//!
//! Every internal buffer in the LSP path is bounded, but internal bounds do
//! not stop a runaway *server* from exhausting host memory. The cap is applied
//! at spawn: `setrlimit(RLIMIT_AS)` in `pre_exec` on supported Unix targets,
//! and a Job Object with `JOB_OBJECT_LIMIT_JOB_MEMORY` on Windows. macOS
//! cannot use `RLIMIT_AS` (lowering it below the process's inherited virtual
//! mappings makes every child spawn fail with `EINVAL`), so there the cap is
//! enforced by an RSS watchdog ([`watch_memory`]): the server tree's resident
//! memory is sampled every [`MEMORY_WATCHDOG_INTERVAL`] and a tree over the
//! cap is killed and its connection failed with a clear error.

use crate::error::Result;
#[cfg(windows)]
use crate::error::{Error, Status};
use std::time::Duration;
use tokio::process::{Child, Command};

/// How often the RSS watchdog samples a language-server tree.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Wired on macOS only.
pub(crate) const MEMORY_WATCHDOG_INTERVAL: Duration = Duration::from_secs(2);

/// Generous default (4 GiB): large enough for heavyweight servers such as
/// rust-analyzer on sizable workspaces, small enough to protect the host.
pub(crate) const DEFAULT_LSP_MEMORY_CAP_MB: u32 = 4_096;

/// Resolve the configured cap to bytes. `None` → default, `Some(0)` → no cap.
pub(crate) fn memory_cap_bytes(max_memory_mb: Option<u32>) -> Option<u64> {
    match max_memory_mb.unwrap_or(DEFAULT_LSP_MEMORY_CAP_MB) {
        0 => None,
        mb => Some(u64::from(mb) * 1024 * 1024),
    }
}

/// Supported Unix targets: cap the child's address space before `exec`. Must
/// be called before `Command::spawn`.
///
/// macOS processes inherit virtual mappings (including the shared cache) that
/// routinely exceed the configured cap before `exec`; lowering `RLIMIT_AS` in
/// the forked child therefore returns `EINVAL` and prevents every server from
/// starting. Darwin has no equivalent enforceable per-child address-space cap,
/// so its implementation below is deliberately a no-op.
#[cfg(all(unix, not(target_os = "macos")))]
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

#[cfg(any(not(unix), target_os = "macos"))]
pub(crate) fn apply_pre_spawn_cap(_command: &mut Command, _cap_bytes: u64) {}

/// Error text for a server killed by the memory watchdog.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Wired on macOS only.
pub(crate) fn memory_cap_exceeded_message(rss_bytes: u64, cap_bytes: u64) -> String {
    format!(
        "language server exceeded memory cap ({} MiB resident > {} MiB maxMemoryMb); it was killed. Raise maxMemoryMb (0 disables the cap) for very large workspaces.",
        rss_bytes / (1024 * 1024),
        cap_bytes / (1024 * 1024)
    )
}

/// RSS watchdog for a spawned server tree (the leader `pid` plus every
/// descendant). Samples every `interval` while `alive()` holds; when the
/// tree's resident memory exceeds `cap_bytes`, calls `on_exceed(rss)` (which
/// fails the connection with a clear error) and then SIGKILLs the whole tree.
/// Returns when the leader is gone, `alive()` turns false, or after a kill.
/// The caller aborts the task before reaping the leader, so the pid being
/// sampled can never have been recycled.
#[cfg(unix)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Wired on macOS only.
pub(crate) async fn watch_memory<A, E>(
    pid: u32,
    cap_bytes: u64,
    interval: Duration,
    alive: A,
    on_exceed: E,
) where
    A: Fn() -> bool,
    E: FnOnce(u64),
{
    use crate::lsp::process_tree;
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        if !alive() {
            return;
        }
        let sample = tokio::task::spawn_blocking(move || process_tree::tree_rss_bytes(pid)).await;
        let Ok(Some(rss)) = sample else {
            return;
        };
        if rss > cap_bytes {
            on_exceed(rss);
            let _ = tokio::task::spawn_blocking(move || process_tree::kill_tree(pid)).await;
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

/// Windows: post-spawn Job Object that always enforces
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` (whole-tree teardown) and additionally
/// holds the memory limit when one is configured. The handle must outlive the
/// child, so dropping the guard after the child is reaped is the correct order.
/// On Unix the guard is a unit type (tree teardown is via the process group).
#[cfg(not(windows))]
#[derive(Debug)]
pub(crate) struct MemoryCapGuard;

#[cfg(not(windows))]
impl MemoryCapGuard {
    pub(crate) fn attach(_child: &Child, _cap_bytes: Option<u64>) -> Result<Self> {
        Ok(Self)
    }
}

#[cfg(windows)]
#[derive(Debug)]
pub(crate) struct MemoryCapGuard(Option<windows_sys::Win32::Foundation::HANDLE>);

// SAFETY: the job handle is an owned kernel handle; Windows job objects are
// safe to use and close from any thread.
#[cfg(windows)]
unsafe impl Send for MemoryCapGuard {}

#[cfg(windows)]
impl MemoryCapGuard {
    pub(crate) fn attach(child: &Child, cap_bytes: Option<u64>) -> Result<Self> {
        use std::mem::{size_of, zeroed};
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_JOB_MEMORY,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectExtendedLimitInformation, SetInformationJobObject,
        };

        let Some(raw) = child.raw_handle() else {
            // The child already exited; nothing to contain.
            return Ok(Self(None));
        };
        let failure = |what: &str| {
            Error::new(
                Status::GenericFailure,
                format!(
                    "Failed to {what} for the language server process guard: {}",
                    std::io::Error::last_os_error()
                ),
            )
        };
        // Always create a kill-on-close Job Object so the entire server tree is
        // torn down when the guard drops — even when no memory cap is configured
        // (`max_memory_mb == 0`). Decoupling tree-kill from the memory cap keeps
        // "no cap" from silently also meaning "no tree teardown". The memory
        // limit is layered on only when a cap is present.
        // SAFETY: null security/name creates an unnamed job owned by the returned handle.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(failure("create a Windows job object"));
        }
        // SAFETY: the Windows structure is plain data and zero is its documented baseline.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if let Some(cap_bytes) = cap_bytes {
            info.BasicLimitInformation.LimitFlags |= JOB_OBJECT_LIMIT_JOB_MEMORY;
            info.JobMemoryLimit = usize::try_from(cap_bytes).unwrap_or(usize::MAX);
        }
        // SAFETY: job is valid and info matches this information class.
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } != 0;
        // SAFETY: child owns a valid process handle while `Child` is alive.
        let assigned = configured && unsafe { AssignProcessToJobObject(job, raw as _) } != 0;
        if !assigned {
            // SAFETY: job is owned by this function and not yet published.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
            return Err(failure("apply the Windows job object"));
        }
        Ok(Self(Some(job)))
    }
}

#[cfg(windows)]
impl Drop for MemoryCapGuard {
    fn drop(&mut self) {
        if let Some(job) = self.0.take() {
            // Closing a kill-on-close job also terminates any straggler tree.
            // SAFETY: the handle is owned by this guard.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_memory_cap_resolves_default_disable_and_explicit_values() {
        assert_eq!(
            memory_cap_bytes(None),
            Some(u64::from(DEFAULT_LSP_MEMORY_CAP_MB) * 1024 * 1024)
        );
        assert_eq!(memory_cap_bytes(Some(0)), None);
        assert_eq!(memory_cap_bytes(Some(512)), Some(512 * 1024 * 1024));
    }

    /// A tree over the cap is reported and killed; one under it is
    /// left alone.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[tokio::test]
    async fn memory_watchdog_kills_a_tree_over_its_cap() {
        let has_python = std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
        if !has_python {
            return;
        }
        let spawn = |megabytes: u32| {
            let mut command = Command::new("python3");
            command
                .args([
                    "-c",
                    &format!("import time\nx = b'a' * ({megabytes} << 20)\ntime.sleep(60)\n"),
                ])
                .process_group(0)
                .kill_on_drop(true);
            command.spawn().expect("spawn python")
        };
        let mut hog = spawn(200);
        let pid = hog.id().expect("pid");
        let exceeded = std::sync::Arc::new(std::sync::Mutex::new(None));
        let seen = std::sync::Arc::clone(&exceeded);
        let cap = 64 * 1024 * 1024;
        tokio::time::timeout(
            Duration::from_secs(30),
            watch_memory(
                pid,
                cap,
                Duration::from_millis(100),
                || true,
                move |rss| {
                    *seen.lock().expect("lock") = Some(rss);
                },
            ),
        )
        .await
        .expect("the watchdog acts well within the timeout");
        let rss = exceeded.lock().expect("lock").expect("cap exceeded");
        assert!(rss > cap, "{rss}");
        let status = tokio::time::timeout(Duration::from_secs(10), hog.wait())
            .await
            .expect("the hog is killed")
            .expect("wait");
        assert!(!status.success());
        assert!(memory_cap_exceeded_message(rss, cap).contains("exceeded memory cap"));

        // Under the cap: the watchdog keeps sampling until `alive` ends it.
        let mut small = spawn(1);
        let pid = small.id().expect("pid");
        let samples = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = std::sync::Arc::clone(&samples);
        tokio::time::timeout(
            Duration::from_secs(30),
            watch_memory(
                pid,
                1024 * 1024 * 1024,
                Duration::from_millis(50),
                move || counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 5,
                |_| panic!("under the cap"),
            ),
        )
        .await
        .expect("alive() ends the watchdog");
        assert!(
            small.try_wait().expect("try_wait").is_none(),
            "left running"
        );
        let _ = small.kill().await;
    }

    /// Linux enforces the cap with `RLIMIT_AS` at spawn: the default cap still
    /// lets a child start, and an allocation past a small cap fails inside
    /// the child instead of growing host memory.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn rlimit_as_cap_allows_spawn_and_blocks_allocation_past_it_on_linux() {
        let mut command = Command::new("/bin/true");
        apply_pre_spawn_cap(&mut command, memory_cap_bytes(None).expect("default cap"));
        assert!(
            command
                .status()
                .await
                .expect("spawn under default cap")
                .success()
        );

        let has_python = std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success());
        if !has_python {
            return;
        }
        let run = |megabytes: u32| {
            let mut command = Command::new("python3");
            command.args([
                "-c",
                &format!("x = bytearray({megabytes} << 20)\nprint(len(x))\n"),
            ]);
            apply_pre_spawn_cap(&mut command, 256 * 1024 * 1024);
            command.output()
        };
        let small = run(16).await.expect("spawn python under the cap");
        assert!(small.status.success(), "16 MiB fits a 256 MiB cap");
        let over = run(512).await.expect("spawn python under the cap");
        assert!(
            !over.status.success(),
            "512 MiB must fail under a 256 MiB cap"
        );
        assert!(
            String::from_utf8_lossy(&over.stderr).contains("MemoryError"),
            "{}",
            String::from_utf8_lossy(&over.stderr)
        );
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn default_cap_does_not_prevent_process_spawn_on_macos() {
        let mut command = Command::new("/usr/bin/true");
        if let Some(cap_bytes) = memory_cap_bytes(None) {
            apply_pre_spawn_cap(&mut command, cap_bytes);
        }
        match command.status().await {
            Ok(status) => assert!(status.success()),
            Err(error) => panic!("macOS child must spawn with the default policy: {error}"),
        }
    }
}
