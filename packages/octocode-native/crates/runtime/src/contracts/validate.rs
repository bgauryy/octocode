mod artifact;
mod clasify;
mod content;
mod depth_range;
mod enum_value;
mod github_search;
mod history;
mod local_search;
mod lsp;
#[cfg(test)]
mod normalizer_tests;
#[cfg(test)]
mod numeric_tests;
#[cfg(test)]
mod pattern_tests;
mod qualifiers;
mod repair;
mod repository;
mod shape;
mod structural_rule;
pub(crate) use qualifiers::qualifier_terms;
pub(crate) use repair::repair_input;
mod issues;
mod messages;
mod output;
mod routing;
mod rules;
mod schema;
mod union;
use rules::{apply_normalization_rules, apply_validation_rules, query_values};

pub(crate) use artifact::split_coordinate;
pub(crate) use issues::levenshtein;
use issues::{
    field_guess, internal, issue, name_field, query_label, schema_issue, sibling_message,
    sibling_requirement, suggest_field,
};
pub use messages::{format_input_error, tool_error};
pub use output::validate_output;
pub(crate) use output::validate_output_in_place;
use routing::routing_details;
use schema::validate_input_schema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::{Display, Formatter};

use crate::id::ToolId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub rule_id: String,
    pub path: Vec<String>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub received: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractValidationError {
    pub issues: Vec<ValidationIssue>,
}

impl Display for ContractValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "contract validation failed with {} issue(s)",
            self.issues.len()
        )
    }
}

impl std::error::Error for ContractValidationError {}

/// The named tool and its embedded contract entry, or the unknown-tool issue.
fn contract_tool(tool_name: &str) -> Result<(ToolId, &'static Value), ContractValidationError> {
    let id = ToolId::from_name(tool_name).ok_or_else(|| {
        issue(
            "contract.unknown-tool",
            vec![],
            format!("Unknown tool: {tool_name}"),
        )
    })?;
    super::tool_contract(id)
        .map(|tool| (id, tool))
        .map_err(|error| internal(error.to_string()))
}

/// Validates the canonical input, applying schema defaults and integer number
/// representations before typed decoding. Relation rules follow structural parsing.
pub fn validate(tool_name: &str, input: Value) -> Result<Value, ContractValidationError> {
    let (input, batch) = validate_batch(tool_name, input)?;
    let mut issues: Vec<_> = batch
        .rows
        .into_iter()
        .flatten()
        .chain(batch.envelope)
        .collect();
    if !issues.is_empty() {
        issues.dedup();
        return Err(ContractValidationError { issues });
    }
    Ok(input)
}

/// One validation pass's issues: each row's own (paths `queries.<i>.…`, in
/// row order) and the envelope's.
pub(crate) struct BatchIssues {
    pub rows: Vec<Vec<ValidationIssue>>,
    pub envelope: Vec<ValidationIssue>,
}

/// [`validate`] in one pass that keeps each row's issues apart, so a batch
/// isolates its invalid rows without validating any row twice. `Err` is a
/// failure of the contract itself (an unknown tool, a broken rule).
pub(crate) fn validate_batch(
    tool_name: &str,
    mut input: Value,
) -> Result<(Value, BatchIssues), ContractValidationError> {
    let (id, tool) = contract_tool(tool_name)?;
    let plan = RowPlan::of(id, tool);
    apply_normalization_rules(&tool["rules"], &mut input)?;
    let mut rows = Vec::new();
    let item_schema = plan.item_schema.as_ref();
    if let Some(queries) = input.get_mut("queries").and_then(Value::as_array_mut) {
        for (index, query) in queries.iter_mut().enumerate() {
            let mut issues = Vec::new();
            let mut path = vec!["queries".to_owned(), index.to_string()];
            let mut parsed =
                validate_input_schema(&tool["querySchema"], &tool["querySchema"], query, &mut path);
            if parsed.is_ok()
                && let Some(item_schema) = item_schema
            {
                parsed = validate_input_schema(&tool["inputSchema"], item_schema, query, &mut path);
            }
            if let Err(error) = parsed {
                let mut issues = error.issues;
                mark_envelope_fields(id, tool, &mut issues);
                rows.push(issues);
                continue;
            }
            // Row rules see a one-row batch; the row moves in and back out.
            let scoped = serde_json::json!({"queries":[std::mem::take(query)]});
            let ruled = apply_validation_rules(&tool["rules"], &scoped);
            if let Value::Object(mut scoped) = scoped
                && let Some(Value::Array(mut rows)) = scoped.remove("queries")
                && let Some(row) = rows.pop()
            {
                *query = row;
            }
            if let Err(mut error) = ruled {
                for issue in &mut error.issues {
                    if issue.path.first().is_some_and(|part| part == "queries")
                        && issue.path.get(1).is_some_and(|part| part == "0")
                    {
                        issue.path[1] = index.to_string();
                    }
                }
                issues.extend(error.issues);
            }
            rows.push(issues);
        }
    }
    // The rows were checked above; the envelope pass checks only the
    // envelope (its own fields and the batch size), and reports its issues
    // beside the rows' so one repair can fix every one.
    let envelope = validate_envelope_with(tool, plan, &mut input)
        .err()
        .map(|error| error.issues)
        .unwrap_or_default();
    Ok((input, BatchIssues { rows, envelope }))
}

/// The envelope pass of [`validate_batch`] alone: the envelope's own fields
/// and the batch size over rows that are validated already.
pub(crate) fn validate_envelope(
    tool_name: &str,
    input: &mut Value,
) -> Result<(), ContractValidationError> {
    let (id, tool) = contract_tool(tool_name)?;
    validate_envelope_with(tool, RowPlan::of(id, tool), input)
}

fn validate_envelope_with(
    tool: &Value,
    plan: &RowPlan,
    input: &mut Value,
) -> Result<(), ContractValidationError> {
    validate_input_schema(&tool["inputSchema"], &plan.envelope, input, &mut Vec::new())
}

/// How one tool's rows validate, decided once from its contract.
struct RowPlan {
    /// The envelope's row schema says more than the query schema: rows
    /// validate against both. Equal schemas (under the same `$defs`) are one
    /// pass.
    item_schema: Option<Value>,
    /// The input schema without its row schema: rows are validated already.
    envelope: Value,
}

impl RowPlan {
    fn of(id: ToolId, tool: &Value) -> &'static RowPlan {
        static PLANS: [std::sync::OnceLock<RowPlan>; ToolId::ALL.len()] =
            [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
        PLANS[id.index()].get_or_init(|| Self::derive(tool))
    }

    fn derive(tool: &Value) -> Self {
        let without_marker = |schema: &Value| {
            let mut schema = schema.clone();
            if let Some(fields) = schema.as_object_mut() {
                fields.remove("$schema");
            }
            schema
        };
        let (input, query) = (&tool["inputSchema"], &tool["querySchema"]);
        let mut items = Vec::new();
        for queries in super::schema_facts::property_schemas(input, &input["$defs"], "queries") {
            for item in super::schema_facts::item_schemas(queries, &input["$defs"]) {
                if !items.contains(&item) {
                    items.push(item);
                }
            }
        }
        let mut envelope = input.clone();
        let item_schema = if let [item] = items.as_slice() {
            strip_query_items(&mut envelope);
            (without_marker(item) != without_marker(query)
                || (item.to_string().contains("\"$ref\"") && input["$defs"] != query["$defs"]))
                .then(|| (*item).clone())
        } else {
            // Different envelope branches may impose different row schemas;
            // keep their complete validation instead of intersecting alternatives.
            None
        };
        Self {
            item_schema,
            envelope,
        }
    }
}

fn strip_query_items(schema: &mut Value) {
    if let Some(queries) = schema
        .pointer_mut("/properties/queries")
        .and_then(Value::as_object_mut)
    {
        queries.remove("items");
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = schema.get_mut(keyword).and_then(Value::as_array_mut) {
            for branch in branches {
                strip_query_items(branch);
            }
        }
    }
}

/// Marks a row's unknown field that only the envelope takes
/// (`responseLength` in a row) as `envelopeField`: it moves beside `queries`.
fn mark_envelope_fields(id: ToolId, tool: &Value, issues: &mut [ValidationIssue]) {
    let input = &tool["inputSchema"];
    for issue in issues {
        let [queries, _, field] = issue.path.as_slice() else {
            continue;
        };
        if issue.rule_id != "schema.unknown-field"
            || queries != "queries"
            || field == "queries"
            || super::query_declares(id, field)
            || !super::schema_facts::property_names(input, &input["$defs"]).contains(field.as_str())
        {
            continue;
        }
        if let Some(schema) = issue.schema.as_mut().and_then(Value::as_object_mut) {
            schema.insert("envelopeField".into(), Value::Bool(true));
        }
    }
}

/// Whether an unknown row field is one the envelope takes.
fn envelope_field(issue: &ValidationIssue) -> bool {
    issue
        .schema
        .as_ref()
        .is_some_and(|schema| schema.get("envelopeField") == Some(&Value::Bool(true)))
}

/// The known fields that express an unknown field's constraint (core
/// `x-covers`: `pageSize` covers `limit`), quoted: a pointer to the field to
/// send, never an alias (the unknown field stays rejected).
fn cover_field(issue: &ValidationIssue) -> Option<String> {
    let fields = issue
        .schema
        .as_ref()?
        .get("coverFields")?
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(|field| format!("'{field}'"))
        .collect::<Vec<_>>();
    (!fields.is_empty()).then(|| fields.join(" or "))
}

/// The one input shape every host's call takes: a bare query row (an object
/// without `queries`) runs as a one-row `queries` (see [`one_row_envelope`]).
/// Nothing else changes: a value the canonical schema rejects is rejected by
/// validation with a repair hint, never repaired here. An unknown tool is
/// returned unchanged.
#[must_use]
pub(crate) fn normalize_input(tool_name: &str, input: Value) -> Value {
    let Some(Ok(tool)) = super::tool_contract_named(tool_name) else {
        return input;
    };
    one_row_envelope(&tool["inputSchema"], input)
}

/// A bare query row (an object without `queries`) as `{"queries":[row]}`: the
/// envelope fields it carries (`responseLength`, …) stay on the envelope. An
/// object with only envelope fields is not a row and is returned unchanged.
fn one_row_envelope(input_schema: &Value, input: Value) -> Value {
    let Value::Object(mut row) = input else {
        return input;
    };
    if row.contains_key("queries") {
        return Value::Object(row);
    }
    let mut envelope = serde_json::Map::new();
    for key in super::schema_facts::property_names(input_schema, &input_schema["$defs"]) {
        if let Some(value) = row.remove(key) {
            envelope.insert(key.to_owned(), value);
        }
    }
    if row.is_empty() {
        return Value::Object(envelope);
    }
    envelope.insert("queries".into(), Value::Array(vec![Value::Object(row)]));
    Value::Object(envelope)
}

/// Validates a single flat query object and returns the validated, defaulted
/// query. Strips the internal `queries.0.` path prefix from error messages so
/// callers see `field` rather than `queries.0.field`.
pub fn validate_query(tool_name: &str, query: Value) -> Result<Value, ContractValidationError> {
    let wrapped = serde_json::json!({ "queries": [query] });
    let mut validated = validate(tool_name, wrapped).map_err(strip_queries_prefix)?;
    validated["queries"]
        .as_array_mut()
        .and_then(|arr| arr.first_mut())
        .map(|q| q.take())
        .ok_or_else(|| ContractValidationError {
            issues: vec![ValidationIssue {
                rule_id: "validate.extract".to_owned(),
                path: Vec::new(),
                message: "validated queries array was empty".to_owned(),
                schema: None,
                received: None,
            }],
        })
}

fn strip_queries_prefix(mut error: ContractValidationError) -> ContractValidationError {
    for issue in &mut error.issues {
        if issue.path.first().map(String::as_str) == Some("queries")
            && issue.path.get(1).map(String::as_str) == Some("0")
        {
            issue.path.drain(..2);
        }
    }
    error
}

#[cfg(test)]
mod tests;
