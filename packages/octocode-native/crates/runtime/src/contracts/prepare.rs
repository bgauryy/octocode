//! The input's shape before any row is validated: an envelope of object
//! rows or a saved-response read. A shape the contract does not take is rejected
//! and reported, never reshaped here: a nested envelope's fields would be lost
//! silently.

use serde_json::Value;

use super::ValidationIssue;

/// The rejection for input with neither fresh queries nor a saved-response token.
const ENVELOPE_REQUIRED: &str = "Fresh calls require {\"queries\":[...]}; continuations require {\"cursor\":\"...\"} without queries.";

/// Rule of a shape the envelope does not take.
const ENVELOPE_SHAPE: &str = "envelope.shape";

/// The message for an object that is neither envelope: a field no row
/// declares is not a request field. A bare row is wrapped before validation
/// (`normalize::shaped`).
fn shape_repair(tool_name: &str, object: &serde_json::Map<String, Value>) -> Option<String> {
    let tool = crate::id::ToolId::from_name(tool_name)?;
    let field = object
        .keys()
        .find(|field| !super::query_declares(tool, field))?;
    Some(format!(
        "Unknown request field '{field}': fresh calls send {{\"queries\":[...]}}; continuations send {{\"cursor\":\"...\"}} alone."
    ))
}

/// A batch over the tool's `queries` limit: the rest go in another call.
fn oversized_batch(tool_name: &str, rows: usize) -> Option<ValidationIssue> {
    let contract = super::tool_contract_named(tool_name)?.ok()?;
    let input = &contract["inputSchema"];
    let maximum = super::schema_facts::property_schemas(input, &input["$defs"], "queries")
        .into_iter()
        .find_map(|queries| {
            super::schema_facts::schema_keyword(queries, &input["$defs"], "maxItems")?.as_u64()
        })?;
    (rows as u64 > maximum).then(|| ValidationIssue {
        rule_id: "schema.size".into(),
        path: vec!["queries".to_owned()],
        message: format!(
            "Value length {rows} exceeds the maximum of {maximum}; send the other rows in another call"
        ),
        schema: None,
        received: None,
    })
}

/// The shape issues of `input` for `tool_name`; empty when it is the
/// envelope of object rows or a saved-response read for canonical validation.
pub(super) fn envelope_issues(tool_name: &str, input: &Value) -> Vec<ValidationIssue> {
    let issue = |path: Vec<String>, message: String| ValidationIssue {
        rule_id: ENVELOPE_SHAPE.to_owned(),
        path,
        message,
        schema: None,
        received: None,
    };
    let Some(rows) = input.as_object().and_then(|object| object.get("queries")) else {
        if input
            .as_object()
            .is_some_and(|object| object.contains_key("cursor"))
        {
            return Vec::new();
        }
        let message = input
            .as_object()
            .and_then(|object| shape_repair(tool_name, object))
            .unwrap_or_else(|| ENVELOPE_REQUIRED.to_owned());
        return vec![issue(Vec::new(), message)];
    };
    let Some(rows) = rows.as_array() else {
        return vec![ValidationIssue {
            rule_id: "schema.type".into(),
            path: vec!["queries".to_owned()],
            message: format!(
                "queries must be an array of query objects; {}",
                super::validate::repair_queries(rows)
            ),
            schema: super::tool_contract_named(tool_name)
                .and_then(Result::ok)
                .and_then(|contract| {
                    let input = &contract["inputSchema"];
                    super::schema_facts::property_schemas(input, &input["$defs"], "queries")
                        .into_iter()
                        .find(|schema| {
                            super::schema_facts::schema_keyword(schema, &input["$defs"], "items")
                                .is_some()
                        })
                        .map(|schema| {
                            super::schema_facts::resolve_ref(schema, &input["$defs"]).clone()
                        })
                }),
            received: Some(rows.clone()),
        }];
    };
    if let Some(issue) = oversized_batch(tool_name, rows.len()) {
        return vec![issue];
    }
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
                    path,
                    "A row holds its own queries: put those rows directly in queries[] and its other fields (responseChars) beside queries. Nothing ran.".to_owned(),
                )
            } else {
                issue(path, format!("queries[{index}] must be a query object."))
            })
        })
        .collect()
}
