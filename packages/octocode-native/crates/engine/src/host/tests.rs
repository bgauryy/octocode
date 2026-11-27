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
