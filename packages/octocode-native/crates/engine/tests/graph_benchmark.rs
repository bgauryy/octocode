use octocode_engine::graph::{condense, scc};

#[path = "../benches/support/frozen_graph.rs"]
mod frozen;
use frozen::frozen_graph;

#[test]
fn frozen_graph_correctness_receipt_is_stable() {
    let graph = frozen_graph(500, 3);
    assert_eq!(scc(&graph, false).len(), 500);
    assert!(scc(&graph, true).is_empty(), "the frozen graph is acyclic");
    let condensed = condense(&graph);
    assert_eq!(condensed.components.len(), 500);
    assert_eq!(
        condensed
            .edges
            .values()
            .map(std::collections::BTreeSet::len)
            .sum::<usize>(),
        1_494
    );
}
