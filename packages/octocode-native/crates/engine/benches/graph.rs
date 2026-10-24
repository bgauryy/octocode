//! Cycle-engine cost on the frozen benchmark graph
//! (`OCTOCODE_GRAPH_BENCH_FILES` files, default 20 000, each using the next
//! `OCTOCODE_GRAPH_BENCH_FANOUT`, default 4). Correctness is
//! `tests/graph_benchmark.rs`; this only times it.

use criterion::{Criterion, criterion_group, criterion_main};
use octocode_engine::graph::{condense, scc};
use std::hint::black_box;

#[path = "support/frozen_graph.rs"]
mod frozen;
use frozen::frozen_graph;

fn setting(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn graph(c: &mut Criterion) {
    let files = setting("OCTOCODE_GRAPH_BENCH_FILES", 20_000);
    let fanout = setting("OCTOCODE_GRAPH_BENCH_FANOUT", 4);
    let graph = frozen_graph(files, fanout);
    let mut group = c.benchmark_group(format!("graph_{files}x{fanout}"));
    group.sample_size(10);
    group.bench_function("build", |b| {
        b.iter(|| frozen_graph(black_box(files), fanout));
    });
    group.bench_function("scc_and_condense", |b| {
        b.iter(|| (scc(&graph, true).len(), condense(&graph).edges.len()));
    });
    group.finish();
}

criterion_group!(benches, graph);
criterion_main!(benches);
