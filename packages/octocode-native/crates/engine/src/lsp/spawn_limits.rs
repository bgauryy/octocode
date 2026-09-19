//! OS-level memory containment for spawned language servers.
//!
//! Every internal buffer in the LSP path is bounded, but internal bounds do
//! not stop a runaway *server* from exhausting host memory. The cap is applied
//! at spawn: `setrlimit(RLIMIT_AS)` in `pre_exec` on supported Unix targets,
//! and a Job Object with `JOB_OBJECT_LIMIT_JOB_MEMORY` on Windows. macOS is
//! excluded because lowering `RLIMIT_AS` below the process's inherited virtual
//! mappings makes every child spawn fail with `EINVAL`; buffer/process lifecycle
//! bounds still apply there.

use crate::error::Result;
#[cfg(windows)]
use crate::error::{Error, Status};
use tokio::process::{Child, Command};

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
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
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
