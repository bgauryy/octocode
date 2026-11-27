//! Tearing a process tree down and waiting on a child without reaping it.

#[cfg(unix)]
use super::MAX_TREE_PROCESSES;
#[cfg(unix)]
use super::identity::{LiveTable, Process, ProcessTable, signal_process};

/// The descendants of a live process, read before it is asked to exit. A
/// descendant whose parent exits is re-parented away (to init or launchd),
/// so a walk after a graceful exit no longer finds it: a server's detached
/// daemon (jdtls starts a Gradle daemon in its own session) survives the
/// group sweep. [`kill_tree_with`] also ends every process held here, and
/// what it started since, while its pid still names the process seen.
#[derive(Clone, Debug, Default)]
pub struct TreeSnapshot {
    #[cfg(unix)]
    processes: Vec<Process>,
}

impl TreeSnapshot {
    /// The current descendants of `root` (never `root` itself).
    #[cfg(unix)]
    pub fn of(root: u32) -> Self {
        Self {
            processes: super::descendants(root),
        }
    }

    /// Nothing to record off Unix: the [`Job`](super::Job) holds the tree.
    #[cfg(not(unix))]
    pub fn of(_root: u32) -> Self {
        Self::default()
    }

    /// [`Self::of`] from async code, on the blocking pool (the walk reads the
    /// process table).
    pub async fn read(root: u32) -> Self {
        tokio::task::spawn_blocking(move || Self::of(root))
            .await
            .unwrap_or_default()
    }
}

/// SIGKILL `root`, its process group, and every descendant — including
/// ones that left the group with `setsid`. `root` must not have been reaped
/// yet, so neither its pid nor its group id can have been recycled; its
/// descendants are not ours to reap, so each is signaled only while its pid
/// still names the process the walk saw.
#[cfg(unix)]
pub fn kill_tree(root: u32) {
    tear_down(&LiveTable, root, &[]);
}

/// [`kill_tree`], and also every process in `known` (a [`TreeSnapshot`] of
/// `root` taken while it was alive) with its own descendants, even after
/// `root` exited and they were re-parented away.
#[cfg(unix)]
pub fn kill_tree_with(root: u32, known: &TreeSnapshot) {
    tear_down(&LiveTable, root, &known.processes);
}

/// [`kill_tree`] over any [`ProcessTable`]. `root` is frozen first, so it
/// cannot fork once the walk starts; each process of `known` that still is
/// the process seen is frozen and walked as a root too. The tree is then
/// frozen and killed while the parent links still hold.
#[cfg(unix)]
pub(super) fn tear_down(table: &impl ProcessTable, root: u32, known: &[Process]) {
    let Ok(root_pid) = libc::pid_t::try_from(root) else {
        return;
    };
    if root_pid <= 1 {
        return;
    }
    table.signal_unreaped(root_pid, libc::SIGSTOP);
    let mut roots = vec![root];
    let mut held = Vec::new();
    for &process in known {
        if signal_process(table, process, libc::SIGSTOP) {
            roots.push(process.pid);
            held.push(process);
        }
    }
    let tree = freeze_descendants(table, &roots, &held);
    // `root` leads its own process group (spawned with `process_group(0)`)
    // and is not reaped, so `-root` names exactly that group.
    table.signal_unreaped(-root_pid, libc::SIGKILL);
    table.signal_unreaped(root_pid, libc::SIGKILL);
    for process in tree {
        signal_process(table, process, libc::SIGKILL);
    }
}

/// SIGSTOP every descendant of the frozen `roots`, in walk order; `held`
/// (already frozen) are not stopped again. A descendant can fork between a
/// walk and its SIGSTOP, so the walk repeats until it finds no new process;
/// every round adds one, and the tree is bounded by [`MAX_TREE_PROCESSES`].
/// Returns every process found, after `held`.
#[cfg(unix)]
fn freeze_descendants(table: &impl ProcessTable, roots: &[u32], held: &[Process]) -> Vec<Process> {
    let mut seen = held
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    let mut tree = held.to_vec();
    loop {
        let known = tree.len();
        for &root in roots {
            for process in table.descendants(root) {
                if seen.insert(process) {
                    signal_process(table, process, libc::SIGSTOP);
                    tree.push(process);
                }
            }
        }
        if tree.len() == known || tree.len() >= MAX_TREE_PROCESSES {
            return tree;
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

/// Non-Unix process trees are contained by their [`Job`].
#[cfg(not(unix))]
pub fn kill_tree_with(_root: u32, _known: &TreeSnapshot) {}

/// [`kill_tree`] from async code. The sweep walks the process table, so it
/// runs on the blocking pool rather than stalling a runtime worker, and this
/// returns once it finished. A runtime that is shutting down cancels the
/// blocking task before it starts; the sweep then runs here, because a
/// skipped kill would leak the tree.
pub async fn kill_tree_blocking(root: u32) {
    kill_tree_blocking_with(root, TreeSnapshot::default()).await;
}

/// [`kill_tree_with`] from async code, as [`kill_tree_blocking`] runs it.
pub async fn kill_tree_blocking_with(root: u32, known: TreeSnapshot) {
    let held = known.clone();
    if let Err(error) = tokio::task::spawn_blocking(move || kill_tree_with(root, &held)).await
        && error.is_cancelled()
    {
        kill_tree_with(root, &known);
    }
}
