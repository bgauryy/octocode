//! Pure presentation helpers for `lspSearch`.
//!
//! Document-symbol flattening, LSP symbol-kind naming, generic result
//! pagination, and URI/path coercion. All functions are pure over
//! `serde_json::Value`; no request state, path policy, or client lives here.

use octocode_engine::lsp::uri::uri_to_path as engine_uri_to_path;
use serde_json::{Value, json};

pub(super) fn flatten_document_symbol(
    value: &Value,
    output: &mut Vec<Value>,
    container_name: Option<&str>,
) {
    let Some(symbol) = value.as_object() else {
        return;
    };
    let kind = symbol_kind_name(symbol.get("kind"));
    let range = symbol
        .get("range")
        .or_else(|| symbol.get("location")?.get("range"));
    if let (Some(name), Some(range)) = (symbol.get("name").and_then(Value::as_str), range) {
        // `line`/`character` point at the symbol NAME (`selectionRange`), not the
        // start of the full range (which includes doc comments/attributes), so
        // they can be fed back as `lineHint`. `endLine` keeps the full extent.
        let anchor = symbol.get("selectionRange").unwrap_or(range);
        let mut compact = json!({
            "name": name,
            "kind": kind,
            "line": anchor.pointer("/start/line").and_then(Value::as_u64).unwrap_or(0) + 1,
            "character": anchor.pointer("/start/character").and_then(Value::as_u64).unwrap_or(0),
            "endLine": range.pointer("/end/line").and_then(Value::as_u64).unwrap_or(0) + 1,
            "childCount": symbol.get("children").and_then(Value::as_array).map_or(0, Vec::len)
        });
        if let Some(container_name) = container_name {
            compact["containerName"] = json!(container_name);
        }
        output.push(compact);
    }
    let structural = matches!(
        kind.as_str(),
        "file"
            | "module"
            | "namespace"
            | "package"
            | "class"
            | "enum"
            | "interface"
            | "markdownHeading"
            | "struct"
    );
    if structural && let Some(children) = symbol.get("children").and_then(Value::as_array) {
        let parent = symbol
            .get("name")
            .and_then(Value::as_str)
            .or(container_name);
        for child in children {
            flatten_document_symbol(child, output, parent);
        }
    }
}

fn symbol_kind_name(kind: Option<&Value>) -> String {
    if let Some(kind) = kind.and_then(Value::as_str) {
        return kind.to_owned();
    }
    match kind.and_then(Value::as_u64) {
        Some(1) => "file",
        Some(2) => "module",
        Some(3) => "namespace",
        Some(4) => "package",
        Some(5) => "class",
        Some(6) => "method",
        Some(7) => "property",
        Some(8) => "field",
        Some(9) => "constructor",
        Some(10) => "enum",
        Some(11) => "interface",
        Some(12) => "function",
        Some(13) => "variable",
        Some(14) => "constant",
        Some(15) => "string",
        Some(16) => "number",
        Some(17) => "boolean",
        Some(18) => "array",
        Some(19) => "object",
        Some(20) => "key",
        Some(21) => "null",
        Some(22) => "enumMember",
        Some(23) => "struct",
        Some(24) => "event",
        Some(25) => "operator",
        Some(26) => "typeParameter",
        _ => "unknown",
    }
    .to_owned()
}

pub(super) fn paginate(items: &[Value], page: u32, page_size: u32) -> (Vec<Value>, Value) {
    let page_size = page_size.max(1);
    let total = items.len() as u32;
    let total_pages = total.div_ceil(page_size).max(1);
    // A page past the end is an empty, terminal, explicitly out-of-range page —
    // never silently clamped to the last page (which would duplicate results).
    let out_of_range = page > total_pages;
    let current = page.max(1);
    let start = if out_of_range {
        items.len()
    } else {
        ((current - 1) * page_size) as usize
    };
    let page_items = items
        .iter()
        .skip(start)
        .take(page_size as usize)
        .cloned()
        .collect::<Vec<_>>();
    let has_more = !out_of_range && current < total_pages;
    let mut pagination = json!({
        "currentPage": current,
        "totalPages": total_pages,
        "totalResults": total,
        "hasMore": has_more,
        "pageSize": page_size
    });
    if has_more {
        pagination["nextPage"] = json!(current + 1);
    }
    if out_of_range {
        pagination["outOfRange"] = json!(true);
    }
    (page_items, pagination)
}

pub(super) fn as_array(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(values) => values.clone(),
        Value::Null => vec![],
        other => vec![other.clone()],
    }
}

pub(super) fn decode_uri_path(uri: &str) -> Result<String, String> {
    if uri.starts_with("file:") {
        engine_uri_to_path(uri).map_err(|error| error.to_string())
    } else {
        Ok(uri.to_owned())
    }
}

pub(super) fn uri_to_path(uri: &str) -> String {
    decode_uri_path(uri).unwrap_or_else(|_| uri.to_owned())
}
