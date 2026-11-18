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
pub fn has_value(query: &Value, key: &str) -> bool {
    match query.get(key) {
        None | Some(Value::Null) => false,
        Some(Value::String(text)) => !text.trim().is_empty(),
        Some(Value::Array(items)) => !items.is_empty(),
        Some(_) => true,
    }
}

/// What the shared response stages ask a tool about its own output.
pub trait ToolOutput: Sync {
    /// This tool's recovery for `code`, or its own for a timeout; [`NO_HINT`]
    /// when the row's own facts are the whole answer. `query` is the row's
    /// query (`Null` for a request-level failure). The response stage answers
    /// the codes every tool recovers from alike (sandbox, auth, permission,
    /// rate limits, transport, size) without asking
    /// ([`super::rows::error_code_hint`]); for any other code
    /// (`notFound`, `invalidInput`, the tool's own) it asks this first and
    /// falls back to the shared hint when this has none.
    fn error_hint(&self, _code: ErrorCode, _query: &Value) -> Option<&'static str> {
        None
    }

    /// Finish a classified provider page's row `data` after selection: the
    /// tool's own leads may only name the selected evidence. `hidden` holds
    /// the files a code page judged related or unknown (neither shown nor
    /// excluded), for a judged read of what their rows did not list.
    /// `removed` holds every unit selection cut from a row-listing page, as
    /// the page listed it (a listing entry as `{"path"}`), so a lead to one
    /// can be dropped.
    fn finish_selection(
        &self,
        _data: &mut Value,
        _query: &Value,
        _hidden: &[Value],
        _removed: &[Value],
    ) {
    }

    /// Remove from a classified row's `data` what only the judge read (a
    /// listing's body), unless the query asked for it. Called on every
    /// outcome of one row's selection: selected, failed or unavailable.
    fn strip_judge_only(&self, _data: &mut Value, _query: &Value) {}
}

/// Keep, in each `tool` lead of `data` (`next` calls and `related[]`), only
/// the query rows `keep` accepts; a lead left with none is dropped. Leads of
/// other tools stay.
pub fn retain_lead_reads(data: &mut Value, tool: crate::id::ToolId, keep: impl Fn(&Value) -> bool) {
    let retain = |lead: &mut Value| {
        if lead["tool"] != tool.as_str() {
            return true;
        }
        let Some(rows) = lead
            .pointer_mut("/input/queries")
            .and_then(Value::as_array_mut)
        else {
            return false;
        };
        rows.retain(&keep);
        !rows.is_empty()
    };
    if let Some(leads) = data.get_mut("next").and_then(Value::as_object_mut) {
        leads.retain(|_, lead| retain(lead));
    }
    if let Some(leads) = data.get_mut("related").and_then(Value::as_array_mut) {
        leads.retain_mut(retain);
    }
}

/// The output of a tool with no recovery or selection finish of its own:
/// only the shared recovery table answers for it.
pub struct SharedOutput;
impl ToolOutput for SharedOutput {}

/// Roots are widened only from a trusted source, so the hint names where the
/// setting is read: a workspace `.env` or `.octocoderc` is ignored for it.
/// The workspace itself is the process cwd unless `WORKSPACE_ROOT` is set, so
/// the quickest fix for another project is to start from that directory.
pub(crate) const SANDBOX_HINT: &str = "Outside allowed roots (the workspace is WORKSPACE_ROOT, else the directory octocode started in): run from inside that project or set WORKSPACE_ROOT to it, or add the dir to ALLOWED_PATHS (comma list) in the env or ~/.octocode/.env, not a workspace file.";

/// Recovery for a name that does not resolve (a path, repository, package,
/// version or symbol): check it where it was looked up, a path from its
/// parent. One text serves every tool, so it names none: the row's own
/// related[] request lists the parent where one exists.
pub const NOT_FOUND_HINT: &str = "Check the exact name, where it was looked up and access to it; for a path, list its parent directory (the related[] listing request when one is given), then retry with the exact listed path.";

/// An `error_hint` for a code whose typed error and leads are the whole
/// answer: no fallback text is added.
pub const NO_HINT: &str = "";

/// Recovery for a path a local read could not open (`fileAccessFailed`).
pub const LIST_FILES_HINT: &str = "Path did not open as a file: list it with structureSearch operation:\"files\", then retry with one exact listed file.";
