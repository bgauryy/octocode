//! Pure presentation helpers for `lspSearch`.
//!
//! Document-symbol flattening, LSP symbol-kind naming, generic result
//! pagination, and URI/path coercion. All functions are pure over
//! `serde_json::Value`; no request state, path policy, or client lives here.

use octocode_engine::lsp::uri::uri_to_path as engine_uri_to_path;
use serde_json::{Value, json};

/// One flattened document symbol: `name`, `kind`, one-based `line` (the
/// name line, a usable `lineHint`) and `endLine` (the full extent), the
/// zero-based name `character`, `parent`/`parentLine` for a member, and
/// `unlisted`, the children of a symbol whose members are not listed (a
/// function's locals).
pub(super) fn flatten_document_symbol(
    value: &Value,
    output: &mut Vec<Value>,
    parent: Option<(&str, u64)>,
) {
    let Some(symbol) = value.as_object() else {
        return;
    };
    let kind = symbol_kind_name(symbol.get("kind"));
    let range = symbol
        .get("range")
        .or_else(|| symbol.get("location")?.get("range"));
    let name = symbol.get("name").and_then(Value::as_str);
    // Rust `impl` blocks are `object` symbols.
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
            | "object"
    );
    let children = symbol
        .get("children")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut line = None;
    if let (Some(name), Some(range)) = (name, range) {
        let anchor = symbol.get("selectionRange").unwrap_or(range);
        let start = anchor
            .pointer("/start/line")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            + 1;
        let mut compact = json!({
            "name": name,
            "kind": kind,
            "line": start,
            "character": anchor.pointer("/start/character").and_then(Value::as_u64).unwrap_or(0),
            "endLine": range.pointer("/end/line").and_then(Value::as_u64).unwrap_or(0) + 1,
        });
        if let Some((parent, parent_line)) = parent {
            compact["parent"] = json!(parent);
            compact["parentLine"] = json!(parent_line);
        }
        if !structural && !children.is_empty() {
            compact["unlisted"] = json!(children.len());
        }
        output.push(compact);
        line = Some(start);
    }
    if structural {
        let parent = match (name, line) {
            (Some(name), Some(line)) => Some((name, line)),
            _ => parent,
        };
        for child in children {
            flatten_document_symbol(child, output, parent);
        }
    }
}

/// Outline rows (`tools::symbol_outline`) for a page of flattened
/// document symbols: `endLine` only when it differs, and the column only
/// when another listed symbol starts on the same line.
pub(super) fn document_symbol_rows(page: &[Value], all: &[Value]) -> Vec<Value> {
    let mut lines = std::collections::HashMap::<u64, usize>::new();
    for symbol in all {
        *lines
            .entry(symbol["line"].as_u64().unwrap_or(0))
            .or_default() += 1;
    }
    let objects = page
        .iter()
        .map(|symbol| {
            let line = symbol["line"].as_u64().unwrap_or(0);
            let mut row = json!({"name": symbol["name"], "kind": symbol["kind"], "line": line});
            if symbol["endLine"].as_u64().is_some_and(|end| end != line) {
                row["endLine"] = symbol["endLine"].clone();
            }
            if lines.get(&line).is_some_and(|count| *count > 1) {
                row["character"] = symbol["character"].clone();
            }
            for key in ["parent", "parentLine"] {
                if let Some(value) = symbol.get(key) {
                    row[key] = value.clone();
                }
            }
            row
        })
        .collect::<Vec<_>>();
    crate::tools::symbol_outline::outline_rows(&objects)
}

pub(super) fn symbol_kind_name(kind: Option<&Value>) -> String {
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
        "totalItems": total,
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

/// Language ids served by the TypeScript server.
pub(super) const TS_LANGUAGE_IDS: [&str; 4] = [
    "typescript",
    "typescriptreact",
    "javascript",
    "javascriptreact",
];

/// A word-bounded literal regex for `name` in the default (`rust`) engine.
pub(super) fn word_pattern(name: &str) -> String {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut pattern = regex::escape(name);
    if word(name.chars().next()) {
        pattern.insert_str(0, "\\b");
    }
    if word(name.chars().last()) {
        pattern.push_str("\\b");
    }
    pattern
}
