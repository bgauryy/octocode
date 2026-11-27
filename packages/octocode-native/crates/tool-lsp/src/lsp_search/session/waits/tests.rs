use super::*;

#[test]
fn the_pool_overflow_warning_states_both_sizes_and_why() {
    let mib = 1024 * 1024;
    let warning = disclose_pool_overflow(300 * mib + 1, 200 * mib);
    assert!(warning.contains("about 301 MiB"), "{warning}");
    assert!(warning.contains("over its 200 MiB budget"), "{warning}");
    assert!(warning.contains("as its request ends"), "{warning}");
}
