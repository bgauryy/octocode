//! Public response stage: the one boundary between executed rows and the
//! envelope a caller receives. Execution (dispatch, selection inference)
//! finishes first; this stage owns the order of output-row isolation,
//! continuation shaping, paging, and final contract validation. It returns
//! JSON only, identically for every surface: MCP, CLI JSON, and the CLI's
//! views, which print from the same envelope.

use super::page_model::ResponsePageOptions;
use super::pager::{ResponseError, ResponseInput, SavedPages};
use crate::contracts::{self, ContractValidationError};
use crate::execution::{ExecutionContext, ExecutionError};
use crate::id::ToolId;
use crate::response::outcome::ToolOutcome;
use crate::response::result::FailureKind;
use crate::security::Sanitized;
use serde_json::Value;

pub(crate) struct StageInput {
    pub tool: ToolId,
    /// The `{results}` envelope after evidence selection, sealed by the
    /// output gate; the stage reshapes it only through [`Sanitized::edit`]
    /// and [`Sanitized::cut`].
    pub structured: Sanitized,
    pub options: ResponsePageOptions,
    pub failure: Option<FailureKind>,
    pub auto_page_chars: usize,
    /// False only when saved-result storage failed and the complete result
    /// must be returned without an unusable display continuation.
    pub(crate) allow_auto_paging: bool,
}

pub(crate) struct Staged {
    pub result: Result<ToolOutcome, ContractValidationError>,
    pub next: Option<super::cursor::Cursor>,
}

/// The page-independent half of the stage: selection receipts, line
/// numbers, output validation (row violations isolated, marking `failure`)
/// and continuation shaping. Returns the remaining-notes the pager words.
/// A settled envelope is what a page replay stores, so its later pages are
/// neither validated nor shaped again.
pub(crate) fn settle(
    tool: ToolId,
    structured: &mut Sanitized,
    response_query: &Value,
    failure: &mut Option<FailureKind>,
) -> Result<Vec<Vec<super::result::Remaining>>, ContractValidationError> {
    structured.edit(|structured| settle_value(tool, structured, response_query, failure))
}

fn settle_value(
    tool: ToolId,
    structured: &mut Value,
    response_query: &Value,
    failure: &mut Option<FailureKind>,
) -> Result<Vec<Vec<super::result::Remaining>>, ContractValidationError> {
    crate::response::selection_errors::decorate(tool, structured, response_query);
    super::notices::place(structured, "results");
    // The notes stay with the pager for the pages a split row moves.
    let remaining = super::pages::take_remaining_notes(structured);
    // Hosts show agents this JSON: source reads carry their line numbers in
    // `content` itself. Numbering precedes validation so an unpaged
    // response is exactly what was checked.
    crate::response::numbered::number_read_rows(tool, structured);
    super::pages::publish_rows(structured);
    // Validate the complete, sanitized rows before deriving error state or a
    // pagination snapshot from them.
    if isolate_output_rows(tool.as_str(), structured)? && failure.is_none() {
        *failure = Some(FailureKind::Execution);
    }
    Ok(remaining)
}

/// The page-dependent half of the stage over a [`settle`]d envelope: paging
/// and the paged shape's contract check. `keep` (a first execution's replay
/// seed) receives the settled envelope only when the response pages, so a
/// response that fits one page is never copied.
pub(crate) fn respond(
    input: StageInput,
    remaining: Vec<Vec<super::result::Remaining>>,
    keep: Option<&mut Option<Sanitized>>,
    context: &ExecutionContext,
) -> Result<Staged, ExecutionError> {
    let StageInput {
        tool,
        structured,
        mut options,
        failure,
        auto_page_chars,
        allow_auto_paging,
        ..
    } = input;
    let all_failed = response_all_failed(&structured);
    if allow_auto_paging {
        options.auto_paginate(&structured, auto_page_chars);
    }
    if options.paging
        && let Some(slot) = keep
    {
        *slot = Some(structured.clone());
    }
    context.check()?;
    let reshaped = options.paging;
    let mut structured_content = structured;
    let mut next = None;
    structured_content
        .edit(|structured| {
            let prepared = super::pager::prepare(
                ResponseInput {
                    tool,
                    structured: std::mem::take(structured),
                    options,
                    remaining,
                },
                &context.cancellation,
            )?;
            *structured = prepared.value;
            next = prepared.next;
            Ok(())
        })
        .map_err(response_failure)?;
    context.check()?;
    Ok(Staged {
        result: finish(tool, structured_content, reshaped, failure, all_failed),
        next,
    })
}

/// A later page of a saved response: cut from `pages`, the memo built from
/// its settled `envelope` on the first read, so the envelope is neither
/// copied nor re-planned per page. `input.structured` is unused.
pub(crate) fn respond_saved(
    input: StageInput,
    envelope: &Sanitized,
    remaining: &[Vec<super::result::Remaining>],
    pages: &SavedPages,
    context: &ExecutionContext,
) -> Result<Staged, ExecutionError> {
    let StageInput {
        tool,
        options,
        failure,
        ..
    } = input;
    let all_failed = response_all_failed(envelope);
    context.check()?;
    let reshaped = options.paging;
    let mut next = None;
    let structured_content = envelope
        .cut(|envelope| {
            let prepared = super::pager::prepare_saved(
                &ResponseInput {
                    tool,
                    structured: Value::Null,
                    options,
                    remaining: remaining.to_vec(),
                },
                envelope,
                pages,
                &context.cancellation,
            )?;
            next = prepared.next;
            Ok(prepared.value)
        })
        .map_err(response_failure)?;
    context.check()?;
    Ok(Staged {
        result: finish(tool, structured_content, reshaped, failure, all_failed),
        next,
    })
}

/// The paged shape's contract check, then the outcome. Page shaping crosses
/// the public contract as well; rows validated before paging and left
/// unpaged are not validated twice.
fn finish(
    tool: ToolId,
    mut structured_content: Sanitized,
    reshaped: bool,
    failure: Option<FailureKind>,
    all_failed: bool,
) -> Result<ToolOutcome, ContractValidationError> {
    let paged = reshaped || structured_content.get("responsePagination").is_some();
    if paged {
        structured_content
            .edit(|structured| contracts::validate_output_in_place(tool.as_str(), structured))?;
    }
    Ok(ToolOutcome {
        structured_content,
        failure,
        all_failed,
    })
}

fn response_failure(error: ResponseError) -> ExecutionError {
    match error {
        ResponseError::Cancelled => ExecutionError::Cancelled,
        ResponseError::StructuredContentMustBeObject
        | ResponseError::Unserializable
        | ResponseError::InvalidPage
        | ResponseError::InvalidEvidence(_) => ExecutionError::WorkerFailed,
    }
}

/// Repair malformed result rows before the response is paged.
/// Envelope violations remain fatal so pagination never snapshots invalid data.
pub(crate) fn isolate_output_rows(
    tool: &str,
    structured: &mut Value,
) -> Result<bool, ContractValidationError> {
    let Err(error) = contracts::validate_output_in_place(tool, structured) else {
        return Ok(false);
    };
    let Some(patched) = contracts::isolate_row_violations(tool, structured, &error) else {
        return Err(error);
    };
    *structured = patched;
    // These errors are created here, after ordinary rows received their
    // recovery guidance. Keep contract failures actionable as well: the
    // violation is the runtime's, so the shared recovery answers it.
    if ToolId::from_name(tool).is_some()
        && let Some(rows) = structured.get_mut("results").and_then(Value::as_array_mut)
    {
        for row in rows {
            if crate::id::error_code_of(&row["data"])
                == Some(octocode_contract_types::ErrorCode::OutputContractViolation)
            {
                super::rows::attach_error_hint(
                    &crate::response::output::SharedOutput,
                    &mut row["data"],
                    &Value::Null,
                );
            }
        }
    }
    Ok(true)
}

pub(crate) fn response_all_failed(structured: &Value) -> bool {
    structured
        .get("results")
        .and_then(Value::as_array)
        .is_some_and(|rows| !rows.is_empty() && rows.iter().all(|row| row["status"] == "error"))
}

#[cfg(test)]
mod tests;
