//! Pure presentation helpers for `lspSearch`.
//!
//! Document-symbol flattening, LSP symbol-kind naming, generic result
//! pagination, and URI/path coercion. All functions are pure over
//! `serde_json::Value`; no request state, path policy, or client lives here.

use octocode_engine::symbol_kind;
use serde_json::{Value, json};

use crate::response::page_windows::{Paging, select};
use octocode_contract_types::LspSearchQuery;

/// One flattened document symbol: `name`, `kind`, one-based `line` (the
/// name line, a usable `line`) and `endLine` (the full extent), the
/// one-based name `character`, and `parent`/`parentLine` for a nested
/// symbol (a member, or a function's local). Every symbol is listed.
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
            // 1-based, like every public column.
            "character": anchor.pointer("/start/character").and_then(Value::as_u64).unwrap_or(0) + 1,
            "endLine": range.pointer("/end/line").and_then(Value::as_u64).unwrap_or(0) + 1,
        });
        if let Some((parent, parent_line)) = parent {
            compact["parent"] = json!(parent);
            compact["parentLine"] = json!(parent_line);
        }
        output.push(compact);
        line = Some(start);
    }
    let parent = match (name, line) {
        (Some(name), Some(line)) => Some((name, line)),
        _ => parent,
    };
    for child in children {
        flatten_document_symbol(child, output, parent);
    }
}

/// Outline rows (`response::symbol_outline`) for a page of flattened
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
    crate::response::symbol_outline::declaration_rows(&objects)
}

/// typescript-language-server reports a `type X = …` alias as a variable
/// (LSP has no alias kind). Name each such symbol `type` when its declaring
/// line says so; nested symbols are walked too.
pub(super) fn name_type_aliases(symbols: &mut Value, content: &str) {
    let lines: Vec<&str> = content.lines().collect();
    fn walk(symbols: &mut Value, lines: &[&str]) {
        let Some(items) = symbols.as_array_mut() else {
            return;
        };
        for symbol in items {
            if symbol["kind"].as_u64() == Some(u64::from(symbol_kind::VARIABLE))
                && let Some(name) = symbol["name"].as_str()
                && let Some(line) = symbol
                    .pointer("/selectionRange/start/line")
                    .or_else(|| symbol.pointer("/range/start/line"))
                    .and_then(Value::as_u64)
                    .and_then(|line| lines.get(usize::try_from(line).ok()?))
                && declares_type_alias(line, name)
            {
                symbol["kind"] = json!("type");
            }
            if let Some(children) = symbol.get_mut("children") {
                walk(children, lines);
            }
        }
    }
    walk(symbols, &lines);
}

/// `[export] [declare] type <name>` at the start of a line.
fn declares_type_alias(line: &str, name: &str) -> bool {
    let mut rest = line.trim_start();
    for keyword in ["export ", "declare "] {
        if let Some(after) = rest.strip_prefix(keyword) {
            rest = after.trim_start();
        }
    }
    rest.strip_prefix("type ")
        .map(str::trim_start)
        .and_then(|after| after.strip_prefix(name))
        .is_some_and(|after| {
            after
                .chars()
                .next()
                .is_none_or(|c| !octocode_engine::text::is_ident_char(c))
        })
}

pub(super) fn symbol_kind_name(kind: Option<&Value>) -> String {
    if let Some(kind) = kind.and_then(Value::as_str) {
        return kind.to_owned();
    }
    kind.and_then(Value::as_u64)
        .and_then(symbol_kind::name)
        .unwrap_or("unknown")
        .to_owned()
}

/// Evidence rows cut by the character budget, pinned to one result digest.
pub(super) struct Pages<'a> {
    pub(super) paging: Paging,
    pub(super) sizes: &'a [usize],
    pub(super) snapshot: &'a str,
}

impl<'a> Pages<'a> {
    /// Fit complete rows within the character budget, at most the query's
    /// `limit` rows.
    pub(super) fn of(
        query: &LspSearchQuery,
        budget: usize,
        sizes: &'a [usize],
        snapshot: &'a str,
    ) -> Self {
        Self {
            paging: Paging::of(super::lsp_search_query::limit(query), budget),
            sizes,
            snapshot,
        }
    }
}

/// The page a continuation asks for next, and the snapshot that pins it. The
/// cursor rides `next.nextPage` (see [`super::failure::offer_next_page`]),
/// never the pagination block.
pub(super) struct NextPage {
    pub(super) page: usize,
    pub(super) snapshot: String,
}

/// One page of `items`, with the cursor of the page after it when there is
/// one. A page past the end is empty, terminal and flagged `outOfRange`,
/// never clamped to the last page, which would repeat rows the caller has.
pub(super) fn paginate(
    items: &[Value],
    page: u32,
    pages: &Pages<'_>,
) -> (Vec<Value>, Value, Option<NextPage>) {
    let (range, pagination, next) = page_window(page, pages);
    (items[range].to_vec(), pagination, next)
}

/// The item range of one page over the `pages.sizes` items, with its
/// pagination facts (see [`paginate`]) and the cursor of the next page.
///
/// The cut is a pure function of the result set and the request's own
/// fields (the budget is pinned per tool, whatever the batch around the
/// call), so the next page is the same page number under the same digest.
pub(super) fn page_window(
    page: u32,
    pages: &Pages<'_>,
) -> (std::ops::Range<usize>, Value, Option<NextPage>) {
    let page = page.max(1) as usize;
    let cut = select(pages.paging, pages.sizes, page);
    let next = cut.more.then(|| NextPage {
        page: page + 1,
        snapshot: pages.snapshot.to_owned(),
    });
    (cut.range, cut.facts.to_value(), next)
}

/// Characters each row costs a page when it prints as its own JSON.
pub(super) fn plain_sizes(items: &[Value]) -> Vec<usize> {
    items
        .iter()
        .map(|item| octocode_engine::json_size::utf16_units(item).unwrap_or(0) + 1)
        .collect()
}

/// An LSP result's items: an array's elements, one item, or none for `null`.
pub(super) fn items(value: &Value) -> &[Value] {
    match value {
        Value::Array(values) => values,
        Value::Null => &[],
        other => std::slice::from_ref(other),
    }
}

/// [`items`] of an owned result, moved out instead of cloned.
pub(super) fn into_items(value: Value) -> Vec<Value> {
    match value {
        Value::Array(values) => values,
        Value::Null => vec![],
        other => vec![other],
    }
}

pub(super) fn decode_uri_path(uri: &str) -> Result<String, String> {
    if uri.starts_with("file:") {
        octocode_engine::lsp::uri::uri_to_path(uri).map_err(|error| error.to_string())
    } else {
        Ok(uri.to_owned())
    }
}

pub(super) fn uri_to_path(uri: &str) -> String {
    decode_uri_path(uri).unwrap_or_else(|_| uri.to_owned())
}
