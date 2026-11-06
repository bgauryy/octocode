//! How [`TreeCpuClock`] adds a tree's samples, on samples instead of a busy
//! process: a child's time is the tree's, and a process that exits keeps
//! what it used, so the total never falls.

use super::super::metrics::SeenCpu;
use std::collections::HashMap;
use std::time::Duration;

const fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

#[test]
fn a_child_s_time_is_the_tree_s() {
    let mut seen = SeenCpu::default();
    assert_eq!(seen.record(HashMap::from([(1, ms(5))])), ms(5));
    assert_eq!(
        seen.record(HashMap::from([(1, ms(5)), (2, ms(40))])),
        ms(45)
    );
}

#[test]
fn the_total_never_falls_when_a_busy_descendant_exits() {
    let mut seen = SeenCpu::default();
    let busy = seen.record(HashMap::from([(1, ms(1)), (2, ms(300))]));
    let after_exit = seen.record(HashMap::from([(1, ms(2))]));
    assert!(after_exit >= busy, "{after_exit:?} fell below {busy:?}");
    assert_eq!(after_exit, ms(302), "the exited process keeps what it used");
    // A later exit adds its own last time once, never the earlier one again.
    let later = seen.record(HashMap::from([(1, ms(2)), (3, ms(10))]));
    assert_eq!(seen.record(HashMap::from([(1, ms(3))])), ms(313));
    assert!(later <= ms(313));
}

#[test]
fn an_idle_tree_does_not_grow() {
    let mut seen = SeenCpu::default();
    let first = seen.record(HashMap::from([(1, ms(7)), (2, ms(9))]));
    assert_eq!(seen.record(HashMap::from([(1, ms(7)), (2, ms(9))])), first);
}
