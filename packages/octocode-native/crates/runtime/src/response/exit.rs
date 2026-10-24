//! Exit classification: how a finished call ended, decided once from its
//! public response so every interface reports the same status.

use super::outcome::ToolOutcome;
use super::result::{FailureKind, failure_kind};
use super::rows::is_invalid_input_code;
use crate::id::error_codes::{self, ErrorClass};
use octocode_contract_types::ErrorCode;
use serde_json::Value;

/// How a call ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitClass {
    /// Results, whether or not a page, a partial source read or a clasify walk
    /// remains: more to read is a continuation (`next.*`, `pagination.hasMore`),
    /// never a different outcome.
    Success,
    /// Every row ran and found nothing, even when it offers a recovery call.
    Empty,
    /// The call, or any row of it, rejected the caller's input.
    InvalidInput,
    /// At least one row failed.
    Failed(FailureKind),
}

impl ToolOutcome {
    #[must_use]
    pub fn exit_class(&self) -> ExitClass {
        let value = &self.structured_content;
        let rows: &[Value] = value["results"].as_array().map_or(&[], Vec::as_slice);
        if self.all_failed {
            // A rejected row ends the call as rejected input, as in a batch
            // where other rows answered.
            if rows.iter().any(is_invalid_input_row) {
                return ExitClass::InvalidInput;
            }
            return match self.failure {
                Some(kind) => ExitClass::Failed(kind),
                // A row a later stage failed (a provider page whose
                // classification failed) states its code; the same code
                // ends the call the same way on every tool. A refusal with
                // no row (a gate or admission-time validation) is the
                // caller's request, never success.
                None => failed_rows(rows).map_or(ExitClass::InvalidInput, ExitClass::Failed),
            };
        }
        // Batch precedence, worst row first: a rejected row (2), a failed
        // row (its shared failure, else execution; a retryable failure is
        // still a failure), an empty batch (1; a page past the end reads
        // nothing either), success (0). An answering row
        // never hides a failed one.
        if rows.iter().any(is_invalid_input_row) {
            ExitClass::InvalidInput
        } else if let Some(kind) = failed_rows(rows) {
            ExitClass::Failed(kind)
        } else if !rows.is_empty()
            && rows
                .iter()
                .all(|row| matches!(row["status"].as_str(), Some("empty" | "outOfRange")))
        {
            ExitClass::Empty
        } else {
            ExitClass::Success
        }
    }
}

impl ExitClass {
    /// How a call that failed with `code` ended: the contract's class of the
    /// code (`errorCodes`), whichever interface or command reports it.
    #[must_use]
    pub fn of(code: ErrorCode) -> Self {
        match error_codes::class(code) {
            ErrorClass::InvalidInput => Self::InvalidInput,
            _ => Self::Failed(failure_kind(code)),
        }
    }
}

/// An error row whose `errorCode` rejects the caller's input.
fn is_invalid_input_row(row: &Value) -> bool {
    row["status"] == "error"
        && row
            .pointer("/data/errorCode")
            .and_then(Value::as_str)
            .is_some_and(is_invalid_input_code)
}

/// The failure of a batch's error rows: their shared failure, else an
/// execution failure.
fn failed_rows(rows: &[Value]) -> Option<FailureKind> {
    shared_failure(
        rows.iter()
            .filter(|row| row["status"] == "error")
            .map(|row| {
                row.pointer("/data/errorCode")
                    .and_then(Value::as_str)
                    .and_then(error_codes::parse)
                    .map_or(FailureKind::Execution, failure_kind)
            }),
    )
}

fn shared_failure(mut kinds: impl Iterator<Item = FailureKind>) -> Option<FailureKind> {
    let first = kinds.next()?;
    Some(if kinds.all(|kind| kind == first) {
        first
    } else {
        FailureKind::Execution
    })
}

#[cfg(test)]
mod tests;
