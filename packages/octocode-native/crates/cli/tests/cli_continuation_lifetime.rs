//! A command that exits cannot promise another call into its memory-only store.
use crate::support::Workspace;
use serde_json::{Value, json};

#[test]
fn a_one_shot_memory_call_refuses_unreachable_continuations() {
    let workspace = Workspace::new();
    workspace.write(
        "many.txt",
        (0..2_000)
            .map(|n| format!("evidence {n}\n"))
            .collect::<String>(),
    );
    let input = json!({"queries":[{"path":"many.txt"}],"responseChars":4000});
    let output = workspace
        .cli()
        .args(["localFetch", &input.to_string(), "--json"])
        .output()
        .expect("CLI exits");
    let value: Value = serde_json::from_slice(&output.stdout).expect("JSON error");
    assert!(
        !output.status.success(),
        "unreachable continuation: {value}"
    );
    assert_eq!(value["errorCode"], "executionFailed", "{value}");
    let message = value["error"].as_str().expect("actionable error");
    assert!(
        message.contains("persistent") && message.contains("MCP"),
        "{message}"
    );
    assert!(
        !value.to_string().contains("cursor2."),
        "dead cursor: {value}"
    );
}

#[test]
fn a_complete_one_shot_memory_answer_still_succeeds() {
    let workspace = Workspace::new();
    workspace.write("small.txt", "complete evidence\n");
    let input = json!({"queries":[{"path":"small.txt"}]});
    let output = workspace
        .cli()
        .args(["localFetch", &input.to_string(), "--json"])
        .output()
        .expect("CLI exits");
    let value: Value = serde_json::from_slice(&output.stdout).expect("JSON result");
    assert!(output.status.success(), "{value}");
    assert!(
        value["results"][0]["data"]["content"]
            .as_str()
            .expect("content")
            .contains("complete evidence")
    );
}
