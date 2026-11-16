//! Exit classification: how a finished call ended, decided once from its
//! public response so every interface reports the same status.

use super::outcome::ToolOutcome;
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
    /// At least one row failed; the class is the strictest of the failed rows.
    Failed(ErrorClass),
}

/// Batch precedence, strictest first: the caller must correct the request,
/// then a refusal, then a restart, then an unservable request; a call whose
/// every failure is transient is the only one a retry can help.
const fn rank(class: ErrorClass) -> u8 {
    match class {
        ErrorClass::CorrectInput => 4,
        ErrorClass::Refused => 3,
        ErrorClass::Restart => 2,
        ErrorClass::Unservable => 1,
        ErrorClass::Transient => 0,
    }
}

/// The strictest of `classes`; `None` when there are none.
pub(crate) fn strictest(classes: impl IntoIterator<Item = ErrorClass>) -> Option<ErrorClass> {
    classes.into_iter().max_by_key(|class| rank(*class))
}

impl ToolOutcome {
    /// Whether the call is a tool error on a surface that has one flag (MCP
    /// `isError`): every row failed. A batch where some row answered is not
    /// an error there, though [`Self::exit_class`] still reports its failed
    /// row. Both read the same `all_failed`, so no surface decides it alone.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.all_failed
    }

    #[must_use]
    pub fn exit_class(&self) -> ExitClass {
        let value = &self.structured_content;
        let rows: &[Value] = value["results"].as_array().map_or(&[], Vec::as_slice);
        // A row a later stage failed states its code; the same code ends the
        // call the same way on every tool. A refusal with no row (a gate or
        // admission-time validation) is the caller's request, never success.
        let failed = failed_rows(rows);
        let stage = if self.all_failed {
            self.failure
                .or_else(|| failed.is_none().then_some(ErrorClass::CorrectInput))
        } else {
            None
        };
        if let Some(class) = strictest(failed.into_iter().chain(stage)) {
            return ExitClass::Failed(class);
        }
        if !rows.is_empty()
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
        Self::Failed(error_codes::class(code))
    }
}

/// The strictest class among a batch's error rows.
fn failed_rows(rows: &[Value]) -> Option<ErrorClass> {
    strictest(
        rows.iter()
            .filter(|row| row["status"] == "error")
            .map(|row| {
                row.pointer("/data/errorCode")
                    .and_then(Value::as_str)
                    .map_or(ErrorClass::Unservable, crate::id::error_class)
            }),
    )
}

#[cfg(test)]
mod tests;
