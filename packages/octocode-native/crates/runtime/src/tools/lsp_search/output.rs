//! lspSearch output facts for the shared response stages.
use crate::response::output::{PathAnchor, TextShape, ToolOutput};
use octocode_contract_types::ErrorCode;
use serde_json::Value;

pub(crate) struct Output;

/// What to try after an lspSearch call timed out.
const TIMEOUT_HINT: &str =
    "Retry -- the first call initialises the language server (~60 s cold start).";

impl ToolOutput for Output {
    fn fallback_hint(&self, query: &Value) -> &'static str {
        match query.get("operation").and_then(Value::as_str) {
            // Document-wide operations have no symbol or line to refresh.
            Some("diagnostic" | "documentSymbols") => {
                "Refresh path from a current structureSearch or localSearch hit."
            }
            Some("workspaceSymbol") => {
                "Try a shorter symbolName, or locate the declaration with astSearch or localSearch."
            }
            _ => "Refresh path/symbolName/line from a current astSearch or localSearch hit.",
        }
    }
    /// An empty diagnostics read is the answer itself: no fallback tip. A
    /// call that timed out may have been the one that started the language
    /// server, whose first answer can take about a minute: retrying is the
    /// recovery (the shared timeout tip says to narrow the call instead).
    fn error_hint(&self, code: ErrorCode) -> Option<&'static str> {
        (code == ErrorCode::Timeout).then_some(TIMEOUT_HINT)
    }
    fn category_hint(&self, category: &str) -> Option<&'static str> {
        (category == "noDiagnostics").then_some(crate::response::output::NO_HINT)
    }
    fn evidence_kind(&self, _query: &Value, data: &Value) -> &'static str {
        match data.pointer("/lsp/source").and_then(Value::as_str) {
            Some("native-graph-facts" | "markdown") => "syntactic",
            _ => "semantic",
        }
    }
    fn path_anchor(&self) -> PathAnchor {
        PathAnchor::Workspace
    }
    /// documentSymbols rows print as a compact outline in YAML text; every
    /// other payload renders structured.
    fn text_shape(&self) -> TextShape {
        TextShape::Outline
    }
}

#[cfg(test)]
mod tests {
    use crate::id::ToolId;
    use octocode_contract_types::ErrorCode;
    use serde_json::json;

    #[test]
    fn fallback_hints_name_only_the_operation_inputs() {
        let lsp = ToolId::LspSearch.output();
        let hint = |operation: &str| lsp.fallback_hint(&json!({"operation": operation}));
        assert!(!hint("diagnostic").contains("symbolName"));
        assert!(!hint("documentSymbols").contains("line"));
        assert!(hint("references").contains("symbolName/line"));
        assert!(hint("workspaceSymbol").contains("symbolName"));
    }

    /// A timeout's recovery is the tool's own: retry while the server
    /// starts, not narrow the call.
    #[test]
    fn a_timeout_names_the_language_server_cold_start() {
        let lsp = ToolId::LspSearch.output();
        assert!(
            lsp.error_hint(ErrorCode::Timeout)
                .is_some_and(|hint| hint.contains("cold start")),
        );
        assert_eq!(
            lsp.category_hint("noDiagnostics"),
            Some(crate::response::output::NO_HINT)
        );
        assert!(lsp.error_hint(ErrorCode::AnchorUnresolved).is_none());
    }

    #[test]
    fn native_language_server_results_are_semantic_evidence() {
        let lsp = ToolId::LspSearch.output();
        let kind =
            |source: &str| lsp.evidence_kind(&json!({}), &json!({"lsp": {"source": source}}));
        assert_eq!(kind("native"), "semantic");
        assert_eq!(kind("native-graph-facts"), "syntactic");
    }
}
