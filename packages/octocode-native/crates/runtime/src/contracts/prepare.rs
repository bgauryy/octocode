//! The input's shape before any row is validated: an envelope of object
//! rows or a saved-response read. A shape the contract does not take is rejected
//! with an issue whose rule the repair reads (`validate/repair.rs`), never
//! reshaped here: a nested envelope's fields would be lost silently.

use serde_json::{Value, json};

use super::ValidationIssue;

/// The rejection for input with neither fresh queries nor a saved-response token.
const ENVELOPE_REQUIRED: &str = "Fresh calls require {\"queries\":[...]}; continuations require {\"cursor\":\"...\"} without queries.";

/// Rule of a bare array of rows: the repair wraps it as `queries`.
pub(crate) const ENVELOPE_ARRAY: &str = "envelope.array";
/// Rule of a row that is itself an envelope: the repair lifts its rows.
pub(crate) const ENVELOPE_NESTED: &str = "envelope.nested";
/// Rule of any other shape the envelope does not take (no repair).
const ENVELOPE_SHAPE: &str = "envelope.shape";

/// The shape issues of `input` for `tool_name`; empty when it is the
/// envelope of object rows or a saved-response read for canonical validation.
pub(super) fn envelope_issues(tool_name: &str, input: &Value) -> Vec<ValidationIssue> {
    let issue =
        |rule: &str, path: Vec<String>, message: String, schema: Option<Value>| ValidationIssue {
            rule_id: rule.to_owned(),
            path,
            message,
            schema,
            received: None,
        };
    let Some(rows) = input.as_object().and_then(|object| object.get("queries")) else {
        if input
            .as_object()
            .is_some_and(|object| object.contains_key("cursor"))
        {
            return Vec::new();
        }
        let rule = if input.as_array().is_some_and(|rows| !rows.is_empty()) {
            ENVELOPE_ARRAY
        } else {
            ENVELOPE_SHAPE
        };
        return vec![issue(rule, Vec::new(), ENVELOPE_REQUIRED.to_owned(), None)];
    };
    let Some(rows) = rows.as_array() else {
        return vec![ValidationIssue {
            rule_id: "schema.type".into(),
            path: vec!["queries".to_owned()],
            message: "queries must be an array of query objects.".into(),
            schema: super::tool_contract_named(tool_name)
                .and_then(Result::ok)
                .and_then(|contract| {
                    let input = &contract["inputSchema"];
                    super::schema_facts::property_schemas(input, &input["$defs"], "queries")
                        .into_iter()
                        .find(|schema| schema.get("items").is_some())
                        .cloned()
                }),
            received: Some(rows.clone()),
        }];
    };
    let envelope_fields = || {
        let fields = super::tool_contract_named(tool_name)
            .and_then(Result::ok)
            .map(|tool| {
                let input = &tool["inputSchema"];
                super::schema_facts::property_names(input, &input["$defs"])
                    .into_iter()
                    .filter(|key| *key != "queries")
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        json!({ "envelopeFields": fields })
    };
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let nested = match row {
                Value::Object(row) if row.contains_key("queries") => true,
                Value::Object(_) => return None,
                Value::Array(_) => true,
                _ => false,
            };
            let path = vec!["queries".to_owned(), index.to_string()];
            Some(if nested {
                let mut path = path;
                path.push("queries".into());
                issue(
                    ENVELOPE_NESTED,
                    path,
                    "A row holds its own queries: put those rows directly in queries[] and its other fields (responseChars) beside queries. Nothing ran.".to_owned(),
                    Some(envelope_fields()),
                )
            } else {
                issue(
                    ENVELOPE_SHAPE,
                    path,
                    format!("queries[{index}] must be a query object."),
                    None,
                )
            })
        })
        .collect()
}
