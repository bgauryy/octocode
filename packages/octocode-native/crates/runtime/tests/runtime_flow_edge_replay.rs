//! Contract-declared evidence and exploration calls execute unchanged.
use crate::support::{self, Run as _, Workspace};
use octocode_runtime_base::{contracts, id::ToolId, response::continuations::visit_calls};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Each served call of `output` with its channel: `related` for a lead,
/// `continue` for any other. A served call states no `kind`; where it sits
/// says what it does.
fn calls(tool: &str, output: &Value) -> Vec<(&'static str, Value)> {
    let tool = ToolId::from_name(tool).expect("tool");
    let mut calls = Vec::new();
    visit_calls(tool, output, &mut |call, path| {
        let channel = if path.iter().any(|key| key == "related") {
            "related"
        } else {
            "continue"
        };
        calls.push((channel, call.clone()));
    })
    .expect("declared call walk");
    for (channel, call) in &calls {
        assert!(
            call.get("kind").is_none(),
            "a served call states no kind: {call}"
        );
        contracts::validate(
            call["tool"].as_str().expect("target"),
            call["input"].clone(),
        )
        .expect("complete public request");
        if *channel == "continue" {
            support::assert_cursor_only(&call["input"]);
        } else {
            assert!(call["input"]["queries"].is_array());
        }
    }
    calls
}

fn checked(tool: &str, output: &Value) {
    support::checks::assert_output_contract(tool, output);
    for row in output["results"].as_array().into_iter().flatten() {
        assert!(
            !matches!(
                row["data"]["errorCode"].as_str(),
                Some("invalidInput" | "outputContractViolation")
            ),
            "{tool}: {row}"
        );
    }
}

#[tokio::test]
async fn emitted_local_calls_are_strict_and_replay_without_input_rewriting() {
    let workspace = Workspace::new();
    let source = (0..40)
        .map(|index| format!("pub fn caller{index}() -> u32 {{\n    helper({index})\n}}\n\n"))
        .collect::<String>();
    workspace.write(
        "src/lib.rs",
        format!("pub fn helper(value:u32)->u32 {{value+1}}\n{source}"),
    );
    for index in 0..12 {
        workspace.write(&format!("src/mod{index}.rs"), "pub fn helper_twin() {}\n");
    }
    let runtime = workspace.runtime(&[]);
    let seeds = [
        (
            "localSearch",
            json_fixture(r#"{"path": "src", "matchString": ["helper"]}"#),
        ),
        (
            "structureSearch",
            json_fixture(r#"{"operation": "files", "path": "src"}"#),
        ),
        (
            "structureSearch",
            json_fixture(r#"{"operation": "tree", "path": "src/lib.rs"}"#),
        ),
        (
            "astSearch",
            json_fixture(r#"{"operation": "syntaxTree", "path": "src/lib.rs"}"#),
        ),
        (
            "astSearch",
            json_fixture(
                r#"{
                    "operation": "match",
                    "path": "src/lib.rs",
                    "rule": {"pattern": "helper($A)"}
                }"#,
            ),
        ),
        (
            "astSearch",
            json_fixture(
                r#"{
                    "operation": "symbols",
                    "path": "src/lib.rs",
                    "symbolFilter": {"terms": ["helper"]}
                }"#,
            ),
        ),
        ("localFetch", json_fixture(r#"{"path": "src/lib.rs"}"#)),
        (
            "localFetch",
            json_fixture(
                r#"{
                    "path": "src/lib.rs",
                    "matchString": ["caller3"],
                    "contextLines": {"before": 1, "after": 2}
                }"#,
            ),
        ),
        (
            "localFetch",
            json_fixture(
                r#"{
                    "path": "src/lib.rs",
                    "ranges": [{"startLine": 5, "endLine": 6}],
                    "block": true
                }"#,
            ),
        ),
    ];
    let mut all = Vec::new();
    let mut kinds = BTreeSet::new();
    for (tool, row) in seeds {
        let output = support::run(
            &runtime,
            tool,
            json!({"queries":[row],"responseChars":1800}),
        )
        .await
        .structured_content
        .clone();
        checked(tool, &output);
        for (channel, call) in calls(tool, &output) {
            kinds.insert(channel);
            all.push(call);
        }
    }
    assert!(
        kinds.contains("continue"),
        "required evidence continuation exercised"
    );
    assert!(kinds.contains("related"), "empty/error recovery exercised");
    let mut seen = BTreeSet::new();
    for call in all {
        if !seen.insert(call.to_string()) {
            continue;
        }
        let tool = call["tool"].as_str().unwrap();
        if tool == "clasify" || tool == "lspSearch" {
            continue;
        }
        let output = runtime
            .run("verbatim".into(), tool.into(), call["input"].clone())
            .await
            .expect("unchanged public call")
            .structured_content;
        checked(tool, &output);
        calls(tool, &output);
    }
    runtime.close().await;
}
