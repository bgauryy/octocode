//! Pure presentation helpers for `lspSearch`.
//!
//! Document-symbol flattening, LSP symbol-kind naming, generic result
//! pagination, and URI/path coercion. All functions are pure over
//! `serde_json::Value`; no request state, path policy, or client lives here.

use octocode_engine::lsp::uri::uri_to_path as engine_uri_to_path;
use serde_json::{Value, json};

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
            if symbol["kind"].as_u64() == Some(13)
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
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '$'))
        })
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

/// How one page is cut: the caller's explicit `pageSize` (a row count), else
/// as many rows as fit `budget` characters, each row costing its `sizes`
/// entry. `snapshot` is the result set's digest; `resume` the first row a
/// copied character-cut continuation names (see [`page_window`]).
pub(super) struct PageCut<'a> {
    pub(super) page_size: Option<u32>,
    pub(super) budget: usize,
    pub(super) sizes: &'a [usize],
    pub(super) snapshot: &'a str,
    pub(super) resume: Option<usize>,
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
    cut: &PageCut<'_>,
) -> (Vec<Value>, Value, Option<NextPage>) {
    let (range, pagination, next) = page_window(page, cut);
    (items[range].to_vec(), pagination, next)
}

/// The item range of one page over the `cut.sizes` items, with its
/// pagination facts (see [`paginate`]) and the cursor of the next page.
///
/// A character cut depends on the response window, which differs between
/// batches, so the next page's snapshot also names the row it starts at
/// (`{digest}@{row}.{check}`, see [`row_cursor`]): a continuation run under
/// any window resumes at exactly that row. A count cut is the same under
/// every window (its size is in the digest), so its snapshot is the digest
/// alone.
pub(super) fn page_window(
    page: u32,
    cut: &PageCut<'_>,
) -> (std::ops::Range<usize>, Value, Option<NextPage>) {
    use crate::response::page_windows::Paging;
    use crate::response::pages::PageFacts;
    let total = cut.sizes.len();
    let page = (page.max(1)) as usize;
    let paging = Paging::of(cut.page_size.map(|size| size as usize), cut.budget);
    let (range, pages) = match (paging, cut.resume) {
        (Paging::Chars(_), Some(start)) if page > 1 => {
            let start = start.min(total);
            let rest = paging.windows(&cut.sizes[start..]);
            let range = rest.first().map_or(total..total, |window| {
                start + window.start..start + window.end
            });
            (range, page - 1 + rest.len())
        }
        _ => {
            let windows = paging.windows(cut.sizes);
            let range = windows.get(page - 1).cloned().unwrap_or(total..total);
            (range, windows.len())
        }
    };
    let more = page < pages;
    let facts = match paging {
        Paging::Count(size) => PageFacts::counted(page, size, total),
        Paging::Chars(_) => PageFacts::sized(page, pages, total, more),
    };
    let pagination = facts.out_of_range(page > pages.max(1)).to_value();
    let next = more.then(|| NextPage {
        page: page + 1,
        snapshot: match paging {
            Paging::Chars(_) => row_cursor(cut.snapshot, range.end, page + 1),
            Paging::Count(_) => cut.snapshot.to_owned(),
        },
    });
    (range, pagination, next)
}

/// The cursor a character-cut continuation names: the digest, the row
/// `page` starts at, and a check over the three, so an edited row or the
/// cursor of another page binds to no page (see [`read_row_cursor`]).
pub(super) fn row_cursor(digest: &str, row: usize, page: usize) -> String {
    format!("{digest}@{row}.{}", cursor_check(digest, row, page))
}

/// Hex digits of the SHA-256 over a cursor's digest, row and page that the
/// cursor carries: short enough to keep a snapshot within its contract
/// length.
const CURSOR_CHECK_HEX: usize = 8;

fn cursor_check(digest: &str, row: usize, page: usize) -> String {
    let mut check = octocode_engine::digest::sha256(format!("{digest}\n{row}\n{page}").as_bytes());
    check.truncate(CURSOR_CHECK_HEX);
    check
}

/// A pinned snapshot's result-set digest and, for a character-cut
/// continuation of `page`, the row that page starts at (see
/// [`page_window`]). `None` when the snapshot names a row its check does not
/// bind to this digest and page: an edited cursor, or another page's.
pub(super) fn read_row_cursor(pinned: &str, page: usize) -> Option<(&str, Option<usize>)> {
    let Some((digest, cursor)) = pinned.rsplit_once('@') else {
        return Some((pinned, None));
    };
    let (row, check) = cursor.split_once('.')?;
    let row = row.parse().ok()?;
    (check == cursor_check(digest, row, page)).then_some((digest, Some(row)))
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
        engine_uri_to_path(uri).map_err(|error| error.to_string())
    } else {
        Ok(uri.to_owned())
    }
}

pub(super) fn uri_to_path(uri: &str) -> String {
    decode_uri_path(uri).unwrap_or_else(|_| uri.to_owned())
}

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
