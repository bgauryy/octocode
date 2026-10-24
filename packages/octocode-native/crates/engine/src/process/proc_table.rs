//! The Linux `/proc` readers: process table, CPU ticks and resident pages.
//! Plain file I/O over a `/proc`-layout directory, so the synthetic-tree
//! tests run them on every host.

use std::path::Path;

/// `(pid, ppid)` from one `/proc/<pid>/stat` line: `pid (comm) state ppid …`.
/// `comm` is the executable name and may itself hold spaces, `(` and `)`, so
/// the fields after it are located from the *last* `)`.
pub(super) fn parse_proc_stat(stat: &str) -> Option<(u32, u32)> {
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
pub(super) fn parse_proc_statm_rss(statm: &str, page_size: u64) -> Option<u64> {
    let pages = statm.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    Some(pages.saturating_mul(page_size))
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

/// `(pid, ppid)` of every process under a `/proc`-layout directory, read in
/// one pass. Entries that are not numeric, vanish mid-scan, or whose `stat`
/// is unreadable or names another pid are skipped. Plain file I/O, so the
/// Linux reader runs against a synthetic tree on any host.
pub(super) fn read_proc_table(proc_root: &Path) -> Vec<(u32, u32)> {
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
pub(super) fn children_from_table(table: Vec<(u32, u32)>) -> impl Fn(u32) -> Vec<u32> {
    move |parent| {
        table
            .iter()
            .filter(|&&(pid, ppid)| ppid == parent && pid != parent)
            .map(|&(pid, _)| pid)
            .collect()
    }
}

/// Resident bytes of `pid` under a `/proc`-layout directory.
pub(super) fn read_proc_rss(proc_root: &Path, pid: u32, page_size: u64) -> Option<u64> {
    let statm = std::fs::read_to_string(proc_root.join(pid.to_string()).join("statm")).ok()?;
    parse_proc_statm_rss(&statm, page_size)
}
