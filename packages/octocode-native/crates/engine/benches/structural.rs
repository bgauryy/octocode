#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Micro-benchmarks for the structural (AST) search hot path.
//!
//! `structuralSearchDetailed` is the synchronous public entry point that drives
//! the whole matcher pipeline for one file: tree-sitter parse of the source +
//! pattern compilation + `match_multi_capture` walk over every node. Parse time
//! dominates on small inputs, so we sweep three realistic TypeScript sizes to
//! separate fixed parse cost from per-node match cost. A regression inside the
//! matcher core is invisible to the Node/CLI benchmarks (JS+FFI noise) — this is
//! the only place it shows up as pure-Rust latency.

use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use octocode_engine::portable::{extract_declarations, structural_search_detailed};

/// Build a realistic TypeScript module of roughly `blocks` service methods,
/// each with a `console.log(...)` call site the pattern will match, plus enough
/// surrounding syntax (types, control flow, awaits) to give the parser real
/// work rather than a trivial one-liner.
fn typescript_source(blocks: usize) -> String {
    let mut out = String::from(
        "import { Logger } from './logger';\nimport type { User, Session } from './types';\n\n",
    );
    for i in 0..blocks {
        out.push_str(&format!(
            "export async function handleRequest{i}(user: User, session: Session): Promise<number> {{\n  \
             const items = user.items.filter((it) => it.active && it.count > {i});\n  \
             let total = 0;\n  \
             for (const item of items) {{\n    \
             total += item.count * {i};\n    \
             console.log(`processed ${{item.id}} for user ${{user.id}}`);\n  \
             }}\n  \
             if (total > {i}) {{\n    \
             console.log('threshold exceeded', total);\n  \
             }}\n  \
             return total;\n\
             }}\n\n"
        ));
    }
    out
}

fn bench_structural_search(c: &mut Criterion) {
    let pattern = "console.log($MSG)";
    let mut group = c.benchmark_group("structural_search_ts");
    for &blocks in &[8_usize, 60, 240] {
        let source = typescript_source(blocks);
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}B", source.len())),
            &source,
            |b, source| {
                b.iter(|| {
                    let result = structural_search_detailed(
                        black_box(source.as_str()),
                        black_box("service.ts"),
                        black_box(Some(pattern)),
                        black_box(None),
                    )
                    .expect("structural search should not panic on valid TS");
                    black_box(result.matches.len())
                });
            },
        );
    }
    group.finish();
}

fn bench_exported_declarations(c: &mut Criterion) {
    let mut group = c.benchmark_group("declaration_exports");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(2));
    for count in [1_000, 20_000] {
        let source = (0..count)
            .map(|i| format!("export function fn{i}(value: number) {{ return value + {i}; }}\n"))
            .collect::<String>();
        group.throughput(Throughput::Elements(count));
        group.bench_with_input(BenchmarkId::from_parameter(count), &source, |b, source| {
            b.iter(|| {
                let result = extract_declarations(black_box(source), "exports.ts")
                    .expect("valid exported declarations");
                let facts: serde_json::Value = serde_json::from_str(&result).expect("facts");
                assert_eq!(
                    facts["declarations"].as_array().unwrap().len(),
                    count as usize
                );
                black_box(result)
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_structural_search,
    bench_exported_declarations
);
criterion_main!(benches);
