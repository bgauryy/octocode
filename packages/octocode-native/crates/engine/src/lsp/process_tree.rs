//! Process-tree inspection for spawned language servers and probes: the
//! descendants of a pid (following parent links, so a child that called
//! `setsid` and left the process group is still found), their resident
//! memory, and a freeze-then-kill teardown of the whole tree.
//!
//! Implemented on macOS (`libproc`) and Linux (`/proc`). Elsewhere the
//! queries return nothing and teardown falls back to the process group.

use std::collections::HashSet;
use std::path::Path;

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

/// `(pid, ppid)` from one `/proc/<pid>/stat` line: `pid (comm) state ppid …`.
/// `comm` is the executable name and may itself hold spaces, `(` and `)`, so
/// the fields after it are located from the *last* `)`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Linux reader; tested everywhere.
fn parse_proc_stat(stat: &str) -> Option<(u32, u32)> {
    let open = stat.find(" (")?;
    let pid = stat[..open].trim().parse().ok()?;
    let close = stat.rfind(')')?;
    if close < open {
        return None;
    }
    let mut fields = stat[close + 1..].split_whitespace();
    let _state = fields.next()?;
    let ppid = fields.next()?.parse().ok()?;
    Some((pid, ppid))
}

/// Resident bytes from one `/proc/<pid>/statm` line (`size resident …`, in
/// pages). A zombie reports `0` resident pages, which is correct.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Linux reader; tested everywhere.
fn parse_proc_statm_rss(statm: &str, page_size: u64) -> Option<u64> {
    let pages = statm.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    Some(pages.saturating_mul(page_size))
}

/// `(pid, ppid)` of every process under a `/proc`-layout directory, read in
/// one pass. Entries that are not numeric, vanish mid-scan, or whose `stat`
/// is unreadable or names another pid are skipped. Plain file I/O, so the
/// Linux reader runs against a synthetic tree on any host.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Linux reader; tested everywhere.
fn read_proc_table(proc_root: &Path) -> Vec<(u32, u32)> {
    std::fs::read_dir(proc_root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| {
            let stat =
                std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
            let (stat_pid, ppid) = parse_proc_stat(&stat)?;
            (stat_pid == pid).then_some((pid, ppid))
        })
        .collect()
}

/// `pid → direct children` over a `(pid, ppid)` snapshot.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Linux reader; tested everywhere.
fn children_from_table(table: Vec<(u32, u32)>) -> impl Fn(u32) -> Vec<u32> {
    move |parent| {
        table
            .iter()
            .filter(|&&(pid, ppid)| ppid == parent && pid != parent)
            .map(|&(pid, _)| pid)
            .collect()
    }
}

/// Resident bytes of `pid` under a `/proc`-layout directory.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Linux reader; tested everywhere.
fn read_proc_rss(proc_root: &Path, pid: u32, page_size: u64) -> Option<u64> {
    let statm = std::fs::read_to_string(proc_root.join(pid.to_string()).join("statm")).ok()?;
    parse_proc_statm_rss(&statm, page_size)
}

/// Resident set size of `pid` in bytes; `None` when it cannot be read (the
/// process is gone, or the platform has no reader).
#[cfg(target_os = "macos")]
pub(crate) fn rss_bytes(pid: u32) -> Option<u64> {
    let pid = libc::c_int::try_from(pid).ok()?;
    // SAFETY: `proc_taskinfo` is plain data; zero is a valid initial value.
    let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
    let size = libc::c_int::try_from(std::mem::size_of::<libc::proc_taskinfo>()).ok()?;
    // SAFETY: the buffer is a live `proc_taskinfo` of exactly `size` bytes,
    // the layout `PROC_PIDTASKINFO` writes.
    let written =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDTASKINFO, 0, (&raw mut info).cast(), size) };
    (written == size).then_some(info.pti_resident_size)
}

#[cfg(target_os = "linux")]
pub(crate) fn rss_bytes(pid: u32) -> Option<u64> {
    // SAFETY: `sysconf` has no preconditions.
    let page_size = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).ok()?;
    read_proc_rss(Path::new("/proc"), pid, page_size)
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
pub(crate) fn rss_bytes(_pid: u32) -> Option<u64> {
    None
}

/// Resident memory of `root` plus every descendant, in bytes; `None` when
/// `root` itself cannot be read (it exited).
#[cfg(unix)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // Wired on macOS only.
pub(crate) fn tree_rss_bytes(root: u32) -> Option<u64> {
    let own = rss_bytes(root)?;
    Some(
        descendants(root)
            .into_iter()
            .filter_map(rss_bytes)
            .fold(own, u64::saturating_add),
    )
}

/// SIGKILL `root`, its process group, and every descendant — including
/// ones that left the group with `setsid`. The tree is frozen with SIGSTOP
/// first (a few rounds, so children forked mid-walk are caught), then killed
/// while the parent links still hold. `root` must not have been reaped yet,
/// so neither its pid nor its group id can have been recycled.
#[cfg(unix)]
pub(crate) fn kill_tree(root: u32) {
    let Ok(root_pid) = i32::try_from(root) else {
        return;
    };
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

/// Platform-independent tests of the tree walk and the Linux `/proc`
/// readers, run against synthetic tables and a synthetic `/proc` directory so
/// the Linux-only parsing is exercised on every host.
#[cfg(test)]
mod pure_tests {
    use super::*;
    use std::path::PathBuf;

    struct FakeProc(PathBuf);

    impl FakeProc {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("octocode-fake-proc-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("fake proc dir");
            Self(dir)
        }

        /// A process whose `stat` is `pid (comm) S ppid …` and whose
        /// `statm` reports `resident` pages.
        fn process(&self, pid: u32, comm: &str, ppid: u32, resident: u64) -> &Self {
            let dir = self.0.join(pid.to_string());
            std::fs::create_dir_all(&dir).expect("pid dir");
            std::fs::write(
                dir.join("stat"),
                format!("{pid} ({comm}) S {ppid} {pid} {pid} 0 -1 4194560 100 0 0 0\n"),
            )
            .expect("stat");
            std::fs::write(
                dir.join("statm"),
                format!("1000 {resident} 50 10 0 200 0\n"),
            )
            .expect("statm");
            self
        }

        fn descendants(&self, root: u32) -> Vec<u32> {
            let mut found = walk_descendants(root, children_from_table(read_proc_table(&self.0)));
            found.sort_unstable();
            found
        }
    }

    impl Drop for FakeProc {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn stat_parsing_survives_hostile_comm_fields() {
        assert_eq!(parse_proc_stat("42 (node) S 7 42 42 0"), Some((42, 7)));
        assert_eq!(parse_proc_stat("42 (my server) R 7 1 1"), Some((42, 7)));
        // `comm` holding `)`, `(`, and digits must not shift the fields.
        assert_eq!(parse_proc_stat("42 (a) S 99 (b)) S 7 1 1"), Some((42, 7)));
        assert_eq!(parse_proc_stat("42 ((x) 1 2) Z 7 1 1"), Some((42, 7)));
        assert_eq!(parse_proc_stat("42 () S 7"), Some((42, 7)));
        // Truncated or malformed lines yield nothing rather than a bogus ppid.
        assert_eq!(parse_proc_stat("42 (node) S"), None);
        assert_eq!(parse_proc_stat("42 (node"), None);
        assert_eq!(parse_proc_stat("x (node) S 7"), None);
        assert_eq!(parse_proc_stat("42 (node) S -1"), None);
        assert_eq!(parse_proc_stat(""), None);
    }

    #[test]
    fn statm_parsing_scales_resident_pages() {
        assert_eq!(
            parse_proc_statm_rss("1000 25 3 1 0 9 0\n", 4096),
            Some(25 * 4096)
        );
        assert_eq!(parse_proc_statm_rss("0 0 0 0 0 0 0", 16_384), Some(0));
        assert_eq!(parse_proc_statm_rss("1000", 4096), None);
        assert_eq!(parse_proc_statm_rss("", 4096), None);
        assert_eq!(
            parse_proc_statm_rss("1 18446744073709551615", 4096),
            Some(u64::MAX)
        );
    }

    #[test]
    fn synthetic_proc_tree_finds_setsid_style_descendants_with_hostile_names() {
        let proc = FakeProc::new("tree");
        proc.process(1, "init", 0, 10)
            .process(100, "language server", 1, 10)
            .process(101, "node) S 1 (evil", 100, 10)
            .process(102, "rust-analyzer-proc-macro-srv", 101, 10)
            .process(200, "unrelated", 1, 10)
            .process(201, "unrelated child", 200, 10);
        // Non-pid entries in /proc are ignored.
        std::fs::create_dir_all(proc.0.join("self")).expect("self");
        std::fs::write(proc.0.join("uptime"), "1.0 2.0").expect("uptime");
        assert_eq!(proc.descendants(100), vec![101, 102]);
        assert_eq!(proc.descendants(102), Vec::<u32>::new());
        assert_eq!(proc.descendants(999), Vec::<u32>::new());
    }

    #[test]
    fn synthetic_proc_tree_skips_vanished_zombie_and_mismatched_entries() {
        let proc = FakeProc::new("vanished");
        proc.process(10, "leader", 1, 5)
            .process(11, "zombie", 10, 0)
            .process(12, "child", 10, 5);
        // A pid directory whose process exited mid-scan (no `stat`).
        std::fs::create_dir_all(proc.0.join("13")).expect("vanished dir");
        // A `stat` naming another pid (recycled/garbage) is not trusted.
        let bogus = proc.0.join("14");
        std::fs::create_dir_all(&bogus).expect("bogus dir");
        std::fs::write(bogus.join("stat"), "99 (x) S 10 0 0").expect("stat");
        assert_eq!(proc.descendants(10), vec![11, 12]);
        assert_eq!(read_proc_rss(&proc.0, 11, 4096), Some(0), "zombie");
        assert_eq!(read_proc_rss(&proc.0, 12, 4096), Some(5 * 4096));
        assert_eq!(read_proc_rss(&proc.0, 13, 4096), None, "vanished");
        assert_eq!(read_proc_table(&proc.0.join("missing")), Vec::new());
    }

    #[test]
    fn walk_terminates_on_cycles_self_parents_and_duplicates() {
        // 1 → 2 → 3 → 1 cycle, 4 parented to itself, 2 listed twice.
        let table = vec![(2, 1), (3, 2), (1, 3), (4, 4), (5, 4), (2, 1)];
        let mut found = walk_descendants(1, children_from_table(table.clone()));
        found.sort_unstable();
        assert_eq!(found, vec![2, 3], "root is never reported, cycle ends");
        let mut from_self = walk_descendants(4, children_from_table(table));
        from_self.sort_unstable();
        assert_eq!(from_self, vec![5]);
        // A lookup that reports a node as its own child still terminates.
        assert_eq!(
            walk_descendants(7, |pid| vec![pid, pid + 1]
                .into_iter()
                .filter(|&p| p < 10)
                .collect()),
            vec![8, 9]
        );
    }

    #[test]
    fn walk_handles_deep_trees_and_stops_at_the_process_bound() {
        // A 3 000-deep chain: iterative, so no stack overflow.
        let chain: Vec<(u32, u32)> = (1..=3_000).map(|pid| (pid + 1, pid)).collect();
        assert_eq!(walk_descendants(1, children_from_table(chain)).len(), 3_000);
        // A fan-out beyond the bound is truncated at exactly the bound.
        let fan: Vec<(u32, u32)> = (2..=10_000).map(|pid| (pid, 1)).collect();
        assert_eq!(
            walk_descendants(1, children_from_table(fan)).len(),
            MAX_TREE_PROCESSES
        );
        // Breadth-first order: every child before any grandchild.
        let tree = vec![(2, 1), (3, 1), (4, 2), (5, 3)];
        let order = walk_descendants(1, children_from_table(tree));
        assert_eq!(&order[..2], &[2, 3]);
    }
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn python_available() -> bool {
        std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn wait_for_pid_file(path: &std::path::Path) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(pid) = std::fs::read_to_string(path)
                .ok()
                .and_then(|text| text.trim().parse().ok())
            {
                return pid;
            }
            assert!(Instant::now() < deadline, "pid file never written");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn process_gone(pid: u32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            // SAFETY: signal 0 only checks existence.
            let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
            if !alive || rss_bytes(pid).is_none() {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn tree_walk_and_kill_reach_a_setsid_grandchild() {
        if !python_available() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("octocode-tree-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let pid_file = dir.join("grandchild.pid");
        let script = "import os,sys,time\nif os.fork()==0:\n    os.setsid()\n    open(sys.argv[1],'w').write(str(os.getpid()))\n    time.sleep(60)\nelse:\n    time.sleep(60)\n";
        let mut child = {
            use std::os::unix::process::CommandExt;
            std::process::Command::new("python3")
                .args(["-c", script])
                .arg(&pid_file)
                .process_group(0)
                .spawn()
                .expect("spawn python")
        };
        let grandchild = wait_for_pid_file(&pid_file);
        assert!(descendants(child.id()).contains(&grandchild));
        assert!(tree_rss_bytes(child.id()).is_some_and(|rss| rss > 0));
        kill_tree(child.id());
        let _ = child.wait();
        assert!(
            process_gone(grandchild),
            "the setsid grandchild must die with the tree"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
