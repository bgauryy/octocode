mod artifact;
#[cfg(test)]
mod canonical_tests;
mod clasify;
mod depth_range;
mod issues;
mod lsp;
mod messages;
#[cfg(test)]
mod numeric_tests;
mod output;
#[cfg(test)]
mod pattern_tests;
mod relations;
mod rules;
mod schema;
mod structural_rule;
mod union;
use rules::{apply_validation_rules, query_values};

pub(crate) use issues::levenshtein;
use issues::{internal, issue, query_label, schema_issue, sibling_requirement};
pub use messages::{format_input_error, tool_error};
pub use output::validate_output;
pub(crate) use output::validate_output_in_place;
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
    validate_from(tool_name, input, false)
}

#[cfg(test)]
pub(super) fn validate_saved(
    tool_name: &str,
    input: Value,
) -> Result<Value, ContractValidationError> {
    validate_from(tool_name, input, true)
}

fn validate_from(
    tool_name: &str,
    input: Value,
    saved: bool,
) -> Result<Value, ContractValidationError> {
    let (input, batch) = validate_batch(tool_name, input, saved)?;
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
    saved: bool,
) -> Result<(Value, BatchIssues), ContractValidationError> {
    let (id, tool) = contract_tool(tool_name)?;
    let plan = RowPlan::of(id, tool);
    let mut rows = Vec::new();
    let item_schema = plan.item_schema.as_ref();
    if let Some(queries) = input.get_mut("queries").and_then(Value::as_array_mut) {
        for (index, query) in queries.iter_mut().enumerate() {
            let mut issues = Vec::new();
            let mut path = vec!["queries".to_owned(), index.to_string()];
            if !saved {
                for field in super::state_fields(id) {
                    if query.get(field).is_some() {
                        let mut at = path.clone();
                        at.push(field.clone());
                        issues.push(ValidationIssue {
                            rule_id: "input.private-state".into(),
                            path: at,
                            message: format!(
                                "{field} is private progress; copy the issued cursor from next.*."
                            ),
                            schema: None,
                            received: None,
                        });
                    }
                }
                if !issues.is_empty() {
                    rows.push(issues);
                    continue;
                }
            }
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
    fn of(id: ToolId, tool: &Value) -> &'static Self {
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
/// (`responseChars` in a row) as `envelopeField`: it moves beside `queries`.
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

/// Validates a single flat query object and returns the validated, defaulted
/// query. Strips the internal `queries.0.` path prefix from error messages so
/// callers see `field` rather than `queries.0.field`.
pub fn validate_query(tool_name: &str, query: Value) -> Result<Value, ContractValidationError> {
    validate_row(tool_name, query, false)
}

pub(super) fn validate_saved_query(
    tool_name: &str,
    query: Value,
) -> Result<Value, ContractValidationError> {
    validate_row(tool_name, query, true)
}

fn validate_row(
    tool_name: &str,
    query: Value,
    saved: bool,
) -> Result<Value, ContractValidationError> {
    let wrapped = serde_json::json!({ "queries": [query] });
    let mut validated = validate_from(tool_name, wrapped, saved).map_err(strip_queries_prefix)?;
    validated["queries"]
        .as_array_mut()
        .and_then(|arr| arr.first_mut())
        .map(serde_json::Value::take)
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
