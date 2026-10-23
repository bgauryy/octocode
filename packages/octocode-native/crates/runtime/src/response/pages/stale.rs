use super::{page_blocks_deep, page_blocks_deep_mut};
use crate::id::ToolId;
use serde_json::{Map, Value, json};

/// The one text of a stale snapshot, whichever tool's cursor went stale.
pub(crate) const STALE_SNAPSHOT_ERROR: &str = "The snapshot does not match this source and query (they changed, or the snapshot is missing or from another call); nothing shown remains valid: run the restart call, page 1 without the snapshot.";

/// A stale-snapshot row ends the page: the shared text, no `isPartial` and no
/// `hasMore` (nothing of the shown result remains to read), no continuation
/// but `next.restart`, which the tool built or is page 1 of the same query
/// without its snapshot and page cursors.
pub(in super::super) fn restart_stale(tool: ToolId, query: &Value, data: &mut Value) {
    let Some(object) = data.as_object_mut() else {
        return;
    };
    object.insert("error".into(), json!(STALE_SNAPSHOT_ERROR));
    object.remove("isPartial");
    for block in page_blocks_deep_mut(object) {
        if let Some(block) = block.as_object_mut()
            && block.contains_key("hasMore")
        {
            block.insert("hasMore".into(), json!(false));
        }
    }
    if let Some(next) = object.get_mut("next").and_then(Value::as_object_mut) {
        next.retain(|kind, _| kind == "restart");
    }
    ensure_restart(tool, query, object);
}

/// `next.restart` when the tool built none: the one restart of the same
/// query ([`crate::response::result::restart_query`]).
fn ensure_restart(tool: ToolId, query: &Value, object: &mut Map<String, Value>) {
    if object
        .get("next")
        .and_then(Value::as_object)
        .is_some_and(|next| next.keys().any(|name| crate::id::is_restart(name)))
    {
        return;
    }
    if !query.is_object() {
        return;
    }
    let restart = crate::response::result::restart_query(tool, query);
    match object.entry("next").or_insert_with(|| json!({})) {
        Value::Object(next) => {
            next.insert("restart".into(), restart);
        }
        other => *other = json!({ "restart": restart }),
    }
}

/// A page past the end is one outcome on every tool: the row is
/// `status:"outOfRange"` (set by `rows::result_row`), its
/// `reason` names the page and the page count (`page 99 of 4`), and
/// `next.restart` re-asks the query from page 1. A tool flags it with
/// `outOfRange` on its `pagination` block or a section block
/// (`contentPagination.files`). Returns whether `data` was such a page.
pub(in super::super) fn settle_out_of_range(tool: ToolId, query: &Value, data: &mut Value) -> bool {
    let Some(object) = data.as_object_mut() else {
        return false;
    };
    let Some(pagination) = out_of_range_block(object) else {
        return false;
    };
    let reason = match (
        pagination.get("currentPage").and_then(Value::as_u64),
        pagination.get("totalPages").and_then(Value::as_u64),
    ) {
        (Some(page), Some(pages)) => format!("page {page} of {pages}"),
        (Some(page), None) => format!("page {page} is past the last page"),
        _ => "page is past the last page".to_owned(),
    };
    object.entry("reason").or_insert(json!(reason));
    // No page is left to read: a tool's partial signals would only restate
    // the outcome `reason` and `next.restart` already carry.
    for signal in ["isPartial", "terminalLimit", "partialReasons"] {
        object.remove(signal);
    }
    ensure_restart(tool, query, object);
    true
}

/// The row's page block flagged `outOfRange`: one of the row's own, or one
/// of a resource item the row holds (`pullRequests[0].contentPagination.files`),
/// since the page cursor is the row's either way.
fn out_of_range_block(object: &Map<String, Value>) -> Option<Value> {
    page_blocks_deep(object)
        .into_iter()
        .find(|block| block["outOfRange"] == true)
        .cloned()
}
