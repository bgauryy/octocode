//! Generated public tool contracts and transport-neutral input preparation.

pub(crate) mod generated;
pub(crate) mod listing_roles;
mod prepare;
mod private_state;
pub mod query_limits;
pub(crate) use private_state::state_fields;
pub(crate) mod output_fields;
mod schema_facts;
#[cfg(test)]
mod undeclared;
mod validate;
use octocode_contract_types::ErrorCode;
pub(crate) use validate::levenshtein;

use crate::id::ToolId;
pub(crate) use schema_facts::{
    child_schemas, item_schemas, property_schemas, query_declares, query_schema_max,
    query_schema_number, query_schema_value, resolve_ref,
};
pub(crate) use validate::validate_output_in_place;
pub use validate::{
    ContractValidationError, ValidationIssue, format_input_error, tool_error, validate,
    validate_output, validate_query,
};

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
pub(crate) fn classification_notice(key: &str) -> &'static str {
    parsed_contract()
        .ok()
        .and_then(|contract| contract.pointer(&format!("/classificationNotices/{key}")))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

/// One tool's contract entry (`tools[i]`), parsed on first use from its
/// build-verified span of the embedded contract. A tool call touches one tool,
/// so a fresh process parses that entry (~100 KB) instead of the whole
/// multi-megabyte contract — the dominant cost of a short-lived CLI call.
/// Reuses the full parse when something (catalog, scheme) already forced it.
pub(crate) fn tool_contract(tool: ToolId) -> Result<&'static serde_json::Value, &'static str> {
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

/// [`tool_contract`] by wire name; `None` for a name the contract lacks.
pub(crate) fn tool_contract_named(
    name: &str,
) -> Option<Result<&'static serde_json::Value, &'static str>> {
    ToolId::from_name(name).map(tool_contract)
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

/// Validate a single internal query in tests through the same saved-request path.
#[cfg(test)]
pub(crate) fn prepare_and_validate(
    tool_name: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, ContractValidationError> {
    validate_built_query(tool_name, input)
}

#[cfg(test)]
pub(crate) fn prepare_many_built(
    tool_name: &str,
    input: serde_json::Value,
) -> Result<Vec<serde_json::Value>, ContractValidationError> {
    prepare_saved_rows(tool_name, input)?.into_iter().collect()
}

#[cfg(test)]
pub(crate) fn validate_built(
    tool_name: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, ContractValidationError> {
    validate::validate_saved(tool_name, input)
}

/// One row of a validated batch: its validated query, or its own issues.
pub(crate) type RowValidation = Result<serde_json::Value, ContractValidationError>;

/// Validate a fresh-query envelope once and isolate its invalid rows, so
/// valid rows can still execute. Rejected rows carry issue paths at their
/// original index. Isolation does not apply, and the whole call fails with
/// every issue, for a malformed envelope shape, every row invalid, or an envelope
/// issue, including the original batch size.
pub(crate) fn prepare_rows(
    tool_name: &str,
    input: serde_json::Value,
) -> Result<Vec<RowValidation>, ContractValidationError> {
    prepare_rows_from(tool_name, input, false)
}

/// Validate a complete request restored from the issued-cursor store.
pub(crate) fn prepare_saved_rows(
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
    let issues = prepare::envelope_issues(tool_name, &input);
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

/// Validate a fresh-query or saved-response envelope and return its query rows.
/// Cursor reads have no query rows. The envelope's shape
/// is checked first (`prepare::envelope_issues`: a bare array or a nested
/// envelope is rejected with a repairable issue, never reshaped); then the
/// complete envelope validates, so cross-query limits and indexed
/// diagnostics remain contract-owned. Clasify's batch relation rules run
/// at engine admission (`tools::clasify::admission`).
pub fn prepare_many_and_validate(
    tool_name: &str,
    input: serde_json::Value,
) -> Result<Vec<serde_json::Value>, ContractValidationError> {
    let issues = prepare::envelope_issues(tool_name, &input);
    if !issues.is_empty() {
        return Err(ContractValidationError { issues });
    }
    match validate(tool_name, input)? {
        serde_json::Value::Object(mut validated) => match validated.remove("queries") {
            Some(serde_json::Value::Array(queries)) => Ok(queries),
            _ => Ok(Vec::new()),
        },
        _ => Ok(Vec::new()),
    }
}

/// Validate a runtime-built continuation row, including its core-declared private progress.
pub(crate) fn validate_built_query(
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

/// The `<tool> --help` section of the CLI presentation tool `name`.
#[must_use]
pub fn cli_tool_help(name: &str) -> Option<&'static str> {
    generated::CLI_TOOL_HELP
        .iter()
        .find(|(tool, _)| *tool == name)
        .map(|(_, help)| *help)
}

/// Provenance written by `@octocodeai/config`: core package version, contract
/// fingerprint, and the digest of the embedded contract bytes.
#[cfg(test)]
const fn contract_provenance_json() -> &'static str {
    generated::CONTRACT_PROVENANCE_JSON
}

#[cfg(test)]
mod contract_owner_tests;
