//! Public rows of walk nodes and edges: named kinds, one-based ranges, `via`.

use super::compact::CALLER_PATH;
use super::nodes::anchor_range;
use super::{Expansion, HierarchyEdge};
use crate::lsp_search::locations::{insert_place, place_end, public_range};
use crate::lsp_search::render::{label_name, symbol_kind_name, uri_to_path};
use serde_json::{Value, json};

/// Internal to a walk node: the label a name was cut from, which a public row
/// states in its `detail` ([`signed_detail`]).
pub(super) const SIGNATURE: &str = "signature";

/// `detail` and `signature` as the one `detail` a row states.
pub(super) fn signed_detail(detail: Option<&str>, signature: Option<&str>) -> Option<String> {
    match (detail, signature) {
        (Some(detail), Some(signature)) => Some(format!("{detail} {signature}")),
        (Some(text), None) | (None, Some(text)) => Some(text.to_owned()),
        (None, None) => None,
    }
}

/// Public hierarchy node (`CallHierarchyItem`/`TypeHierarchyItem`): named
/// kind, `uri`, and a one-based place that starts at the symbol name
/// (`line`, `column`) and ends with the full declaration (`endLine`). Opaque
/// server `data` and empty `detail` are dropped.
fn public_hierarchy_node(node: &Value) -> Value {
    let mut public = serde_json::Map::new();
    let (name, signature) =
        node.get("name")
            .and_then(Value::as_str)
            .map_or((None, None), |label| {
                let (name, signature) = label_name(label);
                (Some(name), signature)
            });
    if let Some(name) = name {
        public.insert("symbolName".into(), json!(name));
    } else if let Some(name) = node.get("name") {
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
    if let Some(signature) = signature {
        public.insert(SIGNATURE.into(), json!(signature));
    }
    if let Some(uri) = node.get("uri").and_then(Value::as_str) {
        public.insert("path".into(), json!(uri_to_path(uri)));
    }
    let anchor = anchor_range(node);
    if let Some(mut place) = anchor.and_then(public_range) {
        if let Some(end) = node.pointer("/range/end/line").and_then(Value::as_u64) {
            place_end(&mut place, end + 1);
        }
        insert_place(&mut public, place);
    }
    Value::Object(public)
}

/// `via`: name and one-based `line`/`column` of the parent node an edge
/// connects to (its symbol name, like a node's own place).
pub(super) fn public_via(node: &Value) -> Value {
    let anchor = anchor_range(node);
    let point = |field: &str| {
        anchor
            .and_then(|range| range.pointer(&format!("/start/{field}")))
            .and_then(Value::as_u64)
            .map(|value| value + 1)
    };
    json!({
        "symbolName": node.get("name").and_then(Value::as_str).map(|label| label_name(label).0),
        "path": node.get("uri").and_then(Value::as_str).map(uri_to_path),
        "line": point("line"),
        "column": point("character")
    })
}

/// The public row of one walk edge. `narrowed` is the incoming edge's
/// class caller narrowed by [`ClassFacts::narrow`](super::ClassFacts::narrow); the walk itself keeps
/// the server's item.
pub(in crate::lsp_search) fn public_edge(
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
                range["line"].as_u64().unwrap_or(0),
                range["column"].as_u64().unwrap_or(0),
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
        let mut node = public_hierarchy_node(&edge.node);
        // Only a call row weighs a signature by the row around it.
        if let Some(row) = node.as_object_mut()
            && let Some(signature) = row.remove(SIGNATURE)
        {
            let detail = row.get("detail").and_then(Value::as_str);
            row.insert(
                "detail".into(),
                json!(signed_detail(detail, signature.as_str())),
            );
        }
        node
    };
    public["level"] = json!(edge.level);
    if edge.level > 1
        && let Some(parent) = &edge.parent
    {
        public["via"] = public_via(parent);
    }
    public
}

#[cfg(test)]
mod tests;
