//! Typed failures, empty rows, partial marking, and executable `next.*`
//! continuations shared by every `lspSearch` operation.

use super::render::uri_to_path;
use crate::id::ToolId;
use crate::tools::lsp_search::lsp_search_query;
use octocode_contract_types::{ErrorCode, LspSearchQuery};
use octocode_engine::error::{ErrorCode as RpcCode, ErrorKind};
use serde_json::{Value, json};

mod anchors;
mod leads;
mod paging;
mod rows;
pub(super) use anchors::{anchor_recovery, attach_recovery_next, path_failure};
use leads::symbol_search_lead;
pub(super) use leads::{
    FILE_LEVEL_REASONS, LISTED_TEXT_FILES, TEXT_CHECK, flag_partial, flagged_with,
    ignored_text_lead, shown_paths, term_search_lead, text_search_lead,
};
pub(super) use paging::{
    ALIAS_RECOVERY_FAILED, IMPORTER_REQUEST_FAILED, RequestFailures, offer_next_page, with_next,
};
use rows::attach_text_search;
pub(super) use rows::{
    BUILD_SCRIPTS_COST, build_scripts_rerun, continuation, empty, mark_anchor_line_errors,
    mark_diagnostics_unpublished, partial_with_retry, runs_build_scripts, stamp_coverage,
};

/// A request-level failure `execute` could not render as a row. The
/// dispatcher emits `code`/`message`/`retryable` verbatim, so each cause
/// keeps its own code instead of one generic "unavailable" one; the
/// recovery tip is the response stage's, keyed by that code and the query
/// (`lsp_search::output`).
#[derive(Clone, Debug, PartialEq)]
pub struct LspFailure {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    /// The language server cannot answer this scope at all (it was stopped
    /// for its memory), so the row leads to an unverified text search
    /// instead of a retry.
    pub lexical_fallback: bool,
}

impl LspFailure {
    pub(super) fn invalid_query(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidInput,
            message: message.into(),
            retryable: false,
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
        Self {
            code: error.shared_code(),
            message: error.message,
            retryable: false,
            lexical_fallback: false,
        }
    }

    /// Classify an engine error by its typed kind (never by message text):
    /// a timeout, a server that exited, an unimplemented method, a stale or
    /// server-cancelled request, and any other request failure each keep
    /// their own code and retry flag.
    pub(super) fn from_engine(error: &octocode_engine::error::Error) -> Self {
        let message = error.to_string();
        match error.kind() {
            ErrorKind::Timeout => Self {
                code: ErrorCode::Timeout,
                message,
                retryable: true,
                lexical_fallback: false,
            },
            ErrorKind::ConnectionClosed => Self {
                code: ErrorCode::ServerCrashed,
                message,
                retryable: true,
                lexical_fallback: false,
            },
            ErrorKind::ResourceExhausted => Self {
                code: ErrorCode::ServerCrashed,
                message,
                retryable: false,
                lexical_fallback: true,
            },
            ErrorKind::Rpc(rpc) => match rpc.code {
                RpcCode::MethodNotFound => Self {
                    code: ErrorCode::CapabilityUnavailable,
                    message,
                    retryable: false,
                    lexical_fallback: false,
                },
                RpcCode::ContentModified
                | RpcCode::ServerCancelled
                | RpcCode::RequestCancelled
                | RpcCode::ServerNotInitialized => Self {
                    code: ErrorCode::RequestFailed,
                    message,
                    retryable: true,
                    lexical_fallback: false,
                },
                RpcCode::UnknownErrorCode if rpc.is_invalid_ast() => {
                    Self::project_not_configured(message)
                }
                RpcCode::InvalidParams | RpcCode::InvalidRequest | RpcCode::ParseError => Self {
                    code: ErrorCode::RequestFailed,
                    message,
                    retryable: false,
                    lexical_fallback: false,
                },
                _ => Self::request_failed(message),
            },
            _ => Self::request_failed(message),
        }
    }

    /// The row a failure the server cannot recover from becomes: the error
    /// and the leads the failed row offers (never a retry); the response
    /// stage states the code's tip beside them.
    pub(super) fn row(&self, query: &LspSearchQuery, canonical_uri: &str) -> Value {
        let mut value = failure(query, canonical_uri, self.code, &self.message, false);
        if lsp_search_query::operation(query) == "callers" {
            let mut references = crate::response::result::query_row(
                query,
                &[
                    "graphDepth",
                    "page",
                    "snapshot",
                    "importerPage",
                    "candidatePage",
                ],
            );
            references["operation"] = json!("references");
            crate::response::result::attach_related(
                &mut value,
                crate::response::result::Continuation::related(ToolId::LspSearch, references)
                    .build(),
            );
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
            lexical_fallback: false,
        }
    }

    fn request_failed(message: String) -> Self {
        Self {
            code: ErrorCode::RequestFailed,
            message,
            retryable: true,
            lexical_fallback: false,
        }
    }
}

impl From<octocode_engine::error::Error> for LspFailure {
    fn from(error: octocode_engine::error::Error) -> Self {
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
            ErrorCode::ServerUnavailable => attach_text_search(&mut value, query),
            // Whatever the server held is lost: the lexical check is all
            // that is left, and no verification stands behind it.
            ErrorCode::ServerCrashed => attach_text_search(&mut value, query),
            _ => {}
        }
    }
    value
}

#[cfg(test)]
mod tests;
