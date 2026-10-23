//! Page-replay benchmark for the runtime's `PageReplayMemo` (B59).
//!
//! For each workload (the largest paged envelopes the local tools produce) it
//! measures, on one warm runtime:
//! - page 1: execution plus storing the replay seed;
//! - a replayed page: a continuation served from the memo;
//! - a re-executed page: the same continuation after eight other paged calls
//!   evicted the entry (`MAX_ENTRIES` = 8), which is what every page costs
//!   when the entry is over the byte cap.
//!
//! The byte cap only decides whether an entry is kept (entry bytes <= cap)
//! and how many fit (at most 8, at most cap bytes in total), so the table at
//! the end maps each candidate cap to the page latency and retained bytes the
//! measured workloads would see.
//!
//! Run: `cargo bench -p octocode-native --bench page_replay`.
//!
//! Profile the replay path alone: `PAGE_REPLAY_PROFILE=<n>` serves the same
//! continuation `n` times per workload and skips the other measurements, e.g.
//! `PAGE_REPLAY_PROFILE=300 samply record <bench binary built with
//! --profile profiling>`.
#![allow(clippy::expect_used, reason = "benchmark setup")]

#[expect(
    dead_code,
    unused_imports,
    reason = "shared integration-test fixture; the benchmark uses part of it"
)]
#[path = "../tests/support/mod.rs"]
mod support;

use std::time::{Duration, Instant};

use octocode_native::runtime::{Channel, ToolRuntime};
use serde_json::{Value, json};
use support::Workspace;

const ITERATIONS: usize = 25;
/// Small enough that every workload pages.
const RESPONSE_LENGTH: u64 = 4_000;
const MIB: usize = 1024 * 1024;
const CAPS: [usize; 8] = [
    256 * 1024,
    MIB,
    4 * MIB,
    8 * MIB,
    16 * MIB,
    32 * MIB,
    64 * MIB,
    128 * MIB,
];
const MAX_ENTRIES: usize = 8;

struct Workload {
    name: &'static str,
    tool: &'static str,
    queries: Vec<Value>,
}

struct Measured {
    name: &'static str,
    /// Characters of the whole paged response (the stored envelope's size).
    total_chars: usize,
    page_one: Vec<Duration>,
    replayed: Vec<Duration>,
    re_executed: Vec<Duration>,
}

fn row(fields: Value) -> Value {
    let mut row = fields;
    row["mainGoal"] = json!("Benchmark page replay.");
    row["reasoning"] = json!("Measure replayed against re-executed pages.");
    row
}

fn workloads(root: &str, wide: &str) -> Vec<Workload> {
    vec![
        // A single read fits its share of the page (its own
        // `continueWindow` pages the file), so a read never pages alone.
        Workload {
            name: "localFetch 5 rows",
            tool: "localFetch",
            queries: (0..5)
                .map(|i| row(json!({"path": wide, "offset": i * 3_000, "length": 50_000})))
                .collect(),
        },
        Workload {
            name: "localSearch 5x1000",
            tool: "localSearch",
            queries: (0..5)
                .map(|i| {
                    row(json!({"path": root, "matchString": format!("needle {i}"),
                        "pageSize": 1000}))
                })
                .collect(),
        },
    ]
}

async fn call(runtime: &ToolRuntime, tool: &str, envelope: Value) -> (Value, Duration) {
    let started = Instant::now();
    let admission = runtime.admit("bench".into()).expect("admitted");
    let outcome = runtime
        .execute(admission, tool.into(), envelope, Channel::Cli)
        .await
        .expect("tool call");
    (outcome.structured_content.into_value(), started.elapsed())
}

/// Eight distinct paged calls: each stores a replay entry, so together they
/// evict every older entry.
async fn evict(runtime: &ToolRuntime, filler: &str) {
    for i in 0..MAX_ENTRIES {
        let (structured, _) = call(
            runtime,
            "localFetch",
            json!({"queries": [
                    row(json!({"path": filler, "matchString": format!("line {i}")})),
                    row(json!({"path": filler, "matchString": format!("filler {i}")})),
                ], "responseLength": 50}),
        )
        .await;
        assert_eq!(
            structured["responsePagination"]["hasMore"], true,
            "filler call must page to be stored"
        );
    }
}

async fn measure(runtime: &ToolRuntime, workload: &Workload, filler: &str) -> Measured {
    let first = json!({"queries": workload.queries, "responseLength": RESPONSE_LENGTH});
    let mut measured = Measured {
        name: workload.name,
        total_chars: 0,
        page_one: Vec::new(),
        replayed: Vec::new(),
        re_executed: Vec::new(),
    };
    for _ in 0..ITERATIONS {
        let (page_one, elapsed) = call(runtime, workload.tool, first.clone()).await;
        measured.page_one.push(elapsed);
        let pagination = &page_one["responsePagination"];
        assert_eq!(pagination["hasMore"], true, "{} must page", workload.name);
        measured.total_chars = pagination["totalChars"]
            .as_u64()
            .and_then(|chars| usize::try_from(chars).ok())
            .expect("totalChars");
        let next = pagination["next"]["query"].clone();

        let (replayed, elapsed) = call(runtime, workload.tool, next.clone()).await;
        assert_ne!(replayed["responsePagination"]["restart"], true);
        measured.replayed.push(elapsed);

        evict(runtime, filler).await;
        let (re_executed, elapsed) = call(runtime, workload.tool, next).await;
        assert_ne!(re_executed["responsePagination"]["restart"], true);
        assert_eq!(
            response_of(&re_executed["responsePagination"]),
            response_of(pagination),
            "re-execution reproduces the same response"
        );
        measured.re_executed.push(elapsed);
    }
    measured
}

/// Replay-only loop for a sampling profiler: one page 1, then `repeats`
/// continuations served from the memo.
async fn replay_only(runtime: &ToolRuntime, workload: &Workload, repeats: usize) {
    let first = json!({"queries": workload.queries, "responseLength": RESPONSE_LENGTH});
    let (page_one, _) = call(runtime, workload.tool, first).await;
    let next = page_one["responsePagination"]["next"]["query"].clone();
    for _ in 0..repeats {
        let (replayed, _) = call(runtime, workload.tool, next.clone()).await;
        assert_ne!(replayed["responsePagination"]["restart"], true);
    }
}

fn median(samples: &[Duration]) -> f64 {
    let mut ms: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1e3).collect();
    ms.sort_by(f64::total_cmp);
    ms[ms.len() / 2]
}

fn mib(bytes: usize) -> String {
    if bytes >= MIB {
        format!("{} MiB", bytes / MIB)
    } else {
        format!("{} KiB", bytes / 1024)
    }
}

/// Source files backdated past the replay freshness window.
fn settle(paths: &[&std::path::Path]) {
    let then = std::time::SystemTime::now() - Duration::from_secs(10);
    for path in paths {
        std::fs::File::open(path)
            .expect("open fixture")
            .set_modified(then)
            .expect("backdate fixture");
    }
}

fn main() {
    let workspace = Workspace::new();
    let wide: String = (0..20_000)
        .map(|i| format!("needle {} {}\n", i % 5, "x".repeat(400)))
        .collect();
    let wide_path = workspace.write("data/wide.txt", wide);
    let filler: String = (0..400)
        .map(|i| format!("line {i} filler text\n"))
        .collect();
    let filler_path = workspace.write("filler/lines.txt", filler);
    settle(&[&wide_path, &filler_path]);
    let root = wide_path
        .parent()
        .expect("data dir")
        .to_string_lossy()
        .into_owned();
    let wide = wide_path.to_string_lossy().into_owned();
    let filler = filler_path.to_string_lossy().into_owned();

    let tokio = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Some(repeats) = std::env::var("PAGE_REPLAY_PROFILE")
        .ok()
        .and_then(|value| value.parse().ok())
    {
        tokio.block_on(async {
            let runtime = workspace.runtime(&[]);
            for workload in workloads(&root, &wide) {
                replay_only(&runtime, &workload, repeats).await;
            }
            runtime.close().await;
        });
        return;
    }
    let results: Vec<Measured> = tokio.block_on(async {
        let runtime = workspace.runtime(&[]);
        let mut results = Vec::new();
        for workload in workloads(&root, &wide) {
            results.push(measure(&runtime, &workload, &filler).await);
        }
        runtime.close().await;
        results
    });

    println!("page replay, median of {ITERATIONS} (ms)");
    println!(
        "| workload | envelope chars | page 1 | replayed page | re-executed page | replay saves |"
    );
    println!("|---|---|---|---|---|---|");
    for m in &results {
        let (hit, miss) = (median(&m.replayed), median(&m.re_executed));
        println!(
            "| {} | {} | {:.3} | {:.3} | {:.3} | {:.1}% |",
            m.name,
            m.total_chars,
            median(&m.page_one),
            hit,
            miss,
            100.0 * (1.0 - hit / miss)
        );
    }

    println!();
    println!(
        "cap model: per workload, entries kept (of {MAX_ENTRIES}) / retained bytes / later-page ms"
    );
    print!("| cap |");
    for m in &results {
        print!(" {} |", m.name);
    }
    println!();
    println!("|---|{}", "---|".repeat(results.len()));
    for cap in CAPS {
        print!("| {} |", mib(cap));
        for m in &results {
            let entry = m.total_chars.max(1);
            let kept = if entry > cap {
                0
            } else {
                MAX_ENTRIES.min(cap / entry)
            };
            let latency = if kept == 0 {
                median(&m.re_executed)
            } else {
                median(&m.replayed)
            };
            print!(" {kept} / {} / {latency:.3} |", mib(kept * entry));
        }
        println!();
    }
}

/// The saved response a page's cursor names (key, content, scope, page
/// length: every byte but the offset): `next` carries the cursor while more
/// is left; the last page names its own in the block.
fn response_of(pagination: &Value) -> Vec<u8> {
    use base64::Engine as _;
    let cursor = match &pagination["next"]["query"]["responseSnapshot"] {
        Value::Null => &pagination["snapshot"],
        cursor => cursor,
    };
    let encoded = cursor
        .as_str()
        .and_then(|cursor| cursor.strip_prefix("saved-v2:"))
        .expect("a cursor");
    let mut bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded)
        .expect("base64url cursor");
    bytes.truncate(73);
    bytes
}
