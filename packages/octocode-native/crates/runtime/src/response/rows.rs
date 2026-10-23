//! Transport-neutral structured result metadata and lossless path compaction.
mod envelope;
mod minimize;
mod paths;
mod recovery;
use crate::id::ToolId;
pub use envelope::envelope;
pub(crate) use envelope::envelope_in;
pub(crate) use minimize::minimize_row;
use octocode_contract_types::ErrorCode;
use paths::{
    common_directory, prefix_relative_paths, relative_continuation_path,
    relativize_continuation_paths, rewrite_paths, visit_paths,
};
#[cfg(test)]
pub(super) use recovery::concise;
pub(crate) use recovery::{
    CANCELLED_HINT, GITHUB_AUTH_RECOVERY_HINT, MAX_GUIDANCE_CHARS, apply_hint_policy,
    attach_diagnostics, ensure_recovery, error_code_hint, has_recovery,
};
use serde_json::{Map, Value, json};

/// What a finished row carries besides its data and status: the tool's
/// diagnostics, whether it came from cache, and the typed notes of what its
/// pages leave to read.
#[derive(Default)]
pub(crate) struct RowParts {
    pub(crate) diagnostics: crate::response::result::ToolDiagnostics,
    pub(crate) cache: bool,
    pub(crate) remaining: Vec<super::pages::Remaining>,
}

/// One row as the response stage ships it, from a tool's `data`: the one
/// finishing sequence ([`result_row`], diagnostics, the cache flag, the hint
/// policy, the remaining-page notes, the default-extras prune and the
/// minimization), so anything that measures a published row (the runtime's
/// own rows, localFetch pricing its share of the response page) runs the same
/// steps in the same order.
pub(crate) fn finish_row(
    tool: ToolId,
    index: usize,
    query: &Value,
    data: Value,
    status: Option<&str>,
    parts: RowParts,
) -> Value {
    let mut row = result_row(tool, index, query, data, status);
    attach_diagnostics(&mut row, parts.diagnostics);
    if parts.cache {
        row["cache"] = json!(1);
    }
    apply_hint_policy(&mut row, tool, query);
    super::pages::note_remaining(&mut row, tool, parts.remaining);
    super::extras::prune_row(&mut row, tool, query);
    minimize_row(&mut row, tool, query);
    row
}

pub(crate) fn result_row(
    tool: ToolId,
    index: usize,
    query: &Value,
    mut data: Value,
    status: Option<&str>,
) -> Value {
    if crate::id::error_code_of(&data) == Some(ErrorCode::StaleSnapshot) {
        super::pages::restart_stale(tool, query, &mut data);
    }
    super::notices::drop_on_later_page(query, &mut data);
    let status =
        if status != Some("error") && super::pages::settle_out_of_range(tool, query, &mut data) {
            Some("outOfRange")
        } else {
            status
        };
    // An error row is not a partial result: a page it offers is the way
    // past the error (a bounded read of a too-large file), not more of it.
    let paged = status != Some("error")
        && !tool.output().resource_major()
        && !super::pages::is_complete(&data)
        && super::pages::has_remaining_page(tool, &data);
    // A row with more to read is not empty, whatever the tool decided from
    // the rows this page holds: an executable page or a pagination block
    // stating `hasMore` means the result continues.
    // A retry runs the same page again: an empty row offering only one (an
    // incomplete provider page) is still empty, so the call exits 1.
    let continues = paged && super::pages::has_page_beyond_retry(tool, &data);
    let status = if status == Some("empty") && (continues || states_more(&data)) {
        None
    } else {
        status
    };
    if let Some(object) = data.as_object_mut()
        && object.get("isPartial") == Some(&Value::Null)
    {
        object.insert("isPartial".into(), Value::Bool(false));
    }
    // A remaining page makes the row itself partial, whatever its nested
    // items say. The row's own `pagination.hasMore` with its `next` page
    // already says so, so the flag is left to what they do not say.
    if let Some(object) = data.as_object_mut() {
        if paged && states_remaining_page(object) {
            object.remove("isPartial");
        } else if paged {
            object.insert("isPartial".into(), Value::Bool(true));
        }
    }
    if let Some(object) = data.as_object_mut() {
        // Request echoes: the row answers its query, it does not repeat it.
        let echoes = crate::response::result::INTENT_FIELDS
            .into_iter()
            .chain(["cache", "query"]);
        for key in echoes {
            object.remove(key);
        }
        object.remove("status");
        if status != Some("error") {
            object.remove("error");
        }
    }
    // `meta` is a diagnostics extra: built only for a row that keeps it
    // (`extras: ["diagnostics"]`, or a `fields` pick naming it).
    let mut row = Map::new();
    row.insert("index".into(), json!(index));
    if super::extras::keeps(tool, query, "results[].meta") {
        row.insert("meta".into(), row_meta(tool, query, &data, status));
    }
    row.insert("data".into(), data);
    if let Some(status) = status {
        row.insert("status".into(), json!(status));
    }
    Value::Object(row)
}

/// Whether any page block of the row, its own or a nested item's, states
/// `hasMore`.
fn states_more(data: &Value) -> bool {
    data.as_object().is_some_and(|object| {
        super::pages::page_blocks_deep(object)
            .into_iter()
            .any(|block| block["hasMore"] == true)
    })
}

/// Whether a page block of the row (`pagination`, a `contentPagination`
/// section, its own or a nested item's) reports `hasMore` and nothing beyond
/// the pages makes it partial: `isPartial` would restate it. A
/// `partialReasons` list or a `terminalLimit` is another cause, which the
/// flag still sums up.
fn states_remaining_page(data: &Map<String, Value>) -> bool {
    data.get("terminalLimit") != Some(&Value::Bool(true))
        && data
            .get("partialReasons")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
        && super::pages::page_blocks_deep(data)
            .into_iter()
            .any(|block| block["hasMore"] == true)
}

/// The diagnostics view of a row: its evidence kind and confidence, and the
/// diagnostics of a partial or failed row.
fn row_meta(tool: ToolId, query: &Value, data: &Value, status: Option<&str>) -> Value {
    let partial = super::pages::is_partial(data);
    let kind = tool.output().evidence_kind(query, data);
    let reported = data.get("confidence").and_then(Value::as_str);
    let confidence = if status == Some("error") || reported == Some("low") {
        "low"
    } else if let Some(value @ ("medium" | "high")) = reported {
        value
    } else if matches!(kind, "provider" | "lexical" | "syntactic") {
        "medium"
    } else {
        "high"
    };
    let mut codes = Vec::new();
    if let Some(code) = data.get("errorCode").and_then(Value::as_str) {
        codes.push(code.to_owned());
    }
    codes.extend(super::pages::coverage_codes(data, partial));
    let mut meta = json!({"evidence":{"kind":kind,"confidence":confidence}});
    if partial || !codes.is_empty() {
        let mut diagnostics = Map::new();
        if !codes.is_empty() {
            diagnostics.insert("codes".into(), json!(codes));
        }
        if partial {
            diagnostics.insert("partial".into(), json!(true));
        }
        meta["diagnostics"] = Value::Object(diagnostics);
    }
    meta
}

/// Row `errorCode`s that reject the caller's input (CLI exit 2): the request
/// is wrong, so broadening or retrying it cannot help.
pub fn is_invalid_input_code(code: &str) -> bool {
    crate::id::error_class(code) == crate::id::error_codes::ErrorClass::InvalidInput
}

#[cfg(test)]
mod tests;
