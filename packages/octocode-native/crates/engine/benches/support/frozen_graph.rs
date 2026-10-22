//! The frozen benchmark graph, shared by `benches/graph.rs` and the
//! correctness receipt in `tests/graph_benchmark.rs`.
use octocode_engine::graph::FileGraphNode;
use std::collections::{BTreeMap, BTreeSet};

/// `files` Rust files where each one uses the next `fanout` files.
pub fn frozen_graph(files: usize, fanout: usize) -> BTreeMap<String, FileGraphNode> {
    let mut graph = BTreeMap::new();
    for index in 0..files {
        let edges = (1..=fanout)
            .filter_map(|distance| {
                let target = index + distance;
                (target < files).then(|| {
                    (
                        format!("src/file-{target:05}.rs"),
                        BTreeMap::from([("rust-use".to_owned(), 1)]),
                    )
                })
            })
            .collect();
        graph.insert(
            format!("src/file-{index:05}.rs"),
            FileGraphNode {
                edges,
                dynamic_only: BTreeSet::new(),
            },
        );
    }
    graph
}
