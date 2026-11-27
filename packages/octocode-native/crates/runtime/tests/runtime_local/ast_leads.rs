//! astSearch leads through the response stage: a lead whose recovery the
//! output reference states once, by its note's code, reaches the caller
//! beside the matches it qualifies.
use crate::support::{Workspace, run};
use serde_json::{Value, json};

/// Two bare calls and one on a receiver: a bare-call pattern matches two,
/// and its receiver note counts the third.
const CALLS_TS: &str = "function f(client: C) {\n  runTask(\"inside\");\n  client.runTask(\"receiver\");\n}\nrunTask(\"outside\");\n";

/// A page with matches keeps the related rule with `any` that its
/// `structural.pattern.qualified` note promises, though the note carries no
/// recovery of its own; the lead lists every spelling.
#[tokio::test]
async fn a_receiver_note_beside_matches_keeps_its_any_rule_lead() {
    let workspace = Workspace::new();
    let file = workspace.write("src/calls.ts", CALLS_TS);
    let runtime = workspace.runtime(&[]);
    let row = run(
        &runtime,
        "astSearch",
        json!({"path": file, "operation": "match", "rule": {"pattern": "runTask($$$A)"}}),
    )
    .await;
    let data = row.data();
    assert_eq!(data["matchCount"], 2, "{data}");
    let note = data["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|diagnostic| diagnostic["code"] == "structural.pattern.qualified")
        .unwrap_or_else(|| panic!("the receiver note: {data}"));
    assert!(note.get("recovery").is_none(), "{data}");
    let spellings = |call: &Value| {
        call["input"]["queries"][0]
            .pointer("/rule/any")
            .and_then(Value::as_array)
            .map(|any| {
                any.iter()
                    .filter_map(|rule| rule["pattern"].as_str())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    let lead = data["related"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|call| call["tool"] == "astSearch" && !spellings(call).is_empty())
        .unwrap_or_else(|| panic!("the `any` lead: {data}"));
    assert!(
        spellings(lead).contains(&"$QUALIFIER.runTask($$$A)".to_owned()),
        "{lead}"
    );
    let listed = run(&runtime, "astSearch", lead["input"].clone()).await;
    assert_eq!(listed.data()["matchCount"], 3, "{}", listed.data());
    runtime.close().await;
}
