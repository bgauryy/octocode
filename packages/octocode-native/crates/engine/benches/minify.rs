//! Micro-benchmarks for the minify hot path.
//!
//! `apply_content_view_minification` is the compact view's entry point: for a
//! language with a grammar it parses the source and lays out the lines; CSS has
//! no grammar and only loses whitespace. This sweeps three JS sizes plus two
//! CSS sizes to track their costs independently.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use octocode_engine::minify::apply::apply_content_view_minification;

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

/// A realistic stylesheet with comments and repeated declarations.
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

/// A markdown document mixing headings, wrapped paragraphs, lists, tables and
/// code fences; `paragraph_lines` sets how long each wrapped paragraph runs
/// without a blank line, the shape that joins lines into one paragraph.
fn markdown_source(sections: usize, paragraph_lines: usize) -> String {
    let mut out = String::from("# Project guide\n\n");
    for i in 0..sections {
        out.push_str(&format!("## Section {i}\n\n"));
        for line in 0..paragraph_lines {
            out.push_str(&format!(
                "Line {line} of section {i} explains the `option_{i}` setting :rocket: in prose.\n"
            ));
        }
        out.push_str(&format!(
            "\n- item one for {i}\n- item two for {i}\n\n\
             | key | value |\n|---|---|\n| retries | {i} |\n\n\
             ```rust\nfn section_{i}() {{}}\n```\n\n---\n\n"
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
                        black_box(source.as_str()),
                        black_box("service.js"),
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
                        black_box(source.as_str()),
                        black_box("theme.css"),
                    ))
                });
            },
        );
    }
    group.finish();
}

fn bench_minify_markdown(c: &mut Criterion) {
    let mut group = c.benchmark_group("minify_content_view_markdown");
    for &(sections, paragraph_lines) in &[(20_usize, 4_usize), (4, 400)] {
        let source = markdown_source(sections, paragraph_lines);
        group.throughput(Throughput::Bytes(source.len() as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}B", source.len())),
            &source,
            |b, source| {
                b.iter(|| {
                    black_box(apply_content_view_minification(
                        black_box(source.as_str()),
                        black_box("guide.md"),
                    ))
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_minify_js,
    bench_minify_css,
    bench_minify_markdown
);
criterion_main!(benches);
