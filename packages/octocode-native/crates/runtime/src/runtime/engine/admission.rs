//! Call admission: the tool a surface may run, each row validated and
//! security-checked once, invalid rows isolated as indexed error rows with
//! their repair lead, and the input errors a rejected call answers with.

use super::{RuntimeError, ToolRuntime};
use crate::contracts;
use crate::id::ToolId;
use crate::response::{extras, rows};
use crate::security::{CheckedQuery, InputPolicy};
use octocode_contract_types::ErrorCode;

use serde_json::{Value, json};

/// An invalid row of an isolated batch, shaped like any other error row. Its
/// `hints.repair` is the one-row call with every field fixed, when each has
/// an exact fix.
fn rejected_row(
    tool: ToolId,
    index: usize,
    raw: Value,
    error: &contracts::ContractValidationError,
) -> (usize, Value) {
    let formatted = contracts::format_input_error(tool.as_str(), error);
    let mut data = json!({
        "error": formatted["error"],
        "errorCode": ErrorCode::InvalidInput,
    });
    let one_row = json!({ "queries": [raw] });
    let issues = error
        .issues
        .iter()
        .map(|issue| {
            let mut issue = issue.clone();
            if issue.path.first().is_some_and(|part| part == "queries") && issue.path.len() > 1 {
                issue.path[1] = "0".to_owned();
            }
            issue
        })
        .collect::<Vec<_>>();
    let mut hints = serde_json::Map::new();
    let mut text = formatted
        .get("details")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(repair) = repair_lead(tool, &one_row, &issues) {
        // The note names the caller's own row, not the one-row repair call.
        text.extend(repair.note(Some(index)).map(Value::String));
        hints.insert("repair".into(), repair.lead);
    }
    if !text.is_empty() {
        hints.insert("text".into(), Value::Array(text));
    }
    if !hints.is_empty() {
        data["hints"] = Value::Object(hints);
    }
    let mut row = rows::result_row(tool, index, &raw, data, Some("error"));
    extras::prune_row(&mut row, tool, &raw);
    (index, row)
}

/// A rejected input's `hints.repair` lead, and what a lossy or approximate
/// one changes.
struct RepairLead {
    lead: Value,
    /// The caller's fields the lead drops (paths); empty for an exact fix.
    dropped: Vec<String>,
    /// The fields the lead sets to a best reading (paths).
    approximate: Vec<String>,
}

impl RepairLead {
    /// The note a lossy or approximate lead needs: what it drops (a field
    /// the contract does not own, a list value it does not list, or one
    /// another form of the row declares) and which values it guesses. What
    /// a dropped constraint changes depends on the field (a filter widens,
    /// a section narrows), so the note names the paths only. A row field reads `queries[N].field`, as
    /// every repair message names a row; `row` renames the one-row repair
    /// call's `queries.0` to the caller's row.
    fn note(&self, row: Option<usize>) -> Option<String> {
        let rename = |paths: &[String]| {
            paths
                .iter()
                .map(|path| match path.strip_prefix("queries.0.") {
                    Some(rest) => format!("queries[{}].{rest}", row.unwrap_or(0)),
                    None => path.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut notes = Vec::new();
        if !self.dropped.is_empty() {
            notes.push(format!("The repair lead drops {}.", rename(&self.dropped)));
        }
        if !self.approximate.is_empty() {
            notes.push(format!(
                "The repair lead reads {} from the value as sent; a ref may contain '/', so check them before relying on the result.",
                rename(&self.approximate)
            ));
        }
        (!notes.is_empty()).then(|| notes.join(" "))
    }
}

/// The call that fixes a rejected input, when every issue has a fix and the
/// fixed call is admitted. An exact fix is `high` confidence; one that drops
/// a field the caller set (no contract field owns it) or guesses a value is
/// `low` and named, since it may read other than the call asked.
fn repair_lead(
    tool: ToolId,
    input: &Value,
    issues: &[contracts::ValidationIssue],
) -> Option<RepairLead> {
    // A lead is a continuation: canonical as written, and admitted as a call.
    // A candidate's own issues (rows lifted out of a nested envelope) are
    // repaired in a further pass; a clasify admission refusal is final.
    let repaired = contracts::repair_input(input, issues, |candidate| {
        let rows = contracts::prepare_many_and_validate(tool.as_str(), candidate.clone())
            .map_err(|error| error.issues)?;
        if tool.is_clasify() && crate::tools::clasify::admit(rows.iter()).is_err() {
            return Err(Vec::new());
        }
        Ok(())
    })?;
    let mut repaired = repaired;
    // The tool settles what the schema admits but it would not reach (an
    // artifactSearch page past the last its search reaches).
    if let Some(rows) = repaired.input["queries"].as_array_mut() {
        let output = tool.output();
        for row in rows {
            output.settle_repaired_row(row);
        }
    }
    let exact = repaired.dropped.is_empty() && repaired.approximate.is_empty();
    let lead = crate::response::result::Continuation::input(tool, repaired.input);
    Some(RepairLead {
        lead: if exact { lead } else { lead.approximate() }.build(),
        dropped: repaired.dropped,
        approximate: repaired.approximate,
    })
}

pub(super) fn invalid_input_error(
    tool: &str,
    input: &Value,
    error: contracts::ContractValidationError,
) -> RuntimeError {
    let mut payload = contracts::format_input_error(tool, &error);
    payload["errorCode"] = json!(ErrorCode::InvalidInput);
    if let Some(repair) =
        ToolId::from_name(tool).and_then(|id| repair_lead(id, input, &error.issues))
    {
        if let Some(note) = repair.note(None) {
            payload["hints"] = json!({ "text": [note] });
        }
        payload["hints"]["repair"] = repair.lead;
    }
    if let Some(id) = ToolId::from_name(tool) {
        crate::response::channels::publish_hints(&mut payload, id);
    }
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
            let nearest = crate::config::nearest_tool_name(tool)
                .filter(|name| self.is_available(name))
                .map(|name| format!(" Did you mean {name}?"))
                .unwrap_or_default();
            return Err(RuntimeError::new(
                ErrorCode::InvalidInput,
                format!(
                    "No tool is named {tool}.{nearest} Use one of {}.",
                    self.availability.available().join(", ")
                ),
            ));
        };
        if availability.is_available() {
            return Ok(id);
        }
        let reason = availability.reason_text();
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
    /// index. A batch isolates invalid rows; clasify (whose matrices share
    /// batch-level rules) and a call with no valid row fail as a whole.
    pub(super) fn admit_queries(
        &self,
        id: ToolId,
        input: &Value,
    ) -> Result<AdmittedRows, RuntimeError> {
        let tool = id.as_str();
        let PreparedRows {
            valid: prepared,
            mut invalid,
        } = prepare_queries(tool, input)?;
        if id.is_clasify() {
            crate::tools::clasify::admit(prepared.iter().map(|(_, query)| query))
                .map_err(|error| invalid_input_error(tool, input, error))?;
        }
        let bulk = input.is_array() || input.get("queries").is_some();
        let mut queries = Vec::with_capacity(prepared.len());
        for (index, query) in prepared {
            let prefix = if bulk {
                vec!["queries".to_owned(), index.to_string()]
            } else {
                Vec::new()
            };
            // What the row asks of the runtime, read before the gate rewrites
            // it in place.
            let mut later = Vec::new();
            if query.get("clasifyContext").is_some() && !self.selection_available {
                let mut path = prefix.clone();
                path.push("clasifyContext".into());
                later.push(crate::contracts::ValidationIssue {
                    rule_id: "clasifyContext.available".into(),
                    schema: None,
                    received: None,
                    path,
                    message: "clasifyContext requires an available classification provider; configure JEV or omit the context.".into(),
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
                    state_limit: crate::tools::clasify::max_state_chars(
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
        if !invalid.is_empty() && (queries.is_empty() || id.is_clasify()) {
            let issues = invalid
                .into_iter()
                .flat_map(|(_, error)| error.issues)
                .collect();
            return Err(invalid_input_error(
                tool,
                input,
                contracts::ContractValidationError { issues },
            ));
        }
        let raw_rows = input
            .get("queries")
            .or(Some(input))
            .and_then(Value::as_array);
        let rejected = invalid
            .iter()
            .map(|(index, error)| {
                let raw = raw_rows
                    .and_then(|rows| rows.get(*index))
                    .cloned()
                    .unwrap_or(Value::Null);
                rejected_row(id, *index, raw, error)
            })
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
fn prepare_queries(tool: &str, input: &Value) -> Result<PreparedRows, RuntimeError> {
    let rows = contracts::prepare_rows(tool, input.clone())
        .map_err(|error| invalid_input_error(tool, input, error))?;
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
