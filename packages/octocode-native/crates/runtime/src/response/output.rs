//! Tool-owned output facts. The shared response stages ask the tool through
//! `ToolId::output` (the dispatch in `runtime::tool_output`) instead of
//! branching on the tool: each tool answers for its own recovery tips,
//! evidence kind, path anchoring and text shape.
//!
//! A tool module implements [`ToolOutput`] on a unit struct and the
//! dispatch points its `ToolId` at it.

use octocode_contract_types::ErrorCode;
use serde_json::Value;

/// Whether the row query sets `key` to something: not absent, null, blank or
/// an empty list. Fallback hints name the filters a query actually set.
pub(crate) fn has_value(query: &Value, key: &str) -> bool {
    match query.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::String(text)) => !text.trim().is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(_) => true,
    }
}

/// What the shared response stages ask a tool about its own output.
pub(crate) trait ToolOutput: Sync {
    /// Recovery for an `errorCode` only this tool emits, or its own for a
    /// timeout; [`NO_HINT`] when the row's own facts are the whole answer.
    /// `query` is the row's query (`Null` for a request-level failure).
    /// Codes every tool shares (sandbox, auth, rate limit, not found,
    /// invalid input) are answered by the response stage.
    fn error_hint(&self, _code: ErrorCode, _query: &Value) -> Option<&'static str> {
        None
    }

    fn rows_key(&self) -> &'static str {
        "results"
    }

    /// Shared row definitions carried by the first display fragment only.
    fn first_fragment_fields(&self) -> &'static [&'static str] {
        &[]
    }

    /// Each row carries one source `content` string a caller copies from
    /// (numbered lines, sized to its share of the response page).
    fn reads_source(&self) -> bool {
        false
    }

    /// Whether `classification` selects this tool's rows at the shared response
    /// boundary, over one sanitized provider page (code search), instead of
    /// before the tool's own source pagination.
    fn selects_provider_page(&self) -> bool {
        false
    }

    /// Finish a classified provider page's row `data` after selection: the
    /// tool's own leads may only name the selected evidence. `hidden` holds
    /// the rows this page judged related or unknown (neither shown nor
    /// excluded), for a judged read of what their rows did not list.
    fn finish_selection(&self, _data: &mut Value, _query: &Value, _hidden: &[Value]) {}
}

/// Roots are widened only from a trusted source, so the hint names where the
/// setting is read: a workspace `.env` or `.octocoderc` is ignored for it.
pub(crate) const SANDBOX_HINT: &str = "Outside allowed roots: add the dir to ALLOWED_PATHS (comma list) in the env or ~/.octocode/.env, not a workspace file.";

/// Recovery for a path that does not resolve: check it from its parent.
pub(crate) const VERIFY_PATH_HINT: &str =
    "Verify the path exists (structureSearch on its parent directory), then retry the exact path.";

/// An `error_hint` for a code whose typed error and leads are the whole
/// answer: no fallback text is added.
pub(crate) const NO_HINT: &str = "";

/// Recovery for a path a local read could not open (`fileAccessFailed`).
pub(crate) const LIST_FILES_HINT: &str =
    "Verify the path with structureSearch operation:\"files\", then retry the exact path.";
