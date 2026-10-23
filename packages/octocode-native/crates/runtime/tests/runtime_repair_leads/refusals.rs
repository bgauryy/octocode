//! Refusals and dropped fields: what an operation lacks or ignores, caller-only fixes, approximate coordinates and named low-confidence drops.

use super::assert_executable;
use crate::support::{Run as _, Workspace};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};

const COMMIT: &str = "commit";

fn commit_row(sections: Value) -> Value {
    json!({
        "owner": "o", "repo": "r", "operation": COMMIT, "ref": "main",
        "sections": sections,
        "mainGoal": "Read the commit.", "reasoning": "Exercise repair leads."
    })
}

#[tokio::test]
async fn an_enum_value_the_operation_lacks_leads_to_the_call_without_it() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let input = json!({"queries":[commit_row(json!(["files", "comments"]))]});
    let error = runtime
        .run("enum".into(), "ghGetHistoryItem".into(), input)
        .await
        .expect_err("comments is a pull-request section");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("ghGetHistoryItem", repair);
    assert_eq!(
        repair["query"],
        json!({"queries":[commit_row(json!(["files"]))]}),
        "{payload}"
    );

    // A text-only MCP client reads the same lead: the text is the envelope.
    let mcp = runtime
        .run_mcp(
            "enum-mcp".into(),
            "ghGetHistoryItem".into(),
            json!({"queries":[commit_row(json!(["comments"]))]}),
        )
        .await
        .expect("MCP reports the rejection in-band");
    let text = mcp["content"][0]["text"].as_str().unwrap();
    assert_eq!(text, mcp["structuredContent"].to_string());
    let hints = mcp["structuredContent"]["hints"].to_string();
    assert!(
        hints.contains("The repair lead drops queries[0].sections.0.")
            && hints.contains("repair: Approximate: ghGetHistoryItem {"),
        "{hints}"
    );
    // Dropping the only value drops the field: the default sections.
    let query =
        &crate::support::checks::hint(&mcp["structuredContent"], "repair")["query"]["queries"][0];
    assert!(query.get("sections").is_none(), "{query}");
    runtime.close().await;
}

/// clasifyContext judges hit snippets: a path-list or count view is refused
/// at admission, before any scan, and the repair drops only the view.
#[tokio::test]
async fn a_selection_with_a_list_view_leads_to_the_snippet_view() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let row = json!({
        "path": ".", "matchString": "x",
        "clasifyContext": {"classifiers": ["Relevant?"]},
        "mainGoal": "Find x.", "reasoning": "Exercise repair leads."
    });
    let mut listed = row.clone();
    listed["resultView"] = json!("files");
    let error = runtime
        .run(
            "view".into(),
            "localSearch".into(),
            json!({"queries":[listed]}),
        )
        .await
        .expect_err("a files view has no snippets to classify");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("localSearch", repair);
    assert_eq!(repair["query"], json!({"queries":[row]}), "{payload}");
    runtime.close().await;
}

#[tokio::test]
async fn a_field_the_operation_ignores_leads_to_the_call_without_it() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let input = json!({"queries":[{
        "operation": "symbols", "path": ".", "pattern": "eval($X)",
        "mainGoal": "List symbols.", "reasoning": "Exercise repair leads."
    }]});
    let error = runtime
        .run("sibling".into(), "astSearch".into(), input)
        .await
        .expect_err("pattern belongs to another operation");
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("astSearch", repair);
    assert!(
        repair["query"]["queries"][0].get("pattern").is_none(),
        "{repair}"
    );
    assert_eq!(repair["query"]["queries"][0]["operation"], "symbols");
    runtime.close().await;
}

#[tokio::test]
async fn a_fix_only_the_caller_knows_carries_no_lead() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_BETA", "1".into())]);
    // A missing field, an enum value no rule maps to one listed value (its
    // default would widen the call), and a near-miss field name each need
    // the caller; the message names the fix.
    for (tool, row) in [
        (
            "lspSearch",
            json!({"operation":"definition","path":"a.ts","mainGoal":"g","reasoning":"r"}),
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","caseMode":"ignore","mainGoal":"g","reasoning":"r"}),
        ),
        (
            "localFetch",
            json!({"path":"README.md","minify":"none","mainGoal":"g","reasoning":"r"}),
        ),
        (
            "localSearch",
            json!({"path":".","matchstring":"x","mainGoal":"g","reasoning":"r"}),
        ),
    ] {
        let error = runtime
            .run("none".into(), tool.into(), json!({"queries":[row]}))
            .await
            .expect_err("invalid");
        let payload = error.payload.as_deref().expect("payload");
        assert!(payload.get("hints").is_none(), "{tool}: {payload}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn dropping_a_constraint_the_caller_set_is_a_named_low_confidence_repair() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let input = json!({"queries":[{
        "path":".","matchString":"x","glob":"*.ts","mainGoal":"g","reasoning":"r"
    }]});
    let error = runtime
        .run("lossy".into(), "localSearch".into(), input)
        .await
        .expect_err("glob is not a localSearch field");
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_executable("localSearch", repair);
    assert_eq!(repair["confidence"], "low", "{payload}");
    assert!(
        crate::support::checks::hint(payload, "text")[0]
            .as_str()
            .is_some_and(|note| note.contains("queries[0].glob")),
        "{payload}"
    );
    runtime.close().await;
}

/// A ref read from a web URL may end inside the file path: the lead is
/// low confidence and names the guessed fields; a field of another form
/// the row's own selector excludes is dropped, named.
#[tokio::test]
async fn approximate_and_lossy_coordinate_repairs_are_named() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (tool, row, fixed, named) in [
        (
            "ghGetFileContent",
            json!({"path":"https://github.com/o/r/blob/main/src/a.rs"}),
            json!({"path":"src/a.rs","owner":"o","repo":"r","ref":"main"}),
            "queries[0].path, queries[0].ref",
        ),
        (
            "ghSearchHistory",
            json!({"owner":"o","repo":"r","operation":"pullRequest","path":"src/a.rs"}),
            json!({"owner":"o","repo":"r","operation":"pullRequest"}),
            "queries[0].path",
        ),
    ] {
        let error = runtime
            .run(tool.into(), tool.into(), json!({"queries":[row]}))
            .await
            .expect_err("invalid");
        let payload = error.payload.as_deref().expect("payload");
        let repair = &crate::support::checks::hint(payload, "repair");
        assert_executable(tool, repair);
        assert_eq!(repair["confidence"], "low", "{tool}: {payload}");
        assert_eq!(
            repair["query"],
            json!({"queries":[fixed]}),
            "{tool}: {payload}"
        );
        assert!(
            crate::support::checks::hint(payload, "text")[0]
                .as_str()
                .is_some_and(|note| note.contains(named)),
            "{tool}: {payload}"
        );
    }
    runtime.close().await;
}

/// A minDepth past maxDepth and a URL package name are refused with the
/// fix named and no lead: only the caller knows the intended value.
#[tokio::test]
async fn rule_refusals_name_the_fix_without_a_lead() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    for (tool, row, message) in [
        (
            "structureSearch",
            json!({"operation":"files","path":".","minDepth":3,"maxDepth":1}),
            "minDepth must be less than or equal to maxDepth",
        ),
        (
            "artifactSearch",
            json!({"ecosystem":"npm","packageName":"https://www.npmjs.com/package/react"}),
            "is a URL; send the name its manifest uses",
        ),
        // A value below the minimum of a field the row requires names what
        // to read: no bound is a safe guess, so the message names the range.
        (
            "ghGetHistoryItem",
            json!({"owner":"facebook","repo":"react","operation":"pullRequest","number":-1}),
            "outside the allowed range (1-",
        ),
        (
            "lspSearch",
            json!({"operation":"definition","path":"src/a.ts","symbolName":"a","line":0}),
            "outside the allowed range (1-",
        ),
    ] {
        let error = runtime
            .run(tool.into(), tool.into(), json!({"queries":[row]}))
            .await
            .expect_err("invalid");
        let payload = error.payload.as_deref().expect("payload");
        assert!(
            payload["error"]
                .as_str()
                .is_some_and(|error| error.contains(message)),
            "{payload}"
        );
        assert!(
            crate::support::checks::hint(payload, "repair").is_null(),
            "{payload}"
        );
    }
    runtime.close().await;
}

/// A field that fails its pattern names itself and the pattern, not the
/// usage prose of its description.
#[tokio::test]
async fn a_pattern_refusal_names_the_field_and_the_pattern() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let error = runtime
        .run(
            "ghSearchRepo".into(),
            "ghSearchRepo".into(),
            json!({"queries":[{"owner":"a b"}]}),
        )
        .await
        .expect_err("invalid");
    let payload = error.payload.as_deref().expect("payload");
    let message = payload["error"].as_str().expect("message");
    assert!(
        message.contains(r#""a b" is not a valid owner"#),
        "{message}"
    );
    assert!(message.contains(r"^[\w.][\w.-]*$"), "{message}");
    assert!(!message.contains("newest push first"), "{message}");
    runtime.close().await;
}

/// An unknown field named for a kind of value that `ref` takes does not
/// overwrite a `ref` the row sets: the lead drops it, named.
#[tokio::test]
async fn a_kind_field_never_overwrites_the_ref_the_row_sets() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let error = runtime
        .run(
            "ghStructure".into(),
            "ghStructure".into(),
            json!({"queries":[{"owner":"o","repo":"r","operation":"refs","ref":"v1","branch":"main"}]}),
        )
        .await
        .expect_err("invalid");
    let payload = error.payload.as_deref().expect("payload");
    let repair = &crate::support::checks::hint(payload, "repair");
    assert_eq!(repair["confidence"], "low", "{payload}");
    assert_eq!(
        repair["query"],
        json!({"queries":[{"owner":"o","repo":"r","operation":"refs","ref":"v1"}]}),
        "{payload}"
    );
    runtime.close().await;
}
