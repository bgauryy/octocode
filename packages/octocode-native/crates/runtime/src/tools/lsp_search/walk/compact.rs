//! Call edges compacted to per-file call rows, and the page sizes they print as.

use crate::policy::path::PathPolicy;
use crate::tools::lsp_search::LspSearchQuery;
use crate::tools::lsp_search::locations::salted_items_payload;
use crate::tools::lsp_search::lsp_search_query;
use crate::tools::lsp_search::render::uri_to_path;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// Internal key of a compacted outgoing call: the file its sites lie in.
pub(super) const CALLER_PATH: &str = "callerPath";

/// The file a hierarchy node or `via` names.
fn node_path(node: &Value) -> String {
    node.get("path")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| node.get("uri").and_then(Value::as_str).map(uri_to_path))
        .unwrap_or_else(|| "unknown".to_owned())
}

/// `target` as the response spells row paths: relative to the workspace
/// root when inside it, absolute otherwise.
fn display_path(paths: &PathPolicy, target: &str) -> String {
    paths
        .workspace_relative(target)
        .unwrap_or_else(|| target.to_owned())
}

/// Declaration words a signature detail may repeat around the name.
const DETAIL_KEYWORDS: &[&str] = &[
    "pub", "crate", "super", "async", "unsafe", "const", "extern", "static", "fn", "function",
    "def", "func", "export", "default",
];

/// Whether a call-hierarchy `detail` says more than the row already does:
/// a container name, parameters, or a return type. A detail that is only
/// the name, its kind, and declaration words (`fn walk()`) adds nothing.
fn detail_adds(detail: &str, node: &Value) -> bool {
    let name = node
        .get("symbolName")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let kind = node.get("kind").and_then(Value::as_str).unwrap_or_default();
    detail
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .filter(|token| !token.is_empty())
        .any(|token| token != name && token != kind && !DETAIL_KEYWORDS.contains(&token))
}

/// A page of call edges (`callers`, `callees`, any depth) as per-file rows
/// `{path, matches: [call row]}`, filed under the file the call sites lie in
/// A call row is `{symbolName, kind, line, endLine?, path?, sites,
/// detail?, via?, source?}`: for `callers` the caller declared in that file,
/// for `callees` the callee called from it (the anchor, or the `via` node).
/// `line`–`endLine` is the declaration's range from its name line (a
/// `line` for the next hop); `path` names the declaration's file when it
/// differs from the row's; `sites` are the 1-based call sites in the row's
/// file. An edge below level 1 names its parent as `via: {symbolName, line}`
/// (with `path` when another listed node shares that name and line). A
/// caller recovered from references carries its label as `source`. `all` is
/// every edge of the walk (not only this page), for that disambiguation.
pub(in crate::tools::lsp_search) fn compact_calls(
    row: &mut Value,
    all: &[Value],
    paths: &PathPolicy,
) {
    let Some(items) = row.pointer("/matches").and_then(Value::as_array) else {
        return;
    };
    if items.is_empty() || items.iter().any(|item| call_node(item).is_none()) {
        return;
    }
    let declared = declaring_files(all.iter().chain(items));
    let mut files: Vec<(CallGroup, serde_json::Map<String, Value>)> = Vec::new();
    for item in items {
        let Some((group, call)) = filed_call(item, &declared, paths) else {
            continue;
        };
        // One entry per (direction, file).
        let index = match files.iter().position(|(seen, _)| *seen == group) {
            Some(index) => index,
            None => {
                let mut entry = serde_json::Map::new();
                entry.insert("path".into(), json!(group.1));
                entry.insert("matches".into(), json!([]));
                files.push((group, entry));
                files.len() - 1
            }
        };
        if let Some(calls) = files[index]
            .1
            .get_mut("matches")
            .and_then(Value::as_array_mut)
        {
            calls.push(call);
        }
    }
    if let Some(payload) = row.as_object_mut() {
        payload.remove("matches");
        payload.insert(
            "files".into(),
            Value::Array(
                files
                    .into_iter()
                    .map(|(_, entry)| Value::Object(entry))
                    .collect(),
            ),
        );
    }
}

/// A compact file entry's identity: direction key and file.
type CallGroup = (&'static str, String);

/// The file entry a call item is filed under and its call row (see
/// [`compact_calls`]); `None` for an item that is not a call edge.
fn filed_call(
    item: &Value,
    declared: &HashMap<(String, u64), HashSet<String>>,
    paths: &PathPolicy,
) -> Option<(CallGroup, Value)> {
    let (key, node) = call_node(item)?;
    let declared_in = node_path(node);
    // Call sites lie in the caller: the listed node for incoming calls,
    // the expanded node for outgoing ones.
    let path = match key {
        "to" => item
            .get(CALLER_PATH)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| item.get("via").filter(|via| via.is_object()).map(node_path))
            .unwrap_or_else(|| declared_in.clone()),
        _ => declared_in.clone(),
    };
    let sites = call_sites(item);
    let call = call_row(node, item, &sites, (&path, &declared_in), declared, paths);
    Some(((key, path), call))
}

/// One page of hierarchy `items` (see [`salted_items_payload`]), call
/// edges compacted to per-file call rows (see [`compact_calls`]). A page is
/// cut on what its rows print as, so it fills `budget`.
pub(in crate::tools::lsp_search) fn call_items_payload(
    query: &LspSearchQuery,
    items: &[Value],
    salt: Option<&str>,
    budget: usize,
    paths: &PathPolicy,
) -> Value {
    let (sizes, reserve) = compact_call_sizes(items, paths);
    // A page that opens inside a file's run of calls repeats its file entry,
    // which the sizes charge only to the run's first call.
    let mut row = salted_items_payload(
        query,
        lsp_search_query::operation(query),
        json!(items),
        salt,
        budget.saturating_sub(reserve).max(1),
        |_| sizes,
    );
    compact_calls(&mut row, items, paths);
    row
}

/// Characters each call edge costs a page as the compact row it prints as:
/// its call row, plus its file entry when the edge before it is filed
/// elsewhere; and the largest file entry. Items that are not all call
/// edges print as themselves.
fn compact_call_sizes(items: &[Value], paths: &PathPolicy) -> (Vec<usize>, usize) {
    if items.iter().any(|item| call_node(item).is_none()) {
        return (crate::tools::lsp_search::render::plain_sizes(items), 0);
    }
    let declared = declaring_files(items.iter());
    let mut previous: Option<CallGroup> = None;
    let mut largest = 0;
    let sizes = items
        .iter()
        .map(|item| {
            let Some((group, call)) = filed_call(item, &declared, paths) else {
                return octocode_engine::json_size::utf16_units(item).unwrap_or(0) + 1;
            };
            let entry =
                octocode_engine::json_size::utf16_units(&json!({"path": group.1, "matches": []}))
                    .unwrap_or(0)
                    + 1;
            largest = largest.max(entry);
            let opens = previous.as_ref() != Some(&group);
            previous = Some(group);
            octocode_engine::json_size::utf16_units(&call).unwrap_or(0)
                + 1
                + if opens { entry } else { 0 }
        })
        .collect();
    (sizes, largest)
}

/// The direction key (`from` or `to`) and node of a call item.
fn call_node(item: &Value) -> Option<(&'static str, &Value)> {
    ["from", "to"].into_iter().find_map(|key| {
        item.get(key)
            .filter(|node| node.is_object())
            .map(|node| (key, node))
    })
}

/// Files declaring each listed (name, line), over the whole walk.
fn declaring_files<'a>(
    items: impl Iterator<Item = &'a Value>,
) -> HashMap<(String, u64), HashSet<String>> {
    let mut declared: HashMap<(String, u64), HashSet<String>> = HashMap::new();
    for item in items {
        if let Some((_, node)) = call_node(item)
            && let (Some(name), Some(line)) = (
                node.get("symbolName").and_then(Value::as_str),
                node.pointer("/displayRange/startLine")
                    .and_then(Value::as_u64),
            )
        {
            declared
                .entry((name.to_owned(), line))
                .or_default()
                .insert(node_path(node));
        }
    }
    declared
}

/// One-based `(line, column)` call sites of an item.
fn call_sites(item: &Value) -> Vec<(u64, Option<u64>)> {
    item.get("fromRanges")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|range| {
            let line = range.get("startLine")?.as_u64()?;
            let column = range.get("startCharacter").and_then(Value::as_u64);
            Some((line, column))
        })
        .collect()
}

/// One call row filed under `path` (see [`compact_calls`]).
fn call_row(
    node: &Value,
    item: &Value,
    sites: &[(u64, Option<u64>)],
    (path, declared_in): (&str, &str),
    declared: &HashMap<(String, u64), HashSet<String>>,
    paths: &PathPolicy,
) -> Value {
    let name = node
        .get("symbolName")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let kind = node.get("kind").and_then(Value::as_str).unwrap_or("symbol");
    let mut row = serde_json::Map::new();
    // A module or file "caller" is top-level code, named by its file: that
    // name anchors no lspSearch call, so the row carries no `symbolName`.
    if !matches!(kind, "module" | "file") {
        row.insert("symbolName".into(), json!(name));
    }
    row.insert("kind".into(), json!(kind));
    if let Some(range) = node.get("displayRange")
        && let Some(start) = range.get("startLine").and_then(Value::as_u64)
    {
        row.insert("line".into(), json!(start));
        if let Some(end) = range
            .get("endLine")
            .and_then(Value::as_u64)
            .filter(|end| *end != start)
        {
            row.insert("endLine".into(), json!(end));
        }
    }
    // A callee declared in another file names it, so its range is a
    // `line` there.
    if declared_in != path {
        row.insert("path".into(), json!(display_path(paths, declared_in)));
    }
    row.insert(
        "sites".into(),
        Value::Array(
            sites
                .iter()
                .map(|(line, column)| match column {
                    Some(column) => json!({"line": line, "column": column}),
                    None => json!({"line": line}),
                })
                .collect(),
        ),
    );
    // A container name (`App`) always helps; a signature only tells apart
    // listed nodes that share a name (the read lead shows it otherwise).
    let shared_name = declared
        .iter()
        .filter(|((listed, _), _)| listed == name)
        .map(|(_, files)| files.len())
        .sum::<usize>()
        > 1;
    if let Some(detail) = node
        .get("detail")
        .and_then(Value::as_str)
        .filter(|detail| detail_adds(detail, node))
        .filter(|detail| shared_name || !detail.contains('('))
    {
        // One line per call row: a multi-line signature's whitespace runs
        // collapse to single spaces.
        row.insert(
            "detail".into(),
            json!(detail.split_whitespace().collect::<Vec<_>>().join(" ")),
        );
    }
    if let Some(via) = item.get("via").filter(|via| via.is_object()) {
        let via_name = via
            .get("symbolName")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let line = via.get("line").and_then(Value::as_u64).unwrap_or(0);
        let mut public = json!({"symbolName": via_name, "line": line});
        let ambiguous = declared
            .get(&(via_name.to_owned(), line))
            .is_some_and(|files| files.len() > 1);
        if ambiguous {
            public["path"] = json!(display_path(paths, &node_path(via)));
        }
        row.insert("via".into(), public);
    }
    if let Some(label) = item.get("source").and_then(Value::as_str) {
        row.insert("source".into(), json!(label));
    }
    Value::Object(row)
}
