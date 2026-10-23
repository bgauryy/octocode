use crate::runtime::engine::channel::batch::BatchBudget;

use super::*;

#[test]
fn independent_batches_fan_out_one_thread_per_query_and_classification_stays_ordered() {
    for id in ToolId::ALL {
        let ordered = matches!(id, ToolId::Clasify);
        let width = BatchBudget::new(id, 5, 12).width;
        assert_eq!(width, if ordered { 1 } else { 5 }, "{id}");
        assert_eq!(BatchBudget::new(id, 1, 12).width, 1, "{id}");
    }
}

#[test]
fn concurrent_walks_split_the_cores_and_a_single_walk_keeps_the_default() {
    // Serializing walks measured slower, so rows stay concurrent and each
    // walk gets its share of the cores (see `BatchBudget`).
    let five = BatchBudget::new(ToolId::LocalSearch, 5, 12);
    assert_eq!(five.width, 5);
    assert_eq!(five.walk_threads, Some(2));
    assert_eq!(
        BatchBudget::new(ToolId::LocalSearch, 1, 12).walk_threads,
        None
    );
    // Never below one worker, even with more walks than cores.
    assert_eq!(
        BatchBudget::new(ToolId::LocalSearch, 5, 2).walk_threads,
        Some(1)
    );
    // Ordered tools run one row at a time with the full default.
    assert_eq!(BatchBudget::new(ToolId::Clasify, 5, 12).walk_threads, None);
}
