use octocode_engine::graph::{
    reachable_files, shortest_file_path, strongly_connected_components, CodeGraphBuilder,
    FileGraphNode,
};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

fn frozen_graph(files: usize, fanout: usize) -> BTreeMap<String, FileGraphNode> {
    let mut graph = BTreeMap::new();
    for index in 0..files {
        let edges = (1..=fanout)
            .filter_map(|distance| {
                let target = index + distance;
                (target < files).then(|| {
                    (
                        format!("src/file-{target:05}.rs"),
                        BTreeSet::from(["rust-use".to_owned()]),
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

#[test]
fn frozen_graph_correctness_receipt_is_stable() {
    let graph = frozen_graph(500, 3);
    assert_eq!(
        reachable_files(&graph, &["src/file-00000.rs".to_owned()], false).len(),
        500
    );
    assert_eq!(strongly_connected_components(&graph, false).len(), 500);
    let path = shortest_file_path(&graph, "src/file-00000.rs", "src/file-00499.rs");
    assert_eq!(path["found"], true);

    let mut builder = CodeGraphBuilder::new("/fixture", 1);
    for index in (0..500).rev() {
        builder
            .add_file(format!("src/file-{index:05}.rs"), format!("digest-{index}"))
            .expect("file");
    }
    for (source, node) in &graph {
        for target in node.edges.keys() {
            builder
                .add_file_relation(source, target, "rust-use", 1)
                .expect("relation");
        }
    }
    let (snapshot, receipt) = builder.finish_with_receipt();
    assert_eq!(snapshot.snapshot.digest, receipt.snapshot_digest);
    assert_eq!(receipt.metrics.files, 500);
    assert_eq!(receipt.metrics.edges, 1_494);
    assert_eq!(receipt.metrics.ast_relations, 1_494);
}

/// Manual, repeatable sensor rather than a CI timing gate. Run with:
/// `cargo test --test graph_benchmark measure_frozen_graph_baseline -- --ignored --nocapture`.
/// The correctness test above remains the non-flaky merge guard.
#[test]
#[ignore = "manual performance sensor"]
fn measure_frozen_graph_baseline() {
    let files = std::env::var("OCTOCODE_GRAPH_BENCH_FILES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(20_000);
    let fanout = std::env::var("OCTOCODE_GRAPH_BENCH_FANOUT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4);

    let build_started = Instant::now();
    let graph = frozen_graph(files, fanout);
    let build_ms = build_started.elapsed().as_secs_f64() * 1_000.0;

    let snapshot_started = Instant::now();
    let mut builder = CodeGraphBuilder::new("/fixture", 1);
    for index in 0..files {
        builder
            .add_file(format!("src/file-{index:05}.rs"), format!("digest-{index}"))
            .expect("file");
    }
    for (source, node) in &graph {
        for target in node.edges.keys() {
            builder
                .add_file_relation(source, target, "rust-use", 1)
                .expect("relation");
        }
    }
    let (_, snapshot_receipt) = builder.finish_with_receipt();
    let snapshot_build_ms = snapshot_started.elapsed().as_secs_f64() * 1_000.0;

    let query_started = Instant::now();
    let reachable = reachable_files(&graph, &["src/file-00000.rs".to_owned()], false).len();
    let components = strongly_connected_components(&graph, false).len();
    let query_ms = query_started.elapsed().as_secs_f64() * 1_000.0;
    let estimated_edge_count: usize = graph.values().map(|node| node.edges.len()).sum();

    let report = format!(
        "{{\"implementation\":\"btree\",\"files\":{files},\"edges\":{estimated_edge_count},\"reachable\":{reachable},\"components\":{components},\"buildMs\":{build_ms:.3},\"snapshotBuildMs\":{snapshot_build_ms:.3},\"snapshotEdges\":{},\"queryMs\":{query_ms:.3}}}",
        snapshot_receipt.metrics.edges
    );
    let report_path = std::env::var_os("OCTOCODE_GRAPH_BENCH_REPORT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("octocode-graph-benchmark.json"));
    std::fs::write(report_path, report).expect("write benchmark receipt");
}
