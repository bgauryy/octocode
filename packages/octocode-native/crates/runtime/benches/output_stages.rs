//! Output-stage benchmark: what one response costs after its tool ran.
//!
//! On real localSearch and localFetch pages (this crate's own sources copied
//! into a fixture workspace) it times, in isolation:
//! - the output gate (`Sanitized` deserialization runs the same
//!   `ContentSecurity::seal_output` a tool result passes);
//! - output validation against the tool's generated output contract;
//! - serializing the envelope, as the size reference.
//!
//! Each line prints the median and the minimum of the samples, in µs; the
//! minimum is the steadier figure on a loaded machine.
//!
//! Run: `cargo bench -p octocode-native --bench output_stages`.
#![expect(clippy::expect_used, reason = "benchmark setup")]

#[expect(
    dead_code,
    unused_imports,
    reason = "shared integration-test fixture; the benchmark uses part of it"
)]
#[path = "../tests/support/mod.rs"]
mod support;

use std::path::Path;
use std::time::{Duration, Instant};

use octocode_native::contracts::validate_output;
use octocode_native::runtime::{Channel, ToolRuntime};
use octocode_native::security::Sanitized;
use serde::Deserialize;
use serde_json::{Value, json};
use support::Workspace;

const SAMPLES: usize = 400;

/// Copy every `.rs` file under `from` into the workspace at `to`.
fn copy_sources(workspace: &Workspace, from: &Path, to: &str) {
    for entry in std::fs::read_dir(from).expect("source dir") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        if path.is_dir() {
            copy_sources(workspace, &path, &format!("{to}/{name}"));
        } else if name.ends_with(".rs") {
            workspace.write(&format!("{to}/{name}"), std::fs::read(&path).expect("read"));
        }
    }
}

async fn call(runtime: &ToolRuntime, tool: &str, query: Value, length: u64) -> Value {
    let query = query;
    let admission = runtime.admit("bench".into()).expect("admitted");
    runtime
        .execute(
            admission,
            tool.into(),
            json!({"queries": [query], "responseLength": length}),
            Channel::Cli,
        )
        .await
        .expect("tool call")
        .structured_content
        .into_value()
}

fn time(label: &str, mut run: impl FnMut()) {
    for _ in 0..SAMPLES / 10 {
        run();
    }
    let mut samples: Vec<Duration> = (0..SAMPLES)
        .map(|_| {
            let started = Instant::now();
            run();
            started.elapsed()
        })
        .collect();
    samples.sort();
    let micros = |at: Duration| at.as_secs_f64() * 1e6;
    println!(
        "| {label} | {:.1} | {:.1} |",
        micros(samples[SAMPLES / 2]),
        micros(samples[0])
    );
}

fn stages(name: &str, tool: &str, page: &Value) {
    let chars = page.to_string().len();
    println!("\n{name}: {tool} page, {chars} bytes");
    println!("| stage | median µs | min µs |");
    println!("|---|---|---|");
    time("output gate", || {
        std::hint::black_box(Sanitized::deserialize(page).expect("sealed"));
    });
    time("validate output", || {
        validate_output(tool, page).expect("valid output");
    });
    time("serialize", || {
        std::hint::black_box(page.to_string());
    });
}

fn main() {
    let workspace = Workspace::new();
    let sources = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tools/local_search");
    copy_sources(&workspace, &sources, "src");
    let root = workspace
        .write("src/.keep", "")
        .parent()
        .expect("root")
        .to_owned();
    let root = root.to_string_lossy().into_owned();
    let read = format!("{root}/layout.rs");
    let tokio = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let (search, fetch) = tokio.block_on(async {
        let runtime = workspace.runtime(&[]);
        let search = call(
            &runtime,
            "localSearch",
            json!({"path": root, "matchString": "let "}),
            16_000,
        )
        .await;
        let fetch = call(
            &runtime,
            "localFetch",
            json!({"path": read, "fullContent": true}),
            50_000,
        )
        .await;
        runtime.close().await;
        (search, fetch)
    });
    stages("search", "localSearch", &search);
    stages("fetch", "localFetch", &fetch);
}
