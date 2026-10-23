//! Platform-independent tests of the tree walk and the Linux `/proc`
//! readers, run against synthetic tables and a synthetic `/proc` directory so
//! the Linux-only parsing is exercised on every host.

use super::*;
use std::path::PathBuf;

/// A synthetic `/proc` root (`.0`), removed with its temporary
/// directory on drop.
struct FakeProc(
    PathBuf,
    #[expect(dead_code, reason = "held to remove the directory on drop")] tempfile::TempDir,
);

impl FakeProc {
    fn new(name: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("octocode-fake-proc-{name}-"))
            .tempdir()
            .expect("fake proc dir");
        Self(dir.path().to_path_buf(), dir)
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

#[test]
fn stat_cpu_ticks_sum_own_and_reaped_children_time() {
    // utime 10, stime 5, cutime 3, cstime 2 are fields 14-17.
    let line = "42 (node) S 7 42 42 0 -1 4194560 100 0 0 0 10 5 3 2 20 0 4 0 1";
    assert_eq!(parse_proc_stat_cpu_ticks(line), Some(20));
    // `comm` with spaces and parentheses must not shift the fields.
    let hostile = "42 (a) S 9 (b)) S 7 42 42 0 -1 4194560 100 0 0 0 10 5 3 2 20";
    assert_eq!(parse_proc_stat_cpu_ticks(hostile), Some(20));
    // Truncated lines yield nothing rather than a partial sum.
    assert_eq!(
        parse_proc_stat_cpu_ticks("42 (node) S 7 42 42 0 -1 0 1 0 0 0 10 5"),
        None
    );
    assert_eq!(parse_proc_stat_cpu_ticks("42 (node"), None);
}

#[test]
fn proc_cpu_ticks_are_read_per_pid() {
    let proc = FakeProc::new("cpu");
    let dir = proc.0.join("42");
    std::fs::create_dir_all(&dir).expect("pid dir");
    std::fs::write(
        dir.join("stat"),
        "42 (srv) S 1 42 42 0 -1 4194560 100 0 0 0 7 1 0 0 20 0 1 0 5\n",
    )
    .expect("stat");
    assert_eq!(read_proc_cpu_ticks(&proc.0, 42), Some(8));
    assert_eq!(read_proc_cpu_ticks(&proc.0, 43), None, "unknown pid");
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
