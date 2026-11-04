//! The typed request kinds and the predicates that classify methods and errors.

use crate::error::Error;
use serde_json::Value;

/// Whether a provider request may answer from the response scope's recorded
/// answers, or must ask the server and record the new answer.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Reuse {
    Recorded,
    Fresh,
}

/// Which location request [`NativeLspClient::get_locations`] sends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocationRequest {
    Definition,
    References { include_declaration: bool },
    TypeDefinition,
    Implementation,
}

impl LocationRequest {
    pub(super) fn method(self) -> &'static str {
        match self {
            Self::Definition => "textDocument/definition",
            Self::References { .. } => "textDocument/references",
            Self::TypeDefinition => "textDocument/typeDefinition",
            Self::Implementation => "textDocument/implementation",
        }
    }
}

/// A diagnostic report's `resultId`.
pub(super) fn result_id(report: &Value) -> Option<&str> {
    report.get("resultId").and_then(Value::as_str)
}

pub(super) fn supports_partial_results(method: &str) -> bool {
    matches!(
        method,
        "textDocument/definition"
            | "textDocument/references"
            | "textDocument/typeDefinition"
            | "textDocument/implementation"
            | "textDocument/documentSymbol"
            | "workspace/symbol"
            | "callHierarchy/incomingCalls"
            | "callHierarchy/outgoingCalls"
            | "typeHierarchy/supertypes"
            | "typeHierarchy/subtypes"
            | "textDocument/diagnostic"
    )
}

/// A request failure worth re-sending: `ContentModified`, or
/// `ServerCancelled` when the server asked for a retrigger. Matches the typed
/// RPC error, never the rendered message text.
pub(in crate::lsp::client) fn is_retryable_error(error: &Error) -> bool {
    error
        .rpc_error()
        .is_some_and(crate::error::RpcError::is_retryable)
}
