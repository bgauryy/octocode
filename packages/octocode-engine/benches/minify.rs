//! Micro-benchmarks for the minify hot path.
//!
//! `applyContentViewMinification` is the agent-readable minify entry point: for
//! JS/TS it runs the OXC parse → semantic → codegen pipeline (the expensive
//! path), and for CSS it runs lightningcss. Both are pure-Rust, CPU-bound, and
//! called on nearly every file the tools surface, so a regression in the OXC or
//! lightningcss glue is a real latency hit. We sweep three JS sizes (OXC path)
//! plus one CSS input (lightningcss path) so the two engines are tracked apart.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use octocode_engine::apply_content_view_minification;

/// A realistic JS module: comments (stripped), imports, classes, control flow —
/// the kind of content OXC actually has to parse and regenerate, not a
/// single expression that would hide parse cost.
fn javascript_source(blocks: usize) -> String {
    let mut out = String::from(
        "// top-of-file banner comment that the content view should strip\n\
         import { EventEmitter } from 'events';\n\
         import { readFile } from 'fs/promises';\n\n",
    );
    for i in 0..blocks {
        out.push_str(&format!(
            "/**\n * Service number {i} — jsdoc that content-view minify removes.\n */\n\
             export class Service{i} extends EventEmitter {{\n  \
             constructor(config) {{\n    \
             super();\n    \
             this.config = config || {{ retries: {i}, timeout: 1000 }};\n    \
             this.cache = new Map();\n  \
             }}\n\n  \
             async process(payload) {{\n    \
             // inline comment describing the work\n    \
             const key = `${{payload.id}}-{i}`;\n    \
             if (this.cache.has(key)) {{\n      \
             return this.cache.get(key);\n    \
             }}\n    \
             const data = await readFile(payload.path, 'utf8');\n    \
             const result = data.split('\\n').filter((line) => line.length > {i});\n    \
             this.cache.set(key, result);\n    \
             return result;\n  \
             }}\n\
             }}\n\n"
        ));
    }
    out
}

/// A realistic stylesheet with comments, zero-unit values, and redundant
/// declarations that lightningcss compresses.
fn css_source(blocks: usize) -> String {
    let mut out = String::from("/* design tokens */\n:root { --gap: 8px; --fg: #333333; }\n\n");
    for i in 0..blocks {
        out.push_str(&format!(
            ".card-{i} {{\n  \
             display: flex;\n  \
             margin: 0px;\n  \
             padding: 0px 0px;\n  \
             color: #333333;\n  \
             border: 1px solid rgba(0, 0, 0, 0.1);\n  \
             background-color: #ffffff;\n\
             }}\n\
             .card-{i} .title {{ font-weight: bold; font-size: 1.0em; }}\n\n"
        ));
    }
    out
}

fn bench_minify_js(c: &mut Criterion) {
    let mut group = c.benchmark_group("minify_content_view_js");
    for &blocks in &[6_usize, 40, 160] {
        let source = javascript_source(blocks);
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}B", source.len())),
            &source,
            |b, source| {
                b.iter(|| {
                    black_box(apply_content_view_minification(
                        black_box(source.clone()),
                        black_box("service.js".to_owned()),
                    ))
                });
            },
        );
    }
    group.finish();
}

fn bench_minify_css(c: &mut Criterion) {
    let mut group = c.benchmark_group("minify_content_view_css");
    for &blocks in &[10_usize, 120] {
        let source = css_source(blocks);
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}B", source.len())),
            &source,
            |b, source| {
                b.iter(|| {
                    black_box(apply_content_view_minification(
                        black_box(source.clone()),
                        black_box("theme.css".to_owned()),
                    ))
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_minify_js, bench_minify_css);
criterion_main!(benches);
