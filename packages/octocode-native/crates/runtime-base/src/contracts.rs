//! Generated public tool contracts and transport-neutral input preparation.

pub mod envelope;
pub(crate) mod generated;
mod instructions;
pub mod listing_roles;
mod normalize;
mod prepare;
mod private_state;
pub mod query_limits;
pub use normalize::{BesideCursor, normalized_input, read_beside_cursor};
pub use private_state::{BRIEF_FIELDS, state_fields};
pub(crate) mod output_fields;
mod schema_facts;
pub mod security;
#[cfg(test)]
pub(crate) mod undeclared;
mod validate;
pub use instructions::{compose_instructions, setting_instructions};
use octocode_contract_types::ErrorCode;
pub use validate::{
    ContractValidationError, ValidationIssue, format_input_error, levenshtein, tool_error,
    validate, validate_output, validate_query,
};

use crate::id::ToolId;
pub(crate) use schema_facts::{
    additional_schemas, child_schemas, item_schemas, property_names, property_order,
    property_schemas, resolve_ref,
};
pub use schema_facts::{
    query_declares, query_schema_max, query_schema_number, query_schema_value, schema_keyword,
};
pub(crate) use validate::validate_output_in_place;
#[cfg(any(test, feature = "test-support"))]
pub use validate::validate_saved;

/// Embedded contracts are immutable across runtime handles and requests.
pub fn parsed_contract() -> Result<&'static serde_json::Value, &'static str> {
    CONTRACT
        .get_or_init(|| serde_json::from_str(contract_json()).map_err(|error| error.to_string()))
        .as_ref()
        .map_err(String::as_str)
}

static CONTRACT: std::sync::OnceLock<Result<serde_json::Value, String>> =
    std::sync::OnceLock::new();

/// A recovery notice of classified results, authored once in core
/// (`classificationNotices.<key>`); empty only if the embedded contract
/// lacks it, which the contract tests rule out.
pub fn classification_notice(key: &str) -> &'static str {
    parsed_contract()
        .ok()
        .and_then(|contract| contract.pointer(&format!("/classificationNotices/{key}")))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

/// Every `partialReasons` value and what it means, authored once in core and
/// served only by the output reference: a response carries the code alone.
pub fn partial_reasons() -> Option<&'static serde_json::Value> {
    parsed_contract().ok()?.get("partialReasons")
}

/// [`partial_reasons`] less every reason `tool` never emits: what the
/// output reference of `tool` serves.
pub fn partial_reasons_of(tool: ToolId) -> Option<serde_json::Value> {
    let all = partial_reasons()?.as_object()?;
    Some(serde_json::Value::Object(
        output_fields::partial_reasons::emitted_by(tool, all),
    ))
}

/// One tool's contract entry (`tools[i]`), parsed on first use from its
/// build-verified span of the embedded contract. A tool call touches one tool,
/// so a fresh process parses that entry (~100 KB) instead of the whole
/// multi-megabyte contract — the dominant cost of a short-lived CLI call.
/// Reuses the full parse when something (catalog, scheme) already forced it.
pub fn tool_contract(tool: ToolId) -> Result<&'static serde_json::Value, &'static str> {
    static TOOLS: [std::sync::OnceLock<Result<serde_json::Value, String>>; ToolId::ALL.len()] =
        [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    let index = tool.index();
    if let Some(Ok(contract)) = CONTRACT.get() {
        return contract["tools"]
            .get(index)
            .ok_or("tool is not in the embedded contract");
    }
    TOOLS[index]
        .get_or_init(|| {
            let (start, end) = generated::CONTRACT_TOOL_SPANS[index];
            contract_json()
                .get(start..end)
                .ok_or_else(|| "tool span is outside the embedded contract".to_owned())
                .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()))
        })
        .as_ref()
        .map_err(String::as_str)
}

/// The embedded tool contract by wire name; `None` for a name the contract lacks.
pub fn tool_contract_named(name: &str) -> Option<Result<&'static serde_json::Value, &'static str>> {
    ToolId::from_name(name).map(tool_contract)
}

/// How many `queries` rows one call of `tool` carries (its schema's
/// `maxItems`); `None` when the contract states no limit.
pub fn batch_limit(tool: ToolId) -> Option<usize> {
    let input = &tool_contract(tool).ok()?["inputSchema"];
    schema_facts::property_schemas(input, &input["$defs"], "queries")
        .into_iter()
        .find_map(|queries| {
            schema_facts::schema_keyword(queries, &input["$defs"], "maxItems")?.as_u64()
        })
        .and_then(|maximum| usize::try_from(maximum).ok())
}

/// Replace contract-violating result rows with row-level
/// `outputContractViolation` errors so one drifting emitter cannot discard a
/// batch's healthy rows. Returns the patched envelope only when every issue
/// maps to a result row and the patched envelope itself validates; envelope-
/// level violations return `None` and the caller keeps the whole-call failure.
pub(crate) fn isolate_row_violations(
    tool_name: &str,
    output: &serde_json::Value,
    error: &ContractValidationError,
) -> Option<serde_json::Value> {
    let mut row_issues: std::collections::BTreeMap<usize, Vec<String>> =
        std::collections::BTreeMap::new();
    for issue in &error.issues {
        let index = match (issue.path.first().map(String::as_str), issue.path.get(1)) {
            (Some("results"), Some(second)) => second.parse::<usize>().ok()?,
            _ => return None,
        };
        row_issues.entry(index).or_default().push(format!(
            "{}: {}",
            issue.path.get(2..).unwrap_or_default().join("."),
            issue.message
        ));
    }
    if row_issues.is_empty() {
        return None;
    }
    let mut patched = output.clone();
    let rows = patched.get_mut("results")?.as_array_mut()?;
    for (index, issues) in &row_issues {
        let row = rows.get_mut(*index)?;
        row["status"] = serde_json::Value::String("error".into());
        row["data"] = serde_json::json!({
            "error": format!(
                "Row output violated the {tool_name} contract and was withheld: {}",
                issues.join("; ")
            ),
            "errorCode": ErrorCode::OutputContractViolation,
        });
    }
    validate_output(tool_name, &patched).ok()?;
    Some(patched)
}

/// One row of a validated batch: its validated query, or its own issues.
pub(crate) type RowValidation = Result<serde_json::Value, ContractValidationError>;

/// Validate a fresh-query envelope once and isolate its invalid rows, so
/// valid rows can still execute. Rejected rows carry issue paths at their
/// original index. Isolation does not apply, and the whole call fails with
/// every issue, for a malformed envelope shape, every row invalid, or an envelope
/// issue, including the original batch size.
pub fn prepare_rows(
    tool_name: &str,
    input: serde_json::Value,
) -> Result<Vec<RowValidation>, ContractValidationError> {
    prepare_rows_from(tool_name, input, false)
}

/// Validate a complete request restored from the issued-cursor store.
pub fn prepare_saved_rows(
    tool_name: &str,
    input: serde_json::Value,
) -> Result<Vec<RowValidation>, ContractValidationError> {
    prepare_rows_from(tool_name, input, true)
}

fn prepare_rows_from(
    tool_name: &str,
    input: serde_json::Value,
    saved: bool,
) -> Result<Vec<RowValidation>, ContractValidationError> {
    let issues = prepare::envelope_issues(tool_name, &input, saved);
    if !issues.is_empty() {
        return Err(ContractValidationError { issues });
    }
    let (mut validated, batch) = validate::validate_batch(tool_name, input, saved)?;
    let rows = match validated.get_mut("queries") {
        Some(serde_json::Value::Array(rows)) => std::mem::take(rows),
        _ => Vec::new(),
    };
    let failed = batch
        .rows
        .iter()
        .filter(|issues| !issues.is_empty())
        .count();
    let whole = |batch: validate::BatchIssues| {
        let mut issues: Vec<_> = batch
            .rows
            .into_iter()
            .flatten()
            .chain(batch.envelope)
            .collect();
        issues.dedup();
        // A request that mixes forms is read as no form: that leads, and
        // the row issues of the form it was not are the details.
        issues
            .sort_by_key(|issue| !(issue.rule_id == "schema.union-mixed" && issue.path.is_empty()));
        ContractValidationError { issues }
    };
    if failed == 0 && batch.envelope.is_empty() {
        return Ok(rows.into_iter().map(Ok).collect());
    }
    if failed == 0 || failed == rows.len() || !batch.envelope.is_empty() {
        return Err(whole(batch));
    }
    Ok(rows
        .into_iter()
        .zip(batch.rows)
        .enumerate()
        .map(|(index, (row, mut issues))| {
            if issues.is_empty() {
                return Ok(row);
            }
            for issue in &mut issues {
                if issue.path.first().map(String::as_str) != Some("queries") {
                    issue
                        .path
                        .splice(0..0, ["queries".to_owned(), index.to_string()]);
                }
            }
            Err(ContractValidationError { issues })
        })
        .collect())
}

/// Validate a runtime-built continuation row, including its core-declared private progress.
pub fn validate_built_query(
    tool_name: &str,
    row: serde_json::Value,
) -> Result<serde_json::Value, ContractValidationError> {
    validate::validate_saved_query(tool_name, row)
}

/// Fingerprint of the tool contract `@octocodeai/config` generated from core.
#[cfg(test)]
const fn contract_fingerprint() -> &'static str {
    generated::CONTRACT_FINGERPRINT
}

/// Canonical generated contract JSON used by adapters for registration.
#[must_use]
pub const fn contract_json() -> &'static str {
    generated::CONTRACT_JSON
}

/// CLI discovery content (`schema` views, `<tool> --help` sections), parsed
/// on first use. Built from the same contract; the build checks the fingerprint.
pub fn cli_presentation() -> Result<&'static serde_json::Value, &'static str> {
    static PRESENTATION: std::sync::OnceLock<Result<serde_json::Value, String>> =
        std::sync::OnceLock::new();
    PRESENTATION
        .get_or_init(|| {
            serde_json::from_str(generated::CLI_PRESENTATION_JSON)
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(String::as_str)
}

/// The input schema `tool` publishes: its properties keep the order core
/// declares them in, which the tool contract's own schemas list by name.
pub(crate) fn published_input_schema(tool: ToolId) -> Option<&'static serde_json::Value> {
    let entry = cli_presentation().ok()?["tools"]
        .as_array()?
        .iter()
        .find(|entry| entry["name"] == tool.as_str())?;
    Some(&entry["mcp"]["inputSchema"])
}

/// `input` as the runtime runs it: the lenient readings its schema gives
/// applied before validation (`exclude: "x"` becomes `["x"]`, a bare row is
/// wrapped in `queries`). A caller that describes a finished call, such as
/// the CLI's text output, reads this rather than what was sent.
#[must_use]
pub fn input_as_run(tool_name: &str, input: serde_json::Value) -> serde_json::Value {
    normalized_input(tool_name, input).0
}

/// The `<tool> --help` section of the CLI presentation tool `name`.
#[must_use]
pub fn cli_tool_help(name: &str) -> Option<&'static str> {
    generated::CLI_TOOL_HELP
        .iter()
        .find(|(tool, _)| *tool == name)
        .map(|(_, help)| *help)
}

/// When to use the tool `name`: the first sentence of its `<when>` text.
#[must_use]
pub fn cli_tool_trigger(name: &str) -> Option<&'static str> {
    generated::CLI_TOOL_TRIGGERS
        .iter()
        .find(|(tool, _)| *tool == name)
        .map(|(_, when)| *when)
}

/// Provenance written by `@octocodeai/config`: core package version, contract
/// fingerprint, and the digest of the embedded contract bytes.
#[cfg(test)]
const fn contract_provenance_json() -> &'static str {
    generated::CONTRACT_PROVENANCE_JSON
}

#[cfg(test)]
mod contract_owner_tests;
