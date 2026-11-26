//! Semantic snapshots that pin pages, and the item and document-symbol
//! payloads they cover.
use super::*;

pub(in super::super) fn semantic_snapshot(
    query: &LspSearchQuery,
    kind: &str,
    items: &[Value],
) -> String {
    salted_snapshot(query, kind, items, None)
}

/// [`semantic_snapshot`] that also digests `salt` when there is one.
pub(super) fn salted_snapshot(
    query: &LspSearchQuery,
    kind: &str,
    items: &[Value],
    salt: Option<&str>,
) -> String {
    let mut scope = lsp_search_query::to_row(query);
    if let Some(object) = scope.as_object_mut() {
        for field in ["page", "snapshot"] {
            object.remove(field);
        }
    }
    // Canonical form: a continuation lists the query's fields in a
    // different order than the caller did, and the digest must not care.
    let mut identity = json!({
        "query": scope,
        "operation": kind,
        "items": items,
    });
    if let Some(salt) = salt {
        identity["salt"] = json!(salt);
    }
    let canonical =
        octocode_engine::digest::canonical_json(identity, octocode_engine::digest::Nulls::Drop);
    // v3: object rows, 1-based columns, matchCount totals, pageSize on
    // every page.
    format!(
        "lsp-v3:{}",
        octocode_runtime_base::digest::json_sha256(&canonical)
    )
}

/// A later page whose `snapshot` no longer matches the current answer; a
/// page without one pins nothing and pages the current answer.
pub(in super::super) fn snapshot_mismatch(query: &LspSearchQuery, actual: &str) -> bool {
    lsp_search_query::page(query).unwrap_or(1) > 1
        && lsp_search_query::snapshot(query).is_some_and(|pinned| pinned != actual)
}

/// The stale-snapshot row; its restart walks from the first location page
/// of the first importer window.
pub(in super::super) fn snapshot_changed(query: &LspSearchQuery) -> Value {
    octocode_runtime_base::response::result::stale_snapshot(
        octocode_runtime_base::response::result::restart_query(
            ToolId::LspSearch,
            &lsp_search_query::to_row(query),
        ),
    )
}

pub(in super::super) fn items_payload(
    query: &LspSearchQuery,
    kind: &str,
    value: Value,
    budget: usize,
) -> Value {
    salted_items_payload(query, kind, value, None, budget, plain_sizes)
}

/// [`items_payload`] whose snapshot also covers `salt` (see
/// [`salted_locations`]) and whose rows cost a page what `sizes` says they
/// print as.
pub(in super::super) fn salted_items_payload(
    query: &LspSearchQuery,
    kind: &str,
    value: Value,
    salt: Option<&str>,
    budget: usize,
    sizes: impl FnOnce(&[Value]) -> Vec<usize>,
) -> Value {
    let raw_items = into_items(value);
    if kind == "documentSymbols" {
        return document_symbols_payload(query, &raw_items, budget);
    }
    let snapshot = salted_snapshot(query, kind, &raw_items, salt);
    if snapshot_mismatch(query, &snapshot) {
        return snapshot_changed(query);
    }
    if raw_items.is_empty() {
        if kind == "diagnostics" {
            return empty(query, "noDiagnostics", None, true);
        }
        return no_answer(
            query,
            "noLocations",
            &format!("{} returned no results", lsp_search_query::operation(query)),
        );
    }
    let sizes = sizes(&raw_items);
    let (page, pagination, next_page) = paginate(
        &raw_items,
        lsp_search_query::page(query).unwrap_or(1),
        &Pages::of(query, budget, &sizes, &snapshot),
    );
    let mut row = json!({
        "operation": lsp_search_query::operation(query),
        "matches": page,
        "lsp": { "serverAvailable": true },
        "pagination": pagination
    });
    offer_next_page(query, &mut row, next_page);
    row
}

pub(super) fn document_symbols_payload(
    query: &LspSearchQuery,
    raw_items: &[Value],
    budget: usize,
) -> Value {
    // An outline is declarations and their members; a callable's children
    // are its body's own symbols, listed only with `locals` and counted.
    let mut left_out = 0;
    let outline = if lsp_search_query::locals(query) {
        raw_items.to_vec()
    } else {
        raw_items
            .iter()
            .map(|item| without_body_symbols(item, &mut left_out))
            .collect()
    };
    let mut symbols = Vec::new();
    for item in &outline {
        flatten_document_symbol(item, &mut symbols, None);
    }
    symbols.sort_by_key(|symbol| {
        (
            symbol.get("line").and_then(Value::as_u64).unwrap_or(0),
            symbol.get("character").and_then(Value::as_u64).unwrap_or(0),
        )
    });
    let snapshot = semantic_snapshot(query, "documentSymbols", &symbols);
    if snapshot_mismatch(query, &snapshot) {
        return snapshot_changed(query);
    }
    // Pages count top-level rows, each with its members; a row costs what
    // it prints as, not its raw provider objects.
    let spans = octocode_runtime_base::response::symbol_outline::top_level_spans(&symbols);
    let sizes = spans
        .iter()
        .map(|span| {
            octocode_engine::json_size::utf16_units(&document_symbol_rows(
                &symbols[span.clone()],
                &[],
            ))
            .unwrap_or(0)
                + 1
        })
        .collect::<Vec<_>>();
    let (window, mut pagination, next_page) = page_window(
        lsp_search_query::page(query).unwrap_or(1),
        &Pages::of(query, budget, &sizes, &snapshot),
    );
    let page = if window.is_empty() {
        Vec::new()
    } else {
        symbols[spans[window.start].start..spans[window.end - 1].end].to_vec()
    };
    // A page is cut at top-level rows, but the outline serves each with its
    // members: the block counts the rows served, members included (as an
    // astSearch outline does), so a stated total is what the walk delivers.
    if let Some(block) = pagination.as_object_mut() {
        let before = spans
            .get(window.start)
            .map_or(symbols.len(), |span| span.start);
        for (key, rows) in [
            ("offset", before),
            ("length", page.len()),
            ("total", symbols.len()),
        ] {
            if let Some(stated) = block.get_mut(key) {
                *stated = json!(rows);
            }
        }
    }
    let mut row = json!({
        "operation": lsp_search_query::operation(query),
        "symbols": document_symbol_rows(&page, &symbols),
        "lsp": {
            "serverAvailable": true,
            "provider": "documentSymbolProvider"
        },
        "pagination": pagination
    });
    // What the outline left out is coverage: evidence rows keep warnings.
    if left_out > 0 {
        octocode_runtime_base::response::paging::push_warnings(
            &mut row,
            [format!(
                "{} inside function and method bodies left out: locals: true lists {}.",
                octocode_engine::text::counted(
                    u64::try_from(left_out).unwrap_or(u64::MAX),
                    "symbol",
                    "symbols"
                ),
                if left_out == 1 { "it" } else { "them" }
            )],
        );
    }
    offer_next_page(query, &mut row, next_page);
    row
}

/// The kinds whose children are a body's own symbols (its locals, the keys of
/// an object it builds), not the file's outline.
const CALLABLE_KINDS: [&str; 3] = ["function", "method", "constructor"];

/// `symbol` with every callable's children dropped, counted in `left_out`.
fn without_body_symbols(symbol: &Value, left_out: &mut usize) -> Value {
    let mut symbol = symbol.clone();
    let callable = CALLABLE_KINDS
        .contains(&crate::lsp_search::render::symbol_kind_name(symbol.get("kind")).as_str());
    if let Some(children) = symbol.get_mut("children").and_then(Value::as_array_mut) {
        if callable {
            *left_out += children.iter().map(with_descendants).sum::<usize>();
            children.clear();
        } else {
            for child in children.iter_mut() {
                *child = without_body_symbols(child, left_out);
            }
        }
    }
    symbol
}

/// A symbol and every symbol nested in it.
fn with_descendants(symbol: &Value) -> usize {
    1 + symbol["children"]
        .as_array()
        .map_or(0, |children| children.iter().map(with_descendants).sum())
}
