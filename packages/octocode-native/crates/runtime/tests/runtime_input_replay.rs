#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Replays localFetch rows agents sent verbatim that the schema once rejected
//! (`fixtures/benchmark-localfetch-ranges.json`, each with the rejection it
//! got). Every row must now run: host spellings of line ranges normalize
//! losslessly before validation. The MCP host wraps a bare row in `queries`
//! before it calls the runtime, so the replay does too.

mod support;

use serde_json::{Value, json};
use support::Workspace;

const FIXTURE: &str = include_str!("fixtures/benchmark-localfetch-ranges.json");

/// Enough numbered lines to cover every requested range.
fn source() -> String {
    (1..=2_200).map(|line| format!("line {line}\n")).collect()
}

#[tokio::test]
async fn rejected_benchmark_range_spellings_now_read() {
    let rows: Vec<Value> = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(rows.len(), 14);
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (index, row) in rows.iter().enumerate() {
        let mut input = row["input"].clone();
        let relative = input["path"].as_str().unwrap().to_owned();
        let file = workspace.write(&relative, source());
        input["path"] = json!(file.to_string_lossy());
        let output = runtime
            .execute_mcp(
                format!("replay-{index}"),
                "localFetch".into(),
                json!({ "queries": [input.clone()] }),
            )
            .await
            .expect("executes");
        assert_eq!(
            output["isError"], false,
            "row {index} {input} (was: {}) -> {output}",
            row["rejectedWith"]
        );
        let first = input["ranges"]
            .to_string()
            .split(|c: char| !c.is_ascii_digit())
            .find(|part| !part.is_empty())
            .unwrap()
            .to_owned();
        assert!(
            output.to_string().contains(&format!("line {first}")),
            "row {index} reads its first range: {output}"
        );
    }
    runtime.close().await;
}
