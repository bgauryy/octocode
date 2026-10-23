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
        for field in crate::response::result::INTENT_FIELDS
            .iter()
            .chain(&["page", "snapshot"])
        {
            object.remove(*field);
        }
    }
    // Canonical form: a continuation lists the query's fields in a
    // different order than the caller did, and the digest must not care.
    let mut identity = json!({
        "query": scope,
        "kind": kind,
        "items": items,
    });
    if let Some(salt) = salt {
        identity["salt"] = json!(salt);
    }
    let canonical =
        octocode_engine::digest::canonical_json(identity, octocode_engine::digest::Nulls::Drop);
    // v3: object rows, 1-based columns, matchCount totals, pageSize on
    // every page.
    format!("lsp-v3:{}", crate::digest::json_sha256(&canonical))
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
    crate::response::result::stale_snapshot(crate::response::result::restart_query(
        ToolId::LspSearch,
        &lsp_search_query::to_row(query),
    ))
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
            return empty(
                query,
                "noDiagnostics",
                "The language server reported no diagnostics for this document.",
                true,
            );
        }
        return empty(
            query,
            "noLocations",
            &format!("{kind} returned no results"),
            true,
        );
    }
    let sizes = sizes(&raw_items);
    let (page, pagination, next_page) = paginate(
        &raw_items,
        lsp_search_query::page(query).unwrap_or(1),
        &Pages::of(query, budget, &sizes, &snapshot),
    );
    let mut row = json!({
        "kind": kind,
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
    let mut symbols = Vec::new();
    for item in raw_items {
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
    let spans = crate::response::symbol_outline::top_level_spans(&symbols);
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
    let (window, pagination, next_page) = page_window(
        lsp_search_query::page(query).unwrap_or(1),
        &Pages::of(query, budget, &sizes, &snapshot),
    );
    let page = if window.is_empty() {
        Vec::new()
    } else {
        symbols[spans[window.start].start..spans[window.end - 1].end].to_vec()
    };
    let mut row = json!({
        "kind": "documentSymbols",
        "symbols": document_symbol_rows(&page, &symbols),
        "lsp": {
            "serverAvailable": true,
            "provider": "documentSymbolProvider",
            "source": "lsp"
        },
        "pagination": pagination
    });
    offer_next_page(query, &mut row, next_page);
    if let (Some(path), Some(read)) = (lsp_search_query::path(query), outline_read(&page)) {
        row["next"]["read"] = crate::response::result::Continuation::input(
            ToolId::LocalFetch,
            json!({ "queries": [{ "path": path, "ranges": [read] }] }),
        )
        .build();
    }
    row
}

/// The line range of an outline page's first top-level multi-line
/// declaration (its name line to its last line), else its first symbol's.
pub(super) fn outline_read(page: &[Value]) -> Option<String> {
    let span = |symbol: &Value| {
        let line = symbol.get("line")?.as_u64()?;
        let end = symbol
            .get("endLine")
            .and_then(Value::as_u64)
            .unwrap_or(line);
        Some((line, end.max(line)))
    };
    let (line, end) = page
        .iter()
        .filter(|symbol| symbol.get("parent").is_none())
        .filter_map(span)
        .find(|(line, end)| end > line)
        .or_else(|| page.iter().find_map(span))?;
    Some(format!("{line}-{end}"))
}
