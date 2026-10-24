//! Per-tool security matrix: every `ToolId` the contract declares passes the
//! same two gates, input and output, and the test lists no tool by hand.
//!
//! Input (a-c) runs the tool's own contract-valid row through
//! `ToolRuntime::admit_queries` and through `check_input` with the tool's
//! policy; output (d) injects secrets into the rows a call shapes (the error
//! row path and an evidence row) and reads the MCP result and the CLI-visible
//! structured value.

use super::construction_tests::runtime;
use crate::contracts;
use crate::id::ToolId;
use crate::security::{ContentSecurity, InputPolicy};
use serde_json::{Value, json};

const TOKEN: &str = "ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8";

fn policy(tool: ToolId) -> InputPolicy {
    if tool.is_clasify() {
        InputPolicy::ClasifyEvidence {
            content_limit: crate::tools::clasify::max_state_chars(false),
        }
    } else {
        InputPolicy::Query
    }
}

/// Authored first-call examples are the schema owner's executable cases.
fn examples(tool: ToolId) -> Vec<Value> {
    let examples: Vec<Value> = contracts::tool_contract(tool).expect("contract")["variants"]
        .as_array()
        .expect("variants")
        .iter()
        .filter_map(|variant| variant.get("example").cloned())
        .collect();
    assert!(!examples.is_empty(), "{tool:?}: no authored examples");
    for row in &examples {
        assert!(
            is_valid(tool, row),
            "{tool:?}: invalid authored example: {row}"
        );
    }
    examples
}

fn is_valid(tool: ToolId, row: &Value) -> bool {
    contracts::prepare_rows(tool.as_str(), input(row))
        .is_ok_and(|rows| rows.iter().all(Result::is_ok))
}

fn base_row(tool: ToolId) -> Value {
    examples(tool).into_iter().next().expect("authored example")
}

/// A caller string the contract admits without changing any sibling fields.
fn free_text_field(tool: ToolId, base: &Value, value: &str) -> Option<String> {
    base.as_object()
        .expect("example row")
        .iter()
        .filter(|(_, sample)| sample.is_string())
        .find_map(|(name, _)| {
            let mut row = base.clone();
            row[name] = json!(value);
            is_valid(tool, &row).then(|| name.clone())
        })
}

fn input(row: &Value) -> Value {
    json!({"queries": [row]})
}

fn payload_text(result: &Result<super::admission::AdmittedRows, super::RuntimeError>) -> String {
    match result {
        Err(error) => format!("{} {:?}", error.message, error.payload),
        Ok(admitted) => format!("{} {:?}", json!(admitted.queries), admitted.rejected),
    }
}

#[test]
fn a_credential_in_a_query_never_runs_and_is_never_echoed_for_any_tool() {
    let home = tempfile::tempdir().expect("home");
    let runtime = runtime(home.path(), &[]);
    for tool in ToolId::ALL {
        let (base, field) = examples(tool)
            .into_iter()
            .find_map(|base| free_text_field(tool, &base, TOKEN).map(|field| (base, field)))
            .unwrap_or_else(|| panic!("{tool:?}: no query text field accepts a token"));
        let mut row = base.clone();
        row[&field] = json!(TOKEN);
        let admitted = runtime.admit_queries(tool, &input(&row), false);
        let shown = payload_text(&admitted);
        assert!(
            !shown.contains(TOKEN),
            "{tool:?}.{field}: credential echoed: {shown}"
        );
        if !tool.is_clasify() {
            assert!(
                admitted
                    .as_ref()
                    .map_or(true, |rows| rows.queries.is_empty()),
                "{tool:?}.{field}: a credential-shaped query was admitted"
            );
        }
        // The gate itself, with the tool's own policy.
        let checked = ContentSecurity::new().check_input(row.clone(), policy(tool));
        if let Ok(checked) = &checked {
            assert!(
                !checked.to_string().contains(TOKEN),
                "{tool:?}: {checked:?}"
            );
        }
        if !tool.is_clasify() {
            let issues = checked.expect_err("a credential is refused, not rewritten");
            assert!(
                issues
                    .iter()
                    .any(|issue| issue.rule_id == "security.credential"
                        && issue.path == [field.clone()]),
                "{tool:?}.{field}: {issues:?}"
            );
        }
    }
}

#[test]
fn a_dangerous_key_is_rejected_for_any_tool() {
    let security = ContentSecurity::new();
    for tool in ToolId::ALL {
        let base = base_row(tool);
        security
            .check_input(base.clone(), policy(tool))
            .unwrap_or_else(|issues| {
                panic!("{tool:?}: the contract-valid base row must pass the gate: {issues:?}")
            });
        for key in ["__proto__", "prototype", "constructor"] {
            for nested in [false, true] {
                let mut row = base.clone();
                if nested {
                    row["ext"] = json!({"outer": {key: {"polluted": true}}});
                } else {
                    row[key] = json!({"polluted": true});
                }
                let issues = security
                    .check_input(row.clone(), policy(tool))
                    .expect_err(&format!(
                        "{tool:?}: {key} (nested: {nested}) must be refused"
                    ));
                assert!(
                    issues.iter().all(|issue| issue.rule_id == "security.input"),
                    "{issues:?}"
                );
                assert!(
                    issues.iter().any(|issue| issue.message.contains(key)),
                    "{tool:?}: {issues:?}"
                );
            }
        }
    }
}

#[test]
fn oversized_or_too_deep_input_is_rejected_for_any_tool() {
    let home = tempfile::tempdir().expect("home");
    let runtime = runtime(home.path(), &[]);
    let security = ContentSecurity::new();
    let deep = (0..25).fold(json!("leaf"), |inner, _| json!({"n": inner}));
    for tool in ToolId::ALL {
        let base = base_row(tool);
        let limit = match policy(tool) {
            InputPolicy::Query => 10_000,
            InputPolicy::ClasifyEvidence { content_limit } => content_limit,
        };
        let field = free_text_field(tool, &base, TOKEN).unwrap_or_else(|| "ext".to_owned());
        let mut long = base.clone();
        long[&field] = json!("a".repeat(limit + 1));
        let wide = {
            let mut row = base.clone();
            row["ext"] = json!(vec!["a"; crate::id::MAX_INPUT_ARRAY_ITEMS + 1]);
            row
        };
        let mut nested = base.clone();
        nested["ext"] = deep.clone();
        for (what, row) in [
            ("long string", long),
            ("long array", wide),
            ("deep object", nested),
        ] {
            let issues = security
                .check_input(row.clone(), policy(tool))
                .expect_err(&format!("{tool:?}: {what} must be refused by the gate"));
            assert!(
                issues.iter().all(|issue| issue.rule_id == "security.input"),
                "{tool:?} {what}: {issues:?}"
            );
            let admitted = runtime.admit_queries(tool, &input(&row), false);
            assert!(
                admitted
                    .as_ref()
                    .map_or(true, |rows| rows.queries.is_empty()),
                "{tool:?}: {what} was admitted"
            );
        }
    }
}

/// (d) A rejected call of any tool whose error text or payload carries a
/// credential answers on every surface (MCP structured and text content,
/// CLI JSON and terminal text) without it.
#[test]
fn a_credential_in_an_error_never_leaves_any_tool() {
    for tool in ToolId::ALL {
        let error = super::RuntimeError {
            code: octocode_contract_types::ErrorCode::ExecutionFailed,
            message: format!("upstream said {TOKEN}"),
            payload: Some(Box::new(json!({"error": format!("token {TOKEN}"),
                "details": [TOKEN],
                "errorCode": octocode_contract_types::ErrorCode::ExecutionFailed}))),
            validation_issues: None,
        };
        let projected = super::super::error::tool_error_result(tool.as_str(), &error);
        let mcp = super::super::error::mcp_result(tool.as_str(), Err(error)).expect("mcp");
        for (surface, shown) in [
            ("CLI JSON", projected.envelope.to_string()),
            ("CLI text", projected.text),
            ("MCP", mcp.to_string()),
        ] {
            assert!(!shown.contains(TOKEN), "{tool:?} {surface}: {shown}");
        }
    }
}

/// (d) Evidence that holds a credential (a local file a tool reads, lists
/// or searches) reaches both channels redacted: the output gate runs on
/// every row a call shapes, before paging, so no page or channel holds it.
#[tokio::test]
async fn a_credential_in_evidence_is_redacted_on_both_channels() {
    let home = tempfile::tempdir().expect("home");
    let runtime = runtime(home.path(), &[]);
    let file = home.path().join("leak.txt");
    std::fs::write(&file, format!("const token = \"{TOKEN}\";\n")).expect("fixture");
    let path = file.to_string_lossy().into_owned();
    let calls = [
        ("localFetch", json!({"queries":[{"path": path}]})),
        (
            "localSearch",
            json!({"queries":[{"path": path, "matchString": ["token"]}]}),
        ),
    ];
    for (tool, input) in calls {
        for channel in [super::Channel::Cli, super::Channel::Mcp] {
            let admission = runtime.admit("matrix".into()).expect("admit");
            let outcome = runtime
                .execute(admission, tool.into(), input.clone(), channel)
                .await
                .expect("call");
            let shown = outcome.structured_content.to_string();
            assert!(
                shown.contains("token"),
                "{tool}: the evidence is read: {shown}"
            );
            assert!(!shown.contains(TOKEN), "{tool} {channel:?}: {shown}");
            let mcp = super::super::error::mcp_result(tool, Ok(outcome)).expect("mcp");
            assert!(
                !mcp.to_string().contains(TOKEN),
                "{tool} {channel:?}: {mcp}"
            );
        }
    }
}
