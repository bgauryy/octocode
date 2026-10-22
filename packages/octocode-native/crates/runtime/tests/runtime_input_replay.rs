//! Replays localFetch rows agents sent verbatim that the canonical schema
//! rejects (`fixtures/benchmark-localfetch-ranges.json`, each with the
//! rejection it got). Admission accepts exactly the canonical language, so
//! every row is still rejected, on MCP and the CLI alike, with the repair
//! that names the canonical spelling (`start-end` items in a JSON array). Each row is replayed bare,
//! as the agents sent it: native runs a bare row as a one-row `queries`.

use crate::support::{self, Run as _};
use octocode_contract_types::ErrorCode;

use serde_json::{Value, json};
use support::Workspace;

const FIXTURE: &str = include_str!("fixtures/benchmark-localfetch-ranges.json");

#[tokio::test]
async fn benchmark_range_spellings_are_rejected_with_the_canonical_repair() {
    let rows: Vec<Value> = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(rows.len(), 14);
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (index, row) in rows.iter().enumerate() {
        let mut input = row["input"].clone();
        let relative = input["path"].as_str().unwrap().to_owned();
        let file = workspace.write(&relative, "line 1\n");
        input["path"] = json!(file.to_string_lossy());
        let output = runtime
            .run_mcp(
                format!("replay-{index}"),
                "localFetch".into(),
                input.clone(),
            )
            .await
            .expect("MCP answers the rejection in-band");
        assert_eq!(output["isError"], true, "row {index} {input} -> {output}");
        assert_eq!(
            output["structuredContent"]["errorCode"], "invalidInput",
            "row {index}: {output}"
        );
        let text = output.to_string();
        assert!(
            text.contains("start-end") || text.contains("send a JSON array"),
            "row {index} names the canonical spelling: {output}"
        );
        let cli = runtime
            .run(
                format!("replay-cli-{index}"),
                "localFetch".into(),
                input.clone(),
            )
            .await
            .expect_err("the CLI path rejects the same row");
        assert_eq!(cli.code, ErrorCode::InvalidInput, "row {index}");
    }
    runtime.close().await;
}
