//! Facts about the machine Octocode runs on, read once per process.

/// Memory available to this process in MiB (the host's physical memory, or a
/// container's smaller cgroup limit), `None` where it cannot be read.
/// Language-server caps and the runtime's memory budget derive from it.
pub fn memory_mb() -> Option<u64> {
    static HOST: std::sync::OnceLock<Option<u64>> = std::sync::OnceLock::new();
    *HOST.get_or_init(read_memory_mb)
}

#[cfg(target_os = "macos")]
fn read_memory_mb() -> Option<u64> {
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
fn read_memory_mb() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let cgroup = |file: &str| std::fs::read_to_string(std::path::Path::new(CGROUP_ROOT).join(file));
    let limits = [
        cgroup("memory.max").ok(),
        cgroup("memory/memory.limit_in_bytes").ok(),
    ];
    memory_mb_within_cgroups(&meminfo, limits.iter().flatten().map(String::as_str))
}

#[cfg(target_os = "windows")]
fn read_memory_mb() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: `status` is a zeroed MEMORYSTATUSEX whose `dwLength` is set, as
    // the call requires; it only writes within the struct.
    let total = unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = u32::try_from(std::mem::size_of::<MEMORYSTATUSEX>()).ok()?;
        (GlobalMemoryStatusEx(&raw mut status) != 0).then_some(status.ullTotalPhys)?
    };
    (total > 0).then_some(total / (1024 * 1024))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn read_memory_mb() -> Option<u64> {
    None
}

/// Where a container's cgroup memory limits are mounted, v2 and v1 alike.
#[cfg(target_os = "linux")]
const CGROUP_ROOT: &str = "/sys/fs/cgroup";

/// Limits at or above this are "no limit": cgroup v1 reports unlimited as a
/// page-rounded `i64::MAX`.
#[cfg(any(target_os = "linux", test))]
const UNLIMITED_BYTES: u64 = 1 << 62;

/// `MemTotal` of a `/proc/meminfo` text, in MiB.
#[cfg(any(target_os = "linux", test))]
fn meminfo_total_mb(meminfo: &str) -> Option<u64> {
    let kib = meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    Some(kib / 1024)
}

/// The byte limit in a cgroup v2 `memory.max` or v1 `memory.limit_in_bytes`
/// file; `None` for `max`, an unlimited or unreadable value.
#[cfg(any(target_os = "linux", test))]
fn cgroup_limit_bytes(text: &str) -> Option<u64> {
    let bytes = text.trim().parse::<u64>().ok()?;
    (bytes > 0 && bytes < UNLIMITED_BYTES).then_some(bytes)
}

/// The memory this process may use: the host's `MemTotal` or, when smaller,
/// the tightest cgroup limit among `cgroup_files` (their texts).
#[cfg(any(target_os = "linux", test))]
fn memory_mb_within_cgroups<'a>(
    meminfo: &str,
    cgroup_files: impl IntoIterator<Item = &'a str>,
) -> Option<u64> {
    let total = meminfo_total_mb(meminfo)?;
    Some(
        cgroup_files
            .into_iter()
            .filter_map(cgroup_limit_bytes)
            .map(|bytes| bytes / (1024 * 1024))
            .fold(total, u64::min),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMINFO: &str = "MemTotal:       65536000 kB\nMemFree:  1 kB\n";

    #[test]
    fn meminfo_total_is_mib() {
        assert_eq!(meminfo_total_mb(MEMINFO), Some(64_000));
        assert_eq!(meminfo_total_mb("MemFree: 1 kB\n"), None);
    }

    #[test]
    fn cgroup_limits_ignore_max_and_unlimited_values() {
        assert_eq!(cgroup_limit_bytes("1073741824\n"), Some(1 << 30));
        assert_eq!(cgroup_limit_bytes("max\n"), None);
        assert_eq!(cgroup_limit_bytes("9223372036854771712\n"), None);
        assert_eq!(cgroup_limit_bytes("0"), None);
        assert_eq!(cgroup_limit_bytes(""), None);
    }

    #[test]
    fn the_tightest_limit_wins_and_a_larger_one_never_raises_the_host() {
        assert_eq!(
            memory_mb_within_cgroups(MEMINFO, ["1073741824", "9223372036854771712"]),
            Some(1024)
        );
        assert_eq!(
            memory_mb_within_cgroups(MEMINFO, ["max", "2147483648", "1073741824"]),
            Some(1024)
        );
        assert_eq!(
            memory_mb_within_cgroups(MEMINFO, ["137438953472"]),
            Some(64_000)
        );
        assert_eq!(memory_mb_within_cgroups(MEMINFO, ["max"]), Some(64_000));
        assert_eq!(memory_mb_within_cgroups("", ["1073741824"]), None);
    }
}
