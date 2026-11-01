//! Tool-owned output behavior. The runtime hands the shared stages the
//! tool's [`ToolOutput`] (resolved by `ToolId::output` in
//! `runtime::tool_output`), so `response` never imports a tool module and
//! never branches on the tool. Plain per-tool facts (source reads, provider
//! page selection, first-fragment fields) are data on `ToolId`.
//!
//! A tool with its own recovery or selection finish implements
//! [`ToolOutput`] on a unit struct; every other tool answers with
//! [`SharedOutput`].

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

    /// Finish a classified provider page's row `data` after selection: the
    /// tool's own leads may only name the selected evidence. `hidden` holds
    /// the rows this page judged related or unknown (neither shown nor
    /// excluded), for a judged read of what their rows did not list.
    fn finish_selection(&self, _data: &mut Value, _query: &Value, _hidden: &[Value]) {}
}

/// The output of a tool with no recovery or selection finish of its own:
/// only the shared recovery table answers for it.
pub(crate) struct SharedOutput;
impl ToolOutput for SharedOutput {}

/// Roots are widened only from a trusted source, so the hint names where the
/// setting is read: a workspace `.env` or `.octocoderc` is ignored for it.
/// The workspace itself is the process cwd unless `WORKSPACE_ROOT` is set, so
/// the quickest fix for another project is to start from that directory.
pub(crate) const SANDBOX_HINT: &str = "Outside allowed roots (the workspace is WORKSPACE_ROOT, else the directory octocode started in): run from inside that project or set WORKSPACE_ROOT to it, or add the dir to ALLOWED_PATHS (comma list) in the env or ~/.octocode/.env, not a workspace file.";

/// Recovery for a path that does not resolve: check it from its parent.
pub(crate) const VERIFY_PATH_HINT: &str = "Path not found: list its parent directory with structureSearch, then retry with the exact listed path.";

/// An `error_hint` for a code whose typed error and leads are the whole
/// answer: no fallback text is added.
pub(crate) const NO_HINT: &str = "";

/// Recovery for a path a local read could not open (`fileAccessFailed`).
pub(crate) const LIST_FILES_HINT: &str = "Path did not open as a file: list it with structureSearch operation:\"files\", then retry with one exact listed file.";
