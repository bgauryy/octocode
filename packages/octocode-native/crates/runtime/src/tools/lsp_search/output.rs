//! lspSearch output facts for the shared response stages.
use crate::response::output::{LIST_FILES_HINT, NO_HINT, ToolOutput};
use octocode_contract_types::ErrorCode;
use serde_json::Value;

pub(crate) struct Output;

/// What to try after an lspSearch call timed out: the retry is the row's own
/// query (the server keeps loading between calls); the rest works without
/// the server. The one Timeout tip, on a row and on a request-level failure.
pub(super) const TIMEOUT_TIP: &str = "Retry once: the first call initialises the language server (~60 s cold start); else astSearch/localSearch + localFetch.";

/// The query's `operation` and whether it names a `path`: the two facts the
/// tips depend on.
fn shape(query: &Value) -> (&str, bool) {
    (
        query.get("operation").and_then(Value::as_str).unwrap_or(""),
        query.get("path").is_some_and(Value::is_string),
    )
}

/// The tip of a failure code, on a failed row and on a request-level
/// failure alike (the one place lspSearch failure tips are written);
/// [`NO_HINT`] when the row's leads and per-call notes say it all
/// (`anchorUnresolved`: `findSimilar`/`read`). A code shared by retryable
/// and final causes names the row's `retryable` flag; the message names the
/// cause.
fn failure_hint(code: ErrorCode, query: &Value) -> Option<&'static str> {
    let (operation, has_path) = shape(query);
    Some(match code {
        ErrorCode::InvalidInput => {
            "Correct the lspSearch fields the error names (see `octocode schema lspSearch`), then retry."
        }
        // A stale or cancelled request is retryable; a relation the source
        // could not establish, or a request the server rejected, is final.
        ErrorCode::RequestFailed => {
            "Retry once if retryable; else confirm in source: locate with astSearch/localSearch, then localFetch it."
        }
        // A closed connection restarts the server; a memory kill repeats,
        // and the row's text lead is all that is left.
        ErrorCode::ServerCrashed => {
            "Retry once if retryable (it restarts); after a memory kill use the unverified text lead, or astSearch + localFetch."
        }
        // The server holds no build of the file (clangd: no compile
        // command); retrying repeats it, project setup changes it.
        ErrorCode::MissingConfiguration => {
            "Configure the server's project (clangd: compile_commands.json, compile_flags.txt or .clangd); the next call reloads it."
        }
        ErrorCode::NotAFile
        | ErrorCode::BinaryFileUnsupported
        | ErrorCode::PathValidationFailed => LIST_FILES_HINT,
        ErrorCode::ServerUnavailable if operation == "workspaceSymbol" && !has_path => {
            "Provide path for a representative workspace source file so Octocode can select its language server."
        }
        ErrorCode::ServerUnavailable if has_path => {
            "Use the related source read or text search; semantic results remain unavailable. Retry LSP only after configuring a server for this file."
        }
        ErrorCode::ServerUnavailable => {
            "Use astSearch symbols/match or localSearch for candidates, then localFetch exact source."
        }
        ErrorCode::CapabilityUnavailable if matches!(operation, "supertypes" | "subtypes") => {
            "This server cannot prove type hierarchy; inspect declarations with astSearch and confirm exact source."
        }
        ErrorCode::CapabilityUnavailable => {
            "Use the advertised LSP operations, or fall back to astSearch/localSearch and exact source reads."
        }
        ErrorCode::Timeout => TIMEOUT_TIP,
        ErrorCode::FileTooLarge => {
            "Use localSearch or astSearch to locate a bounded region, then read that exact source range."
        }
        ErrorCode::AnchorUnresolved => NO_HINT,
        _ => return None,
    })
}

impl ToolOutput for Output {
    fn error_hint(&self, code: ErrorCode, query: &Value) -> Option<&'static str> {
        failure_hint(code, query)
    }
}
