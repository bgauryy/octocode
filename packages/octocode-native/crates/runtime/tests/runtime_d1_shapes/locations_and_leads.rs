//! Location rows named like tool inputs, runnable recovery leads, and one camelCase error-code set.

use crate::support::{self, Workspace};
use octocode_native::response::outcome::ToolOutcome;
use serde_json::{Value, json};

// ---------- location rows are objects named like tool inputs ----------

#[tokio::test]
async fn location_rows_are_objects_named_like_tool_inputs() {
    let workspace = Workspace::new();
    workspace.write(
        "pkg/src/lib.rs",
        "pub fn run() {\n    let a = 1;\n    let b = a + 1;\n}\n",
    );
    workspace.write("pkg/a.txt", "x\n");
    let runtime = workspace.runtime(&[]);
    // The enclosing declaration is an object an lspSearch/localFetch
    // call takes as is.
    let hits = support::run(
        &runtime,
        "localSearch",
        json!({"path":"pkg","matchString":"let"}),
        &[],
    )
    .await;
    let first = &hits.data()["files"][0]["matches"][0];
    assert_eq!(
        first["enclosing"],
        json!({"symbolName":"run","kind":"function","line":1,"endLine":4}),
        "{}",
        hits.structured_content
    );
    assert!(first.get("in").is_none(), "{first}");
    // A declaration without members is an object too.
    let outline = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"symbols","path":"pkg"}),
        &[],
    )
    .await;
    assert_eq!(
        outline.data()["files"][0]["symbols"][0],
        json!({"symbolName":"run","kind":"function","line":1,"endLine":4,"exported":true}),
        "{}",
        outline.structured_content
    );
    // Match rows are objects with their 1-based column: two matches can
    // start on one line.
    let matched = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"pkg","language":"rust","pattern":"let $A = $B;"}),
        &[],
    )
    .await;
    assert_eq!(
        matched.data()["files"][0]["matches"][0],
        json!({"line":2,"column":5,"value":"let a = 1;"}),
        "{}",
        matched.structured_content
    );
    // One listing shape, workspace-relative groups, key `files`.
    let tree = support::run(&runtime, "structureSearch", json!({"path":"pkg"}), &[]).await;
    assert!(
        tree.data().get("entries").is_none(),
        "{}",
        tree.structured_content
    );
    assert_eq!(
        tree.data()["files"],
        json!([{"dir":"pkg","files":["a.txt (2)","src/"]}]),
        "{}",
        tree.structured_content
    );
    let files = support::run(
        &runtime,
        "structureSearch",
        json!({"path":"pkg","operation":"files"}),
        &[],
    )
    .await;
    assert_eq!(
        files.data()["files"],
        json!([{"dir":"pkg","files":["a.txt (2)"]},{"dir":"pkg/src","files":["lib.rs (51)"]}]),
        "{}",
        files.structured_content
    );
    runtime.close().await;
}

// ---------- lead names and runnable recoveries ----------

/// The lead `name` under `next` or `hints`, whichever channel holds it.
fn lead(outcome: &ToolOutcome, name: &str) -> Value {
    let data = support::row_data(outcome);
    [
        data["next"][name].clone(),
        support::checks::hint(data, name),
    ]
    .into_iter()
    .find(|lead| lead.is_object())
    .unwrap_or_else(|| panic!("no `{name}` lead: {}", outcome.structured_content))
}

#[tokio::test]
async fn a_case_sensitive_miss_leads_to_a_runnable_ignore_case_read() {
    let workspace = Workspace::new();
    workspace.write("pkg/a.txt", "alpha\nHello World\n");
    let runtime = workspace.runtime(&[]);
    let miss = support::run(
        &runtime,
        "localFetch",
        json!({"path":"pkg/a.txt","matchString":"hello World"}),
        &[],
    )
    .await;
    let hint = support::checks::hint(miss.data(), "text")[0]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!hint.contains("caseMode"), "{hint}");
    let ignore = lead(&miss, "ignoreCase");
    assert_eq!(ignore["tool"], "localFetch", "{ignore}");
    let row = ignore["query"]["queries"][0].clone();
    assert_eq!(row["caseMode"], "insensitive", "{row}");
    let hit = support::run(&runtime, "localFetch", row, &[]).await;
    assert!(
        hit.data()["content"]
            .as_str()
            .is_some_and(|text| text.contains("Hello World")),
        "{}",
        hit.structured_content
    );
    // A miss that already ignored case offers no ignoreCase lead.
    let insensitive = support::run(
        &runtime,
        "localFetch",
        json!({"path":"pkg/a.txt","matchString":"absent","caseMode":"insensitive"}),
        &[],
    )
    .await;
    assert!(
        insensitive.data()["next"].get("ignoreCase").is_none()
            && support::checks::hint(insensitive.data(), "ignoreCase").is_null(),
        "{}",
        insensitive.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn a_missing_file_leads_to_find_file() {
    let workspace = Workspace::new();
    workspace.write("pkg/server.rs", "fn main() {}\n");
    let runtime = workspace.runtime(&[]);
    let missing = support::run(&runtime, "localFetch", json!({"path":"pkg/server.ts"}), &[]).await;
    let find = lead(&missing, "findFile");
    assert_eq!(find["tool"], "structureSearch", "{find}");
    runtime.close().await;
}

// ---------- one camelCase error-code set ----------

#[tokio::test]
async fn error_codes_are_declared_camel_case_names() {
    let workspace = Workspace::new();
    workspace.write("pkg/a.rs", "fn a() {}\n");
    let runtime = workspace.runtime(&[]);
    let unsupported = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"pkg/a.rs","language":"cobol","pattern":"fn $A() {}"}),
        &[],
    )
    .await;
    assert_eq!(
        unsupported.data()["errorCode"],
        "languageUnsupported",
        "{}",
        unsupported.structured_content
    );
    let missing = support::run(&runtime, "localFetch", json!({"path":"pkg/none.rs"}), &[]).await;
    assert_eq!(missing.data()["errorCode"], "pathNotFound");
    for outcome in [&unsupported, &missing] {
        let code = support::row_data(outcome)["errorCode"]
            .as_str()
            .expect("code");
        assert!(
            code.chars().all(|c| c.is_ascii_alphanumeric()),
            "{code} is not camelCase"
        );
        assert!(
            octocode_native::id::error_codes::parse(code).is_some(),
            "{code} is not declared"
        );
    }
    runtime.close().await;
}
