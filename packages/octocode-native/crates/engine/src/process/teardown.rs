//! Tearing a process tree down and waiting on a child without reaping it.

#[cfg(unix)]
use super::descendants;

/// SIGKILL `root`, its process group, and every descendant — including
/// ones that left the group with `setsid`. The tree is frozen with SIGSTOP
/// first (a few rounds, so children forked mid-walk are caught), then killed
/// while the parent links still hold. `root` must not have been reaped yet,
/// so neither its pid nor its group id can have been recycled.
#[cfg(unix)]
pub fn kill_tree(root: u32) {
    let Ok(root_pid) = i32::try_from(root) else {
        return;
    };
    if root_pid <= 1 {
        return;
    }
    let mut tree = Vec::new();
    for _ in 0..3 {
        let snapshot = descendants(root);
        let grew = snapshot.iter().any(|pid| !tree.contains(pid));
        for &pid in &snapshot {
            if !tree.contains(&pid) {
                signal(pid, libc::SIGSTOP);
                tree.push(pid);
            }
        }
        if !grew {
            break;
        }
    }
    signal(root, libc::SIGSTOP);
    // SAFETY: `root` leads its own process group (spawned with
    // `process_group(0)`) and is not reaped, so `-root` names exactly it.
    unsafe {
        libc::kill(-root_pid, libc::SIGKILL);
    }
    signal(root, libc::SIGKILL);
    for pid in tree {
        signal(pid, libc::SIGKILL);
    }
}

#[cfg(unix)]
fn signal(pid: u32, signal: libc::c_int) {
    if let Ok(pid) = i32::try_from(pid)
        && pid > 1
    {
        // SAFETY: plain signal delivery to a positive pid.
        unsafe {
            libc::kill(pid, signal);
        }
    }
}

/// Whether an unreaped child has exited. Its pid stays reserved for tree
/// teardown until the caller subsequently waits on the child.
#[cfg(unix)]
pub fn exited_without_reaping(pid: u32) -> std::io::Result<bool> {
    wait_for_exit(pid, libc::WNOHANG)
}

#[cfg(unix)]
pub(crate) fn wait_for_exit_without_reaping(pid: u32) {
    let _ = wait_for_exit(pid, 0);
}

#[cfg(unix)]
fn wait_for_exit(pid: u32, flags: libc::c_int) -> std::io::Result<bool> {
    loop {
        // SAFETY: zero is a valid baseline for this plain output structure.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        // SAFETY: info is exclusively borrowed and correctly sized; WNOWAIT
        // keeps this caller-owned child waitable for the subsequent sweep.
        let status = unsafe {
            libc::waitid(
                libc::P_PID,
                libc::id_t::from(pid),
                &mut info,
                libc::WEXITED | flags | libc::WNOWAIT,
            )
        };
        if status == 0 {
            // SAFETY: successful waitid initialized siginfo_t; si_pid is the
            // process-id field for this SIGCHLD result.
            return Ok(unsafe { info.si_pid() } != 0);
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINTR) {
            return Err(error);
        }
    }
}

/// Non-Unix process trees are contained by their [`Job`].
#[cfg(not(unix))]
pub fn kill_tree(_root: u32) {}

/// [`kill_tree`] from async code. The sweep walks the process table, so it
/// runs on the blocking pool rather than stalling a runtime worker, and this
/// returns once it finished. A runtime that is shutting down cancels the
/// blocking task before it starts; the sweep then runs here, because a
/// skipped kill would leak the tree.
pub async fn kill_tree_blocking(root: u32) {
    if let Err(error) = tokio::task::spawn_blocking(move || kill_tree(root)).await
        && error.is_cancelled()
    {
        kill_tree(root);
    }
}
