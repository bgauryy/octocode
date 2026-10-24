//! Contract-declared evidence and exploration calls execute unchanged.
use crate::support::{self, Run as _, Workspace};
use octocode_native::{contracts, id::ToolId, response::continuations::visit_calls};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn calls(tool: &str, output: &Value) -> Vec<Value> {
    let tool = ToolId::from_name(tool).expect("tool");
    let mut calls = Vec::new();
    visit_calls(tool, output, &mut |call, _| calls.push(call.clone())).expect("declared call walk");
    for call in &calls {
        contracts::validate(
            call["tool"].as_str().expect("target"),
            call["input"].clone(),
        )
        .expect("complete public request");
        if call["kind"] == "continue" {
            support::assert_cursor_only(&call["input"]);
        } else {
            assert_eq!(call["kind"], "related");
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
            json!({"path":"src","matchString":["helper"]}),
        ),
        ("structureSearch", json!({"operation":"files","path":"src"})),
        (
            "astSearch",
            json!({"operation":"syntaxTree","path":"src/lib.rs"}),
        ),
        (
            "astSearch",
            json!({"operation":"match","path":"src/lib.rs","rule":{"pattern":"helper($A)"}}),
        ),
        (
            "astSearch",
            json!({"operation":"symbols","path":"src/lib.rs","symbolFilter":{"terms":["helper"]}}),
        ),
        ("localFetch", json!({"path":"src/lib.rs"})),
        (
            "localFetch",
            json!({"path":"src/lib.rs","matchString":["caller3"],"contextLines":{"before":1,"after":2}}),
        ),
        (
            "localFetch",
            json!({"path":"src/lib.rs","ranges":[{"startLine":5,"endLine":6}],"block":true}),
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
        for call in calls(tool, &output) {
            kinds.insert(call["kind"].as_str().unwrap().to_owned());
            all.push(call);
        }
    }
    assert!(
        kinds.contains("continue"),
        "required evidence continuation exercised"
    );
    assert!(kinds.contains("related"), "new exploration exercised");
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
