//! lspSearch output facts for the shared response stages.
use crate::response::output::{LIST_FILES_HINT, NO_HINT, PathAnchor, ToolOutput};
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
fn failure_hint(code: ErrorCode, query: &Value) -> &'static str {
    let (operation, has_path) = shape(query);
    match code {
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
        ErrorCode::NotAFile | ErrorCode::BinaryFileUnsupported | ErrorCode::PathValidationFailed => {
            LIST_FILES_HINT
        }
        ErrorCode::ServerUnavailable if operation == "workspaceSymbol" && !has_path => {
            "Provide path for a representative workspace source file so Octocode can select its language server."
        }
        // The row's `next.read` (and `next.findText` for a symbol) is the
        // fallback; no static tip repeats it.
        ErrorCode::ServerUnavailable if has_path => NO_HINT,
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
        _ => "Use astSearch or localSearch to locate candidates, then localFetch exact source.",
    }
}

/// The tip of an empty category; `None` when the row's reason says it all
/// (the row's `next.read` is the recovery for `noLocations`).
fn empty_hint(category: &str, query: &Value) -> Option<&'static str> {
    let (operation, _) = shape(query);
    Some(match category {
        // The anchor resolved: an empty hierarchy is an answer, not a bad anchor.
        "noLocations" if operation == "subtypes" => {
            "Nothing extends or implements this symbol in the files checked; confirm with the name search lead."
        }
        "noLocations" if operation == "supertypes" => {
            "No base type found: a root type has none. Confirm with the definition."
        }
        "noLocations" | "locationsWithheld" => return None,
        "noProject" => {
            "Use localSearch or astSearch here, or run lspSearch where its project file (Cargo.toml, package.json) is present."
        }
        // An empty diagnostics read is the answer itself.
        "noDiagnostics" => NO_HINT,
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
    /// A call that timed out may have been the one that started the language
    /// server, whose first answer can take about a minute: retrying is the
    /// recovery (the shared timeout tip says to narrow the call instead). The
    /// tips depend on the query's operation and path.
    fn error_hint(&self, code: ErrorCode, query: &Value) -> Option<&'static str> {
        Some(failure_hint(code, query))
    }
    fn category_hint(&self, category: &str, query: &Value) -> Option<&'static str> {
        empty_hint(category, query)
    }
    /// A failed row's retry or read lead runs the next call; the tip says
    /// why the server needs it.
    fn hints_beside_leads(&self) -> bool {
        true
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
            lsp.error_hint(ErrorCode::Timeout, &json!({}))
                .is_some_and(|hint| hint.contains("cold start")),
        );
        assert_eq!(
            lsp.category_hint("noDiagnostics", &json!({})),
            Some(crate::response::output::NO_HINT)
        );
        assert_eq!(
            lsp.error_hint(ErrorCode::AnchorUnresolved, &json!({})),
            Some(crate::response::output::NO_HINT)
        );
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
