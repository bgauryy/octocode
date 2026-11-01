use crate::runtime::engine::channel::batch::batch_width;

use super::*;

/// Read-only rows run one thread per query; classification stays ordered.
/// How many cores each row's own work takes is the process budget's, not
/// the batch's (`octocode_engine::search::cpu`).
#[test]
fn independent_batches_fan_out_one_thread_per_query_and_classification_stays_ordered() {
    for id in ToolId::ALL {
        let ordered = matches!(id, ToolId::Clasify);
        assert_eq!(batch_width(id, 5), if ordered { 1 } else { 5 }, "{id}");
        assert_eq!(batch_width(id, 1), 1, "{id}");
        assert_eq!(batch_width(id, 0), 1, "{id}");
    }
}
