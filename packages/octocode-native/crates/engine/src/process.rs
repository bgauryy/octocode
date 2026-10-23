//! Process containment and tree inspection for spawned commands: the
//! descendants of a pid (following parent links, so a child that called
//! `setsid` and left the process group is still found), their resident
//! memory and CPU time, and a freeze-then-kill teardown of the whole tree.
//!
//! Implemented on macOS (`libproc`) and Linux (`/proc`); Windows reads the
//! server process's own CPU time. Elsewhere the tree walk finds nothing,
//! there is no RSS or CPU reader, and teardown falls back to the process
//! group.

use std::collections::HashSet;
#[cfg(target_os = "linux")]
use std::path::Path;

mod job;
mod metrics;
mod proc_table;
mod teardown;

pub use job::Job;
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
pub(crate) use metrics::TreeCpuClock;
#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
use metrics::rss_bytes;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) use metrics::tree_rss_bytes;
#[cfg(any(test, target_os = "linux"))]
use proc_table::{children_from_table, read_proc_table};
#[cfg(test)]
use proc_table::{
    parse_proc_stat, parse_proc_stat_cpu_ticks, parse_proc_statm_rss, read_proc_cpu_ticks,
    read_proc_rss,
};
#[cfg(unix)]
pub use teardown::exited_without_reaping;
#[cfg(unix)]
pub(crate) use teardown::wait_for_exit_without_reaping;
pub use teardown::{kill_tree, kill_tree_blocking};

/// Upper bound on the processes one tree walk visits, so a fork bomb cannot
/// make the walk itself unbounded.
const MAX_TREE_PROCESSES: usize = 4_096;

/// Every descendant of `root` (not `root` itself), breadth-first. Parent links
/// are followed, not the process group or session, so descendants that
/// called `setsid` are included. A child whose parent already exited has
/// been re-parented away and cannot be found this way; callers snapshot the
/// tree before killing any of it.
#[cfg(unix)]
pub(crate) fn descendants(root: u32) -> Vec<u32> {
    walk_descendants(root, children_lookup())
}

/// The one tree walk shared by every platform: `children_of(pid)` yields
/// the direct children of `pid` (from `libproc` on macOS, from a `/proc`
/// snapshot on Linux). Pure over the lookup, so cycles (a self-parented or
/// looping table), duplicates, and the [`MAX_TREE_PROCESSES`] bound are
/// covered by tests on every host. `root` is never reported.
#[cfg_attr(not(unix), allow(dead_code))] // No process-tree reader off Unix.
fn walk_descendants(root: u32, mut children_of: impl FnMut(u32) -> Vec<u32>) -> Vec<u32> {
    let mut seen = HashSet::from([root]);
    let mut found = Vec::new();
    let mut frontier = std::collections::VecDeque::from([root]);
    while let Some(parent) = frontier.pop_front() {
        for child in children_of(parent) {
            if !seen.insert(child) {
                continue;
            }
            if found.len() >= MAX_TREE_PROCESSES {
                return found;
            }
            found.push(child);
            frontier.push_back(child);
        }
    }
    found
}

/// `pid → direct children` for the current platform.
#[cfg(target_os = "macos")]
fn children_lookup() -> impl Fn(u32) -> Vec<u32> {
    |parent| {
        /// `PROC_PPID_ONLY` from `<libproc.h>` (not exported by `libc`).
        const PROC_PPID_ONLY: u32 = 6;
        let mut pids = vec![0 as libc::pid_t; 256];
        loop {
            let Ok(bytes) = libc::c_int::try_from(pids.len() * std::mem::size_of::<libc::pid_t>())
            else {
                return Vec::new();
            };
            // SAFETY: the buffer holds `bytes` writable bytes of `pid_t`s.
            let written = unsafe {
                libc::proc_listpids(PROC_PPID_ONLY, parent, pids.as_mut_ptr().cast(), bytes)
            };
            let Ok(written) = usize::try_from(written) else {
                return Vec::new();
            };
            let count = written / std::mem::size_of::<libc::pid_t>();
            if count < pids.len() || pids.len() >= MAX_TREE_PROCESSES {
                return pids[..count.min(pids.len())]
                    .iter()
                    .filter_map(|&pid| u32::try_from(pid).ok())
                    .filter(|&pid| pid != 0)
                    .collect();
            }
            pids.resize(pids.len() * 2, 0);
        }
    }
}

#[cfg(target_os = "linux")]
fn children_lookup() -> impl Fn(u32) -> Vec<u32> {
    // One `/proc` scan per walk.
    children_from_table(read_proc_table(Path::new("/proc")))
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn children_lookup() -> impl Fn(u32) -> Vec<u32> {
    |_| Vec::new()
}

/// Give a child its own process group before spawning it. Windows containment
/// is attached immediately after spawn through [`Job`].
pub fn contain(command: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = command;
}

#[cfg(test)]
mod tests;
