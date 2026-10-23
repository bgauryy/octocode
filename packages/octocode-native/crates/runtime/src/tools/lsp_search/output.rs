//! lspSearch output facts for the shared response stages.
use crate::response::output::{PathAnchor, ToolOutput};
use octocode_contract_types::ErrorCode;
use serde_json::Value;

pub(crate) struct Output;

/// What to try after an lspSearch call timed out: the retry is the row's own
/// query (the server keeps loading between calls); the rest works without
/// the server. The one Timeout tip, on a row and on a request-level failure.
pub(super) const TIMEOUT_TIP: &str = "Retry once: the first call initialises the language server (~60 s cold start); else astSearch/localSearch + localFetch.";

/// The static tip of a failure code; `None` when the row's leads and
/// per-call notes say it all (`anchorUnresolved`: `findSimilar`/`read`).
pub(super) fn failure_hint(query: &super::LspSearchQuery, code: ErrorCode) -> Option<&'static str> {
    Some(match code {
        ErrorCode::ServerUnavailable
            if super::lsp_search_query::operation(query) == "workspaceSymbol"
                && super::lsp_search_query::path(query).is_none() =>
        {
            "Provide path for a representative workspace source file so Octocode can select its language server."
        }
        // The row's `next.read` (and `next.findText` for a symbol) is the
        // fallback; no static tip repeats it.
        ErrorCode::ServerUnavailable if super::lsp_search_query::path(query).is_some() => {
            return None;
        }
        ErrorCode::ServerUnavailable => {
            "Use astSearch symbols/match or localSearch for candidates, then localFetch exact source."
        }
        ErrorCode::CapabilityUnavailable
            if matches!(
                super::lsp_search_query::op(query),
                super::Op::Supertypes | super::Op::Subtypes
            ) =>
        {
            "This server cannot prove type hierarchy; inspect declarations with astSearch and confirm exact source."
        }
        ErrorCode::CapabilityUnavailable => {
            "Use the advertised LSP operations, or fall back to astSearch/localSearch and exact source reads."
        }
        ErrorCode::Timeout => TIMEOUT_TIP,
        ErrorCode::FileTooLarge => {
            "Use localSearch or astSearch to locate a bounded region, then read that exact source range."
        }
        ErrorCode::AnchorUnresolved => return None,
        _ => "Use astSearch or localSearch to locate candidates, then localFetch exact source.",
    })
}

/// The static tip of an empty category; `None` when the row's reason says
/// it all.
pub(super) fn empty_hint(category: &str, operation: &str) -> Option<&'static str> {
    Some(match category {
        // The anchor resolved: an empty hierarchy is an answer, not a bad anchor.
        "noLocations" if operation == "subtypes" => {
            "Nothing extends or implements this symbol in the files checked; confirm with the name search lead."
        }
        "noLocations" if operation == "supertypes" => {
            "No base type found: a root type has none. Confirm with the definition."
        }
        // The row's `next.read` is the recovery; the reason states the count.
        "noLocations" | "locationsWithheld" => return None,
        "noProject" => {
            "Use localSearch or astSearch here, or run lspSearch where its project file (Cargo.toml, package.json) is present."
        }
        "noDiagnostics" => return None,
        "diagnosticsNotPublished" => {
            "Retry once the server has analyzed the file, or run the project's type-check/build for authoritative errors."
        }
        _ => "Use astSearch or localSearch to locate candidates, then localFetch exact source.",
    })
}

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
    /// Row tips that depend on the query (`failure_hint`, `empty_hint`) are
    /// the same table, below.
    fn error_hint(&self, code: ErrorCode) -> Option<&'static str> {
        (code == ErrorCode::Timeout).then_some(TIMEOUT_TIP)
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
    /// A `kind` that repeats the operation asked, or the row's `empty`
    /// status, restates what the caller already has.
    fn compact(&self, data: &mut serde_json::Map<String, Value>, query: &Value) {
        let kind = data.get("kind").and_then(Value::as_str);
        if kind.is_some_and(|kind| kind == "empty" || Some(kind) == query["operation"].as_str()) {
            data.remove("kind");
        }
    }
    fn path_anchor(&self) -> PathAnchor {
        PathAnchor::Workspace
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

    /// A row's `kind` that repeats the operation asked, or the `empty`
    /// status, goes by default; any other kind stays.
    #[test]
    fn compact_drops_a_kind_that_restates_the_query_or_status() {
        let lsp = ToolId::LspSearch.output();
        let compacted = |data: serde_json::Value, query: serde_json::Value| {
            let mut data = data.as_object().cloned().expect("object");
            lsp.compact(&mut data, &query);
            serde_json::Value::Object(data)
        };
        let references = json!({"operation": "references"});
        assert_eq!(
            compacted(
                json!({"kind": "references", "files": []}),
                references.clone()
            ),
            json!({"files": []})
        );
        assert_eq!(
            compacted(
                json!({"kind": "empty", "category": "noLocations"}),
                references.clone()
            ),
            json!({"category": "noLocations"})
        );
        assert_eq!(
            compacted(
                json!({"kind": "documentSymbols", "symbols": []}),
                references
            ),
            json!({"kind": "documentSymbols", "symbols": []})
        );
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
