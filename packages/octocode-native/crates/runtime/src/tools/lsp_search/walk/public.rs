//! Public rows of walk nodes and edges: named kinds, one-based ranges, `via`.

use super::compact::CALLER_PATH;
use super::nodes::anchor_range;
use super::{Expansion, HierarchyEdge};
use crate::tools::lsp_search::locations::public_range;
use crate::tools::lsp_search::render::{symbol_kind_name, uri_to_path};
use serde_json::{Value, json};

/// Public hierarchy node (`CallHierarchyItem`/`TypeHierarchyItem`): named
/// kind, `uri`, and a one-based `displayRange` that starts at the symbol
/// name (usable as `line`) and ends with the full declaration. Opaque
/// server `data` and empty `detail` are dropped.
fn public_hierarchy_node(node: &Value) -> Value {
    let mut public = serde_json::Map::new();
    if let Some(name) = node.get("name") {
        public.insert("symbolName".into(), name.clone());
    }
    public.insert("kind".into(), json!(symbol_kind_name(node.get("kind"))));
    if let Some(detail) = node
        .get("detail")
        .and_then(Value::as_str)
        .filter(|detail| !detail.is_empty())
    {
        public.insert("detail".into(), json!(detail));
    }
    if let Some(uri) = node.get("uri").and_then(Value::as_str) {
        public.insert("path".into(), json!(uri_to_path(uri)));
    }
    let anchor = anchor_range(node);
    if let Some(mut display) = anchor.and_then(public_range) {
        if let Some(end) = node.pointer("/range/end/line").and_then(Value::as_u64) {
            display["endLine"] = json!(end + 1);
        }
        public.insert("displayRange".into(), display);
    }
    Value::Object(public)
}

/// `via`: name and one-based `line`/`character` of the parent node an edge
/// connects to (its symbol name, like `displayRange.start*`).
pub(super) fn public_via(node: &Value) -> Value {
    let anchor = anchor_range(node);
    let point = |field: &str| {
        anchor
            .and_then(|range| range.pointer(&format!("/start/{field}")))
            .and_then(Value::as_u64)
            .map(|value| value + 1)
    };
    json!({
        "symbolName": node.get("name"),
        "path": node.get("uri").and_then(Value::as_str).map(uri_to_path),
        "line": point("line"),
        "character": point("character")
    })
}

/// The public row of one walk edge. `narrowed` is the incoming edge's
/// class caller narrowed by [`ClassFacts::narrow`](super::ClassFacts::narrow); the walk itself keeps
/// the server's item.
pub(in crate::tools::lsp_search) fn public_edge(
    expansion: Expansion,
    edge: &HierarchyEdge,
    narrowed: Option<&Value>,
) -> Value {
    let mut public = if expansion.is_call() {
        let key = if expansion == Expansion::IncomingCalls {
            "from"
        } else {
            "to"
        };
        // Call-site ranges: in the caller's file for incoming calls, in the
        // expanded (parent or anchor) node's file for outgoing calls.
        let mut ranges = edge
            .sites
            .iter()
            .filter_map(public_range)
            .collect::<Vec<_>>();
        ranges.sort_by_key(|range| {
            (
                range["startLine"].as_u64().unwrap_or(0),
                range["startCharacter"].as_u64().unwrap_or(0),
                range["endLine"].as_u64().unwrap_or(0),
            )
        });
        // Servers can report one call site as several raw ranges that differ
        // only in their end column; the public range drops that column, so
        // identical public sites are one call.
        ranges.dedup();
        let mut call = serde_json::Map::new();
        let node = narrowed.filter(|_| expansion == Expansion::IncomingCalls);
        call.insert(
            key.into(),
            public_hierarchy_node(node.unwrap_or(&edge.node)),
        );
        call.insert("fromRanges".into(), json!(ranges));
        // An outgoing call site lies in the expanded node's file; the
        // compact row is filed there.
        if expansion == Expansion::OutgoingCalls
            && let Some(uri) = edge
                .parent
                .as_ref()
                .and_then(|parent| parent.get("uri"))
                .and_then(Value::as_str)
        {
            call.insert(CALLER_PATH.into(), json!(uri_to_path(uri)));
        }
        Value::Object(call)
    } else {
        public_hierarchy_node(&edge.node)
    };
    public["level"] = json!(edge.level);
    if edge.level > 1
        && let Some(parent) = &edge.parent
    {
        public["via"] = public_via(parent);
    }
    public
}
