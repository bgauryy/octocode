//! The page contract every tool shares: one pagination block shape, the
//! unfinished-result disclosure, the stale-snapshot restart, and the facts
//! that tell whether a row has more to read.
//!
//! A row whose `next` holds pages that leave a counted rest shows no
//! warning for it: its `pagination` blocks and `next` already say where it
//! stands and what continues. The one exception is a split row whose pages
//! moved to the response pager, which a part names once in `warnings`
//! (`7 more changed files (next.nextFilePage on the row's last part)`) because its own `next`
//! no longer carries them. The counts come from the tool's own typed notes
//! ([`crate::response::result::ToolData::remaining`], for counts only the tool
//! knows) or else the row's public pagination facts, noted as data
//! ([`crate::response::result::Remaining`]) before extras pruning ([`note_remaining`]); the stage
//! takes them off the rows ([`take_remaining_notes`]) and the pager words
//! the moved ones ([`redisclose`]). `partialReasons` say why a row is
//! partial, never how much is left.

mod facts;
mod remaining;
mod stale;
pub(crate) use facts::{PageFacts, SpanFacts};
pub(crate) use remaining::{
    counted, first_named, items_left, more, note_remaining, redisclose, take_remaining_notes,
};
use serde_json::{Map, Value};
pub(super) use stale::{restart_stale, settle_out_of_range};

use crate::id::ToolId;
use crate::id::continuation_channels::{HINTS_KEY, PAGES_KEY};
use crate::response::result::is_call;

const WARNINGS: &str = "warnings";

/// Cursor keys a tool's pagination block carries to the response stage,
/// which moves them onto `next` (see [`slim_pagination`]).
const CURSOR_KEYS: [&str; 3] = ["nextPage", "snapshot", "resultId"];

/// One pagination block, one shape for every tool and page: where the page
/// stands, its size, what is known of the total and whether more exists.
/// The cursor (`nextPage`, `snapshot`, `resultId`) rides `next`; a false
/// cap flag asserts nothing, and a true one repeats `providerLimit`.
pub(super) fn slim_pagination(block: &mut Map<String, Value>, limited: bool) {
    for key in CURSOR_KEYS {
        block.remove(key);
    }
    match block.get("totalItemsCapped").and_then(Value::as_bool) {
        Some(false) => {
            block.remove("totalItemsCapped");
        }
        Some(true) if limited => {
            block.remove("totalItemsCapped");
        }
        _ => {}
    }
}

/// Whether `key` is a pagination block: the shared output fields
/// `pagination`, `contentPagination` and `responsePagination` share the
/// suffix by the contract's naming convention; the contract lists no such
/// keys, so the convention is the rule.
pub(super) fn is_pagination_key(key: &str) -> bool {
    key == "pagination" || key.ends_with("Pagination")
}

/// The sectioned pagination field: an item read's per-section page blocks
/// (`contentPagination.files`, `.commits`, `.comments`, …).
const SECTION_PAGINATION: &str = "contentPagination";

/// The page blocks of the pagination field `key`: one block, or each section
/// block of `contentPagination`.
fn page_blocks<'a>(key: &str, value: &'a Value) -> Vec<&'a Value> {
    match (is_pagination_key(key), key == SECTION_PAGINATION) {
        (false, _) => Vec::new(),
        (true, true) => value
            .as_object()
            .map(|sections| sections.values().collect())
            .unwrap_or_default(),
        (true, false) => vec![value],
    }
}

/// [`page_blocks`], mutably.
fn page_blocks_mut<'a>(key: &str, value: &'a mut Value) -> Vec<&'a mut Value> {
    match (is_pagination_key(key), key == SECTION_PAGINATION) {
        (false, _) => Vec::new(),
        (true, true) => value
            .as_object_mut()
            .map(|sections| sections.values_mut().collect())
            .unwrap_or_default(),
        (true, false) => vec![value],
    }
}

/// Every page block of a row's `data` at any depth: the [`page_blocks`] of
/// its own pagination fields and of its nested items' (a pull request's
/// `contentPagination`, a file's match `pagination`). `next`/`hints` and
/// calls are never entered; the walk stops at
/// [`crate::response::result::NESTING_DEPTH`].
pub(super) fn page_blocks_deep(data: &Map<String, Value>) -> Vec<&Value> {
    fn object<'a>(fields: &'a Map<String, Value>, out: &mut Vec<&'a Value>, depth: usize) {
        for (key, child) in fields {
            if is_pagination_key(key) {
                out.extend(page_blocks(key, child));
            } else if key != PAGES_KEY && key != HINTS_KEY {
                value(child, out, depth + 1);
            }
        }
    }
    fn value<'a>(child: &'a Value, out: &mut Vec<&'a Value>, depth: usize) {
        if depth > crate::response::result::NESTING_DEPTH || is_call(child) {
            return;
        }
        match child {
            Value::Object(fields) => object(fields, out, depth),
            Value::Array(items) => items.iter().for_each(|item| value(item, out, depth + 1)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    object(data, &mut out, 0);
    out
}

/// [`page_blocks_deep`], mutably.
pub(super) fn page_blocks_deep_mut(data: &mut Map<String, Value>) -> Vec<&mut Value> {
    fn object<'a>(fields: &'a mut Map<String, Value>, out: &mut Vec<&'a mut Value>, depth: usize) {
        for (key, child) in fields.iter_mut() {
            if is_pagination_key(key) {
                out.extend(page_blocks_mut(key, child));
            } else if key != PAGES_KEY && key != HINTS_KEY {
                value(child, out, depth + 1);
            }
        }
    }
    fn value<'a>(child: &'a mut Value, out: &mut Vec<&'a mut Value>, depth: usize) {
        if depth > crate::response::result::NESTING_DEPTH || is_call(child) {
            return;
        }
        match child {
            Value::Object(fields) => object(fields, out, depth),
            Value::Array(items) => items
                .iter_mut()
                .for_each(|item| value(item, out, depth + 1)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    object(data, &mut out, 0);
    out
}

/// Keep only the page blocks of a pagination field that pass `keep`; a
/// sectioned field keeps its passing sections. Returns whether anything is
/// left.
pub(super) fn retain_page_blocks(
    key: &str,
    value: &mut Value,
    keep: impl Fn(&Value) -> bool,
) -> bool {
    if key == SECTION_PAGINATION
        && let Some(sections) = value.as_object_mut()
    {
        sections.retain(|_, block| keep(block));
        return !sections.is_empty();
    }
    keep(value)
}

/// A row that states `coverage.state: "complete"` lists everything in its scope: any
/// `next.*` it carries is a drill-down, not unread rest.
#[must_use]
pub(crate) fn is_complete(data: &Value) -> bool {
    data.pointer("/coverage/state").and_then(Value::as_str) == Some("complete")
}

pub(crate) fn is_partial(data: &Value) -> bool {
    tree_some(data, &|map| {
        map.get("isPartial") == Some(&Value::Bool(true))
            || map.get("hasMore") == Some(&Value::Bool(true))
            || bounded(map)
    })
}

/// Whether `value` offers, under any `next` (never inside a call), a page
/// that leaves more of this result to read ([`crate::id::is_remaining`]).
#[must_use]
pub(crate) fn has_remaining_page(tool: ToolId, value: &Value) -> bool {
    has_call(value, &|name| crate::id::is_remaining(tool, name))
}

/// [`has_remaining_page`] other than a `retry`, which reads the same page
/// again rather than more of the result.
#[must_use]
pub(crate) fn has_page_beyond_retry(tool: ToolId, value: &Value) -> bool {
    has_call(value, &|name| {
        crate::id::kind(name) != "retry" && crate::id::is_remaining(tool, name)
    })
}

/// Whether `value` holds, at any depth, an executable `{tool, query}` call
/// under a key `name_matches` accepts.
fn has_call(value: &Value, name_matches: &impl Fn(&str) -> bool) -> bool {
    match value {
        Value::Object(map) => {
            if is_call(value) {
                return false;
            }
            map.iter().any(|(key, child)| {
                (name_matches(key) && is_call(child)) || has_call(child, name_matches)
            })
        }
        Value::Array(array) => array.iter().any(|v| has_call(v, name_matches)),
        _ => false,
    }
}

/// Codes of a row's coverage: for a `partial` row, a terminal limit or a
/// continuation missing for its rest; for any row, a continuation missing
/// for a clipped value none reaches.
pub(super) fn coverage_codes(data: &Value, partial: bool) -> Vec<String> {
    if partial {
        pagination_codes(data)
    } else if clipped_value_unreached(data) {
        vec!["continuationMissing".into()]
    } else {
        Vec::new()
    }
}

/// Codes of a partial row: a terminal limit, or a continuation missing.
fn pagination_codes(data: &Value) -> Vec<String> {
    if tree_some(data, &|m| {
        m.get("terminalLimit") == Some(&Value::Bool(true))
    }) {
        return vec!["terminalLimitReached".into()];
    }
    let pageable = tree_some(data, &|m| m.get("hasMore") == Some(&Value::Bool(true)));
    let partial = tree_some(data, &|m| m.get("isPartial") == Some(&Value::Bool(true)));
    let page = has_call(data, &crate::id::resumes_after_shown);
    let expansion = has_call(data, &|name| !crate::id::resumes_after_shown(name));
    if (pageable && !page)
        || ((tree_some(data, &bounded) || partial) && !page && !expansion)
        || clipped_value_unreached(data)
    {
        vec!["continuationMissing".into()]
    } else {
        vec![]
    }
}

/// Whether a value clipped inside a file row (the contract's `clipped:true`
/// under a row with a `path`) has no continuation reaching it: neither a
/// read or expansion of that file nor one that widens values
/// (`matchContentLength`). Page continuations (`next*`, `continue*`) list
/// more rows and never widen a shown one.
fn clipped_value_unreached(data: &Value) -> bool {
    fn clipped_paths<'a>(value: &'a Value, path: Option<&'a str>, out: &mut Vec<&'a str>) {
        match value {
            Value::Object(map) => {
                let path = map.get("path").and_then(Value::as_str).or(path);
                if map.get("clipped") == Some(&Value::Bool(true))
                    && let Some(path) = path
                {
                    out.push(path);
                }
                for (key, child) in map {
                    if key != "next" {
                        clipped_paths(child, path, out);
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| clipped_paths(item, path, out)),
            _ => {}
        }
    }
    fn reaching<'a>(value: &'a Value, paths: &mut Vec<&'a str>, widened: &mut bool) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    if is_call(child)
                        && let Some(query) = crate::response::result::continuation_row(child)
                            .and_then(Value::as_object)
                        && !crate::id::resumes_after_shown(key)
                    {
                        *widened |= query.contains_key("matchContentLength");
                        if let Some(path) = query.get("path").and_then(Value::as_str) {
                            paths.push(path);
                        }
                    }
                    reaching(child, paths, widened);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| reaching(item, paths, widened)),
            _ => {}
        }
    }
    let mut clipped = Vec::new();
    clipped_paths(data, None, &mut clipped);
    if clipped.is_empty() {
        return false;
    }
    let (mut targets, mut widened) = (Vec::new(), false);
    reaching(data, &mut targets, &mut widened);
    // Rows name files absolutely here; a continuation keeps the caller's
    // (workspace-relative) spelling of the same file.
    let same_file = |row: &str, target: &str| {
        let target = target.trim_start_matches("./");
        row == target
            || row
                .strip_suffix(target)
                .is_some_and(|prefix| prefix.ends_with('/'))
    };
    !widened
        && clipped
            .iter()
            .any(|path| !targets.iter().any(|target| same_file(path, target)))
}

fn tree_some(value: &Value, predicate: &impl Fn(&Map<String, Value>) -> bool) -> bool {
    match value {
        Value::Object(map) => predicate(map) || map.values().any(|v| tree_some(v, predicate)),
        Value::Array(array) => array.iter().any(|v| tree_some(v, predicate)),
        _ => false,
    }
}

/// Whether `record` says a count it states is capped: the contract's one
/// cap flag of a pagination block (`totalItemsCapped`).
fn bounded(record: &Map<String, Value>) -> bool {
    record.get("totalItemsCapped") == Some(&Value::Bool(true))
}

#[cfg(test)]
mod tests;
