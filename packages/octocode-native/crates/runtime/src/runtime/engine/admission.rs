//! Call admission: the tool a surface may run, each row validated and
//! security-checked once, invalid rows isolated as indexed error rows with
//! their repair lead, and the input errors a rejected call answers with.

use super::{RuntimeError, ToolRuntime};
use crate::contracts;
use crate::id::ToolId;
use crate::response::rows;
use crate::security::{CheckedQuery, InputPolicy};
use octocode_contract_types::ErrorCode;

use serde_json::{Value, json};

fn rejected_row(
    tool: ToolId,
    index: usize,
    error: &contracts::ContractValidationError,
) -> (usize, Value) {
    let formatted = contracts::format_input_error(tool.as_str(), error);
    let mut data = json!({"error": formatted["error"], "errorCode": ErrorCode::InvalidInput});
    if let Some(details) = formatted
        .get("details")
        .filter(|details| details.as_array().is_some_and(|items| !items.is_empty()))
    {
        data["hints"] = details.clone();
    }
    (index, rows::result_row(index, data, Some("error")))
}

pub(super) fn invalid_input_error(
    tool: &str,
    error: contracts::ContractValidationError,
) -> RuntimeError {
    let mut payload = contracts::format_input_error(tool, &error);
    payload["errorCode"] = json!(ErrorCode::InvalidInput);
    RuntimeError {
        code: ErrorCode::InvalidInput,
        message: error.to_string(),
        payload: Some(Box::new(payload)),
        validation_issues: Some(error.issues),
    }
}

pub(super) fn output_contract_error(
    tool: &str,
    error: contracts::ContractValidationError,
) -> RuntimeError {
    let details = error
        .issues
        .iter()
        .map(|issue| format!("{}: {}", issue.path.join("."), issue.message))
        .collect::<Vec<_>>()
        .join("; ");
    RuntimeError {
        code: ErrorCode::OutputContractViolation,
        message: format!(
            "{tool} produced a response that violates its canonical output contract: {details}"
        ),
        payload: None,
        validation_issues: Some(error.issues),
    }
}

impl ToolRuntime {
    /// The tool behind `tool` when this surface may run it, else the
    /// actionable reason it may not.
    pub(super) fn admit_tool(&self, tool: &str, mcp: bool) -> Result<ToolId, RuntimeError> {
        // Contract `cliOnly` tools run only from the CLI. MCP never executes
        // them, even if an embedder used a CLI host surface. A parked tool
        // says so on every surface.
        let requested = ToolId::from_name(tool);
        if mcp && requested.is_some_and(|id| id.is_cli_only() && !id.is_disabled()) {
            return Err(RuntimeError::new(
                ErrorCode::ToolUnavailable,
                format!("Tool {tool} is not available through MCP; run `octocode {tool}`."),
            ));
        }
        let (Some(id), Some(availability)) = (requested, self.availability(tool)) else {
            return Err(RuntimeError::new(
                ErrorCode::InvalidInput,
                format!(
                    "No tool is named {tool}. Use one of {}.",
                    self.availability.available().join(", ")
                ),
            ));
        };
        if availability.is_available() {
            return Ok(id);
        }
        let reason = availability.reason_text();
        if let crate::execution::availability::Availability::ProviderUnavailable { code, .. } =
            availability
        {
            return Err(RuntimeError::new(
                *code,
                format!("Tool {tool} is not available: {reason}"),
            ));
        }
        if !mcp && availability.needs_opt_in() {
            let mut message = format!("Tool {tool} is not available: {reason}");
            if id == ToolId::Clasify
                && crate::execution::availability::classification_key(&self.config).is_none()
            {
                message.push_str(&format!(
                    " Provider setup: {}",
                    self.classification.defaults().docs_url
                ));
            }
            return Err(RuntimeError::new(ErrorCode::MissingConfiguration, message));
        }
        Err(RuntimeError::new(
            ErrorCode::ToolUnavailable,
            format!("Tool {tool} is not available in this native runtime: {reason}"),
        ))
    }

    /// Validated, security-checked rows to run, and the rows rejected by
    /// index. Every tool isolates invalid rows; a call with no valid row
    /// fails as a whole.
    pub(super) fn admit_queries(
        &self,
        id: ToolId,
        input: &Value,
        saved: bool,
    ) -> Result<AdmittedRows, RuntimeError> {
        let tool = id.as_str();
        let PreparedRows {
            valid: prepared,
            mut invalid,
        } = prepare_queries(tool, input, saved)?;
        let mut queries = Vec::with_capacity(prepared.len());
        for (index, query) in prepared {
            let prefix = vec!["queries".to_owned(), index.to_string()];
            // What the row asks of the runtime, read before the gate rewrites
            // it in place.
            let mut later = Vec::new();
            // A selection plan (sealed in a cursor) is classification's own page.
            let judged = ["classification", "selection"]
                .into_iter()
                .find(|field| query.get(*field).is_some());
            if let Some(field) = judged.filter(|_| !self.selection_available) {
                let mut path = prefix.clone();
                path.push(field.into());
                later.push(crate::contracts::ValidationIssue {
                    rule_id: "classification.available".into(),
                    schema: None,
                    received: None,
                    path,
                    message: "classification requires an available classification provider; configure JEV or omit the context.".into(),
                });
            }
            if id.is_clasify() {
                later.extend(crate::tools::clasify::unavailable_source_issues(
                    &query,
                    &prefix,
                    &self.availability,
                ));
            }
            let policy = if id.is_clasify() {
                InputPolicy::ClasifyEvidence {
                    content_limit: crate::tools::clasify::max_state_chars(
                        query["longContext"].as_bool() == Some(true),
                    ),
                }
            } else {
                InputPolicy::Query
            };
            let (checked, mut issues) = match self.security.check_input(query, policy) {
                Ok(checked) => (Some(checked), Vec::new()),
                Err(mut issues) => {
                    for issue in &mut issues {
                        issue.path.splice(0..0, prefix.iter().cloned());
                    }
                    (None, issues)
                }
            };
            issues.extend(later);
            match checked {
                Some(checked) if issues.is_empty() => queries.push(checked),
                _ => invalid.push((index, contracts::ContractValidationError { issues })),
            }
        }
        invalid.sort_by_key(|(index, _)| *index);
        if !invalid.is_empty() && queries.is_empty() {
            let issues = invalid
                .into_iter()
                .flat_map(|(_, error)| error.issues)
                .collect();
            return Err(invalid_input_error(
                tool,
                contracts::ContractValidationError { issues },
            ));
        }
        let rejected = invalid
            .iter()
            .map(|(index, error)| rejected_row(id, *index, error))
            .collect();
        Ok(AdmittedRows { queries, rejected })
    }
}

/// A call's rows that passed admission, and the rejected ones as indexed
/// error rows.
pub(super) struct AdmittedRows {
    pub(super) queries: Vec<CheckedQuery>,
    pub(super) rejected: Vec<(usize, Value)>,
}

/// A batch's contract-validated rows by input index, and the invalid ones.
struct PreparedRows {
    valid: Vec<(usize, Value)>,
    invalid: Vec<(usize, contracts::ContractValidationError)>,
}

/// Contract-validated rows by input index, and the invalid ones. Row
/// isolation: when only some rows of a batch are invalid, the valid rows
/// still run and the rest become indexed error rows.
fn prepare_queries(tool: &str, input: &Value, saved: bool) -> Result<PreparedRows, RuntimeError> {
    let rows = if saved {
        contracts::prepare_saved_rows(tool, input.clone())
    } else {
        contracts::prepare_rows(tool, input.clone())
    }
    .map_err(|error| invalid_input_error(tool, error))?;
    let mut prepared = PreparedRows {
        valid: Vec::with_capacity(rows.len()),
        invalid: Vec::new(),
    };
    for (index, row) in rows.into_iter().enumerate() {
        match row {
            Ok(query) => prepared.valid.push((index, query)),
            Err(error) => prepared.invalid.push((index, error)),
        }
    }
    Ok(prepared)
}

#[cfg(test)]
mod security_issue_tests;
