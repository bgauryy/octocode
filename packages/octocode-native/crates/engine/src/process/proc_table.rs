//! The Linux `/proc` readers: process table, CPU ticks and memory.
//! Plain file I/O over a `/proc`-layout directory, so the synthetic-tree
//! tests run them on every host.

use std::path::Path;

use super::Process;

/// The identity fields of one `/proc/<pid>/stat` record, read together so
/// the parent link and the start time belong to the same process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ProcStat {
    pub(super) pid: u32,
    pub(super) ppid: u32,
    /// `starttime`: clock ticks after boot.
    pub(super) start: u64,
}

/// [`ProcStat`] from one `/proc/<pid>/stat` line: `pid (comm) state ppid …
/// starttime …`. `comm` is the executable name and may itself hold spaces,
/// `(` and `)`, so the fields after it are located from the *last* `)`.
pub(super) fn parse_proc_stat(stat: &str) -> Option<ProcStat> {
    /// `starttime` is field 22 of proc(5); counting from `state` (field 3),
    /// it is 17 fields after `ppid`.
    const START_AFTER_PPID: usize = 17;
    let open = stat.find(" (")?;
    let pid = stat[..open].trim().parse().ok()?;
    let close = stat.rfind(')')?;
    if close < open {
        return None;
    }
    let mut fields = stat[close + 1..].split_whitespace();
    let _state = fields.next()?;
    let ppid = fields.next()?.parse().ok()?;
    let start = fields.nth(START_AFTER_PPID)?.parse().ok()?;
    Some(ProcStat { pid, ppid, start })
}

/// [`ProcStat`] of `pid` under a `/proc`-layout directory; `None` when the
/// process is gone or its `stat` names another pid.
pub(super) fn read_proc_stat(proc_root: &Path, pid: u32) -> Option<ProcStat> {
    let stat = std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
    parse_proc_stat(&stat).filter(|stat| stat.pid == pid)
}

/// Bytes a process holds, from its `/proc/<pid>/status` text: `VmRSS` (in
/// RAM) plus `VmSwap` (swapped out, to zram or zswap too), both in kB. A
/// zombie lists neither and reads `0`, which is correct.
pub(super) fn parse_proc_status_memory(status: &str) -> u64 {
    status
        .lines()
        .filter_map(|line| {
            line.strip_prefix("VmRSS:")
                .or_else(|| line.strip_prefix("VmSwap:"))
        })
        .filter_map(|value| value.trim().strip_suffix("kB")?.trim().parse::<u64>().ok())
        .fold(0, |total, kib| {
            total.saturating_add(kib.saturating_mul(1024))
        })
}

/// CPU clock ticks from one `/proc/<pid>/stat` line: `utime + stime` of the
/// process plus `cutime + cstime` of the children it has reaped, so a
/// finished `cargo` or proc-macro helper stays counted. Fields are located
/// after the last `)`, as in [`parse_proc_stat`].
pub(super) fn parse_proc_stat_cpu_ticks(stat: &str) -> Option<u64> {
    /// Fields after `comm`, counting from `state`: `utime stime cutime cstime`
    /// are fields 14-17 of proc(5), i.e. offsets 11-14 here.
    const FIRST_CPU_FIELD: usize = 11;
    let close = stat.rfind(')')?;
    let mut fields = stat[close + 1..].split_whitespace().skip(FIRST_CPU_FIELD);
    let mut ticks = 0u64;
    for _ in 0..4 {
        ticks = ticks.saturating_add(fields.next()?.parse::<u64>().ok()?);
    }
    Some(ticks)
}

/// CPU clock ticks of `pid` under a `/proc`-layout directory.
pub(super) fn read_proc_cpu_ticks(proc_root: &Path, pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
    parse_proc_stat_cpu_ticks(&stat)
}

/// [`ProcStat`] of every process under a `/proc`-layout directory, read in
/// one pass. Entries that are not numeric, vanish mid-scan, or whose `stat`
/// is unreadable or names another pid are skipped. Plain file I/O, so the
/// Linux reader runs against a synthetic tree on any host.
pub(super) fn read_proc_table(proc_root: &Path) -> Vec<ProcStat> {
    std::fs::read_dir(proc_root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| read_proc_stat(proc_root, pid))
        .collect()
}

/// `pid → direct children` over a [`ProcStat`] snapshot.
pub(super) fn children_from_table(table: Vec<ProcStat>) -> impl Fn(u32) -> Vec<Process> {
    move |parent| {
        table
            .iter()
            .filter(|stat| stat.ppid == parent && stat.pid != parent)
            .map(|stat| Process {
                pid: stat.pid,
                start: stat.start,
            })
            .collect()
    }
}

/// Bytes `pid` holds under a `/proc`-layout directory (see
/// [`parse_proc_status_memory`]); `None` when the process is gone.
pub(super) fn read_proc_memory(proc_root: &Path, pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(proc_root.join(pid.to_string()).join("status")).ok()?;
    Some(parse_proc_status_memory(&status))
}
