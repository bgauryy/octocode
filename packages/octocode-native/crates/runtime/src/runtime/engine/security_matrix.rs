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
use crate::contracts::resolve_ref;
use crate::id::ToolId;
use crate::security::{ContentSecurity, InputPolicy};
use serde_json::{Map, Value, json};

const TOKEN: &str = "ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8";

fn policy(tool: ToolId) -> InputPolicy {
    if tool.is_clasify() {
        InputPolicy::ClasifyEvidence {
            state_limit: crate::tools::clasify::max_state_chars(false),
        }
    } else {
        InputPolicy::Query
    }
}

/// A value the schema admits, from the schema alone: the first enum/const,
/// the first union branch, the smallest string/number, required fields only.
fn sample(schema: &Value, defs: &Value, root: &Value) -> Value {
    let schema = resolve_ref(schema, defs);
    if let Some(constant) = schema.get("const") {
        return constant.clone();
    }
    if let Some(first) = schema["enum"].as_array().and_then(|values| values.first()) {
        return first.clone();
    }
    if let Some(branch) = ["oneOf", "anyOf"]
        .iter()
        .find_map(|key| schema[*key].as_array())
        .and_then(|branches| branches.first())
    {
        return sample(branch, defs, root);
    }
    let kind = match &schema["type"] {
        Value::String(kind) => kind.as_str(),
        Value::Array(kinds) => kinds.first().and_then(Value::as_str).unwrap_or("object"),
        _ => "object",
    };
    match kind {
        "string" => json!("a".repeat(schema["minLength"].as_u64().unwrap_or(1).max(1) as usize)),
        "integer" | "number" => schema.get("minimum").cloned().unwrap_or(json!(1)),
        "boolean" => json!(true),
        "array" => {
            let item = schema
                .get("items")
                .map_or(json!("a"), |items| sample(items, defs, root));
            json!([item])
        }
        _ => Value::Object(required_fields(schema, defs, root)),
    }
}

fn required_fields(schema: &Value, defs: &Value, root: &Value) -> Map<String, Value> {
    let mut object = Map::new();
    for name in schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        let property = &schema["properties"][name];
        object.insert(name.to_owned(), sample(property, defs, root));
    }
    object
}

/// Every `oneOf`/`anyOf` branch of the tool's query schema (or the schema
/// itself) merged over the shared fields, as one object schema each.
fn variants(tool: ToolId) -> Vec<Value> {
    let schema = &contracts::tool_contract(tool).expect("contract")["querySchema"];
    let branches: Vec<&Value> = ["oneOf", "anyOf"]
        .iter()
        .find_map(|key| schema[*key].as_array())
        .map_or_else(|| vec![schema], |branches| branches.iter().collect());
    let defs = &schema["$defs"];
    branches
        .into_iter()
        .map(|branch| {
            let branch = resolve_ref(branch, defs);
            let mut merged = schema.clone();
            for key in ["properties", "required"] {
                match (&mut merged[key], &branch[key]) {
                    (Value::Object(into), Value::Object(from)) => into.extend(from.clone()),
                    (Value::Array(into), Value::Array(from)) => into.extend(from.clone()),
                    (slot @ Value::Null, from) => *slot = from.clone(),
                    _ => {}
                }
            }
            merged
        })
        .collect()
}

fn is_valid(tool: ToolId, row: &Value) -> bool {
    contracts::prepare_rows(tool.as_str(), json!({"queries": [row]}))
        .is_ok_and(|rows| rows.iter().all(Result::is_ok))
}

/// A row the contract accepts, built from the schema: a variant's required
/// fields, plus one sampled optional field when a rule beyond `required`
/// (e.g. "needs keywords, topics, owner, or a filter") asks for one.
fn base_row(tool: ToolId) -> (Value, Value) {
    let schema = &contracts::tool_contract(tool).expect("contract")["querySchema"];
    let defs = &schema["$defs"];
    for variant in variants(tool) {
        let required = required_fields(&variant, defs, schema);
        let mut candidates = vec![Value::Object(required.clone())];
        for (name, property) in variant["properties"].as_object().into_iter().flatten() {
            let mut row = required.clone();
            row.entry(name.clone())
                .or_insert_with(|| sample(property, defs, schema));
            candidates.push(Value::Object(row));
        }
        if let Some(row) = candidates.into_iter().find(|row| is_valid(tool, row)) {
            return (row, variant);
        }
    }
    panic!("{tool:?}: no contract-valid row could be sampled from its query schema");
}

/// A string property (no enum, const or format) the contract accepts
/// `value` in: where a caller's text goes. `None` when the contract itself
/// bounds every such field below `value`.
fn free_text_field(tool: ToolId, base: &Value, variant: &Value, value: &str) -> Option<String> {
    let defs = &contracts::tool_contract(tool).expect("contract")["querySchema"]["$defs"];
    variant["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, schema)| {
            let schema = resolve_ref(schema, defs);
            schema["type"] == "string"
                && ["enum", "const", "format"]
                    .iter()
                    .all(|key| schema.get(*key).is_none())
        })
        .map(|(name, _)| name.clone())
        .find(|name| {
            let mut row = base.clone();
            row[name] = json!(value);
            is_valid(tool, &row)
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
        let (base, variant) = base_row(tool);
        let field = free_text_field(tool, &base, &variant, TOKEN)
            .unwrap_or_else(|| panic!("{tool:?}: no query text field accepts a token"));
        let mut row = base.clone();
        row[&field] = json!(TOKEN);
        let admitted = runtime.admit_queries(tool, &input(&row));
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
        let (base, _) = base_row(tool);
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
        let (base, variant) = base_row(tool);
        // A field the contract leaves unbounded, else an unknown one: the
        // gate bounds every string either way.
        let field = free_text_field(tool, &base, &variant, &"a".repeat(10_001))
            .unwrap_or_else(|| "ext".to_owned());
        let mut long = base.clone();
        long[&field] = json!("a".repeat(10_001));
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
            let admitted = runtime.admit_queries(tool, &input(&row));
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
        (
            "localFetch",
            json!({"queries":[{"path": path, "fullContent": true}]}),
        ),
        (
            "localSearch",
            json!({"queries":[{"path": path, "matchString": "token"}]}),
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
