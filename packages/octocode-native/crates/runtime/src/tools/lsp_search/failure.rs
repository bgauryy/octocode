//! Typed failures, empty rows, partial marking, and executable `next.*`
//! continuations shared by every `lspSearch` operation.

use super::LspSearchQuery;
use super::output::{TIMEOUT_TIP, empty_hint, failure_hint};
use super::render::uri_to_path;
use crate::id::ToolId;
use crate::response::result::Confidence;
use crate::tools::lsp_search::lsp_search_query;
use octocode_contract_types::ErrorCode;
use octocode_engine::error::{Error as EngineError, ErrorCode as RpcCode, ErrorKind};
use serde_json::{Value, json};

mod anchors;
mod leads;
mod paging;
mod rows;
pub(super) use anchors::*;
pub(super) use leads::*;
pub(super) use paging::*;
pub(super) use rows::*;

/// A request-level failure `execute` could not render as a row. The
/// dispatcher emits `code`/`message`/`hint`/`retryable` verbatim, so each
/// cause keeps its own recovery instead of one generic "unavailable" code.
#[derive(Clone, Debug, PartialEq)]
pub struct LspFailure {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub hint: &'static str,
    /// The language server cannot answer this scope at all (it was stopped
    /// for its memory), so the row leads to an unverified text search
    /// instead of a retry.
    pub lexical_fallback: bool,
}

const RETRY_OR_FALL_BACK: &str =
    "Retry once; if it persists, re-anchor from localFetch source or use astSearch/localSearch.";

/// The fix for a C-family file clangd cannot build an AST for: tell clangd
/// how this file is compiled. Retrying without that repeats the failure; with
/// it, the next call restarts the stale server by itself.
const INVALID_AST_HINT: &str = "Give clangd a compile command (compile_commands.json, compile_flags.txt or .clangd); the next call reloads it.";

impl LspFailure {
    pub(super) fn invalid_query(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidInput,
            message: message.into(),
            retryable: false,
            hint: "Correct the lspSearch fields (see `octocode schema lspSearch`), then retry.",
            lexical_fallback: false,
        }
    }

    /// A relation the source and the server's answers could not establish
    /// (an unresolved base, an ambiguous candidate): asking again gives the
    /// same answer, so it is never retryable.
    pub(super) fn unestablished(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::RequestFailed,
            message: message.into(),
            retryable: false,
            hint: "Confirm the relation in source: locate the declarations with astSearch and read them with localFetch.",
            lexical_fallback: false,
        }
    }

    /// A request left unanswered past the time a cheaper labelled answer
    /// was willing to wait for it.
    pub(super) fn overdue(message: String) -> Self {
        Self {
            code: ErrorCode::Timeout,
            message,
            retryable: true,
            hint: TIMEOUT_TIP,
            lexical_fallback: false,
        }
    }

    /// The caller cancelled or the deadline expired; the dispatcher reports
    /// the runtime cancellation outcome instead of this row.
    pub(super) fn cancelled(message: String) -> Self {
        Self {
            code: ErrorCode::Cancelled,
            message,
            retryable: false,
            hint: "The request was cancelled or exceeded its deadline.",
            lexical_fallback: false,
        }
    }

    /// Whether the request's cancel or deadline caused this failure: such a
    /// failure ends the request instead of falling back.
    pub(super) fn is_cancelled(&self) -> bool {
        self.code == ErrorCode::Cancelled
    }

    /// Path-policy refusal: the same code local reads use, so agents route it
    /// as a sandbox/path problem rather than a missing language server.
    pub(super) fn path_denied(error: crate::policy::PolicyError) -> Self {
        use crate::policy::PolicyErrorCode;
        let hint = match error.code {
            PolicyErrorCode::OutsideAllowedRoots | PolicyErrorCode::SymlinkEscape => {
                crate::response::output::SANDBOX_HINT
            }
            _ => crate::response::output::LIST_FILES_HINT,
        };
        Self {
            code: error.shared_code(),
            message: error.message,
            retryable: false,
            hint,
            lexical_fallback: false,
        }
    }

    /// Classify an engine error by its typed kind (never by message text):
    /// a timeout, a server that exited, an unimplemented method, a stale or
    /// server-cancelled request, and any other request failure each keep
    /// their own code and retry advice.
    pub(super) fn from_engine(error: &EngineError) -> Self {
        let message = error.to_string();
        match error.kind() {
            ErrorKind::Timeout => Self {
                code: ErrorCode::Timeout,
                message,
                retryable: true,
                hint: TIMEOUT_TIP,
                lexical_fallback: false,
            },
            ErrorKind::ConnectionClosed => Self {
                code: ErrorCode::ServerCrashed,
                message,
                retryable: true,
                hint: "The language server exited or its connection failed; retry once (it restarts), then fall back to astSearch/localSearch.",
                lexical_fallback: false,
            },
            ErrorKind::ResourceExhausted => Self {
                code: ErrorCode::ServerCrashed,
                message,
                retryable: false,
                hint: "Server hit its memory limit; a retry repeats it. Use the unverified text lead, or astSearch, then localFetch the lines.",
                lexical_fallback: true,
            },
            ErrorKind::Rpc(rpc) => match rpc.code {
                RpcCode::MethodNotFound => Self {
                    code: ErrorCode::CapabilityUnavailable,
                    message,
                    retryable: false,
                    hint: "Use the advertised LSP operations, or fall back to astSearch/localSearch and exact source reads.",
                    lexical_fallback: false,
                },
                RpcCode::ContentModified
                | RpcCode::ServerCancelled
                | RpcCode::RequestCancelled
                | RpcCode::ServerNotInitialized => Self {
                    code: ErrorCode::RequestFailed,
                    message,
                    retryable: true,
                    hint: RETRY_OR_FALL_BACK,
                    lexical_fallback: false,
                },
                RpcCode::UnknownErrorCode if rpc.is_invalid_ast() => {
                    Self::project_not_configured(message)
                }
                RpcCode::InvalidParams | RpcCode::InvalidRequest | RpcCode::ParseError => Self {
                    code: ErrorCode::RequestFailed,
                    message,
                    retryable: false,
                    hint: "The language server rejected the request; re-anchor from localFetch source or use astSearch/localSearch.",
                    lexical_fallback: false,
                },
                _ => Self::request_failed(message),
            },
            _ => Self::request_failed(message),
        }
    }

    /// The row a failure the server cannot recover from becomes: the error,
    /// its one hint, and the leads the failed row offers (never a retry).
    pub(super) fn row(&self, query: &LspSearchQuery, canonical_uri: &str) -> Value {
        let mut value = failure(query, canonical_uri, self.code, &self.message, false);
        value["hints"] = json!([self.hint]);
        if lsp_search_query::operation(query) == "callers" {
            let mut references =
                crate::response::result::query_row(query, &["graphDepth", "page", "snapshot"]);
            references["operation"] = json!("references");
            value["next"]["findReferences"] = continuation(references);
        }
        value
    }

    /// clangd holds no AST for the file: its compile command is missing or
    /// cannot be turned into a compiler invocation. Asking again repeats the answer; only
    /// project setup changes it, and the next call picks that up.
    fn project_not_configured(message: String) -> Self {
        Self {
            code: ErrorCode::MissingConfiguration,
            message,
            retryable: false,
            hint: INVALID_AST_HINT,
            lexical_fallback: false,
        }
    }

    fn request_failed(message: String) -> Self {
        Self {
            code: ErrorCode::RequestFailed,
            message,
            retryable: true,
            hint: RETRY_OR_FALL_BACK,
            lexical_fallback: false,
        }
    }
}

impl From<EngineError> for LspFailure {
    fn from(error: EngineError) -> Self {
        Self::from_engine(&error)
    }
}

pub(super) fn failure(
    query: &LspSearchQuery,
    canonical_uri: &str,
    code: ErrorCode,
    message: &str,
    server_available: bool,
) -> Value {
    let mut value = json!({
        "status": "error",
        "errorCode": code,
        "error": message,
        "lsp": { "serverAvailable": server_available },
        "hints": failure_hint(query, code).into_iter().collect::<Vec<_>>()
    });
    if !canonical_uri.is_empty() {
        value["path"] = json!(super::render::uri_to_path(canonical_uri));
    }
    if code == ErrorCode::Timeout {
        value["next"]["retry"] = continuation(lsp_search_query::to_row(query));
    }
    if lsp_search_query::path(query).is_some() {
        attach_recovery_next(&mut value, query);
        match code {
            ErrorCode::ServerUnavailable => attach_text_search(&mut value, query, None),
            // Whatever the server held is lost: the lexical check is all
            // that is left, and no verification stands behind it.
            ErrorCode::ServerCrashed => {
                attach_text_search(&mut value, query, Some(Confidence::Low))
            }
            _ => {}
        }
    }
    value
}

#[cfg(test)]
mod tests;
