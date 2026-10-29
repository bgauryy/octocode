//! Public result shaping for `lspSearch`: locations, symbol lists, and item
//! pages, with content-addressed snapshots and pagination.
//!
//! Coordinate convention (one choke point, every emitted coordinate): lines
//! are **one-based** and characters are **one-based UTF-16 code-unit
//! columns** (LSP `character + 1`). `displayRange` is
//! `{startLine, startCharacter, endLine}` and starts at the symbol itself;
//! an emitted `startLine` feeds back unchanged as the one-based `line`
//! input.

use super::failure::{empty, no_answer, offer_next_page};
use super::render::{
    Pages, document_symbol_rows, flatten_document_symbol, into_items, page_window, paginate,
    plain_sizes, symbol_kind_name, uri_to_path,
};
use super::source::SourceCache;
use crate::id::ToolId;
use crate::tools::lsp_search::lsp_search_query;
use octocode_contract_types::LspSearchQuery;
use serde_json::{Value, json};

mod rows;
mod shaping;
mod snapshots;
use rows::location_sort_key;
pub(super) use rows::{attach_read_lead, group_by_file};
#[cfg(test)]
pub(super) use shaping::MAX_DECLARATION_CONTENT_LINES;
use shaping::{DECLARATION_CLIPPED, location_payload, location_sizes};
pub(super) use shaping::{
    RECOVERED_ALIAS, apply_context_lines, cap_declaration_content, compact_location,
    public_location,
};
use snapshots::salted_snapshot;
#[cfg(test)]
pub(super) use snapshots::semantic_snapshot;
pub(super) use snapshots::{
    items_payload, salted_items_payload, snapshot_changed, snapshot_mismatch,
};

/// One-based public `displayRange` of a zero-based LSP range.
pub(super) fn public_range(range: &Value) -> Option<Value> {
    let line = range.pointer("/start/line")?.as_u64()?;
    let character = range.pointer("/start/character")?.as_u64()?;
    let end = range
        .pointer("/end/line")
        .and_then(Value::as_u64)
        .unwrap_or(line);
    Some(json!({ "startLine": line + 1, "startCharacter": character + 1, "endLine": end + 1 }))
}

/// Public hover: the server's zero-based `range` becomes the one-based
/// `displayRange` every other emitted coordinate uses.
pub(super) fn public_hover(mut hover: Value) -> Value {
    if let Some(object) = hover.as_object_mut()
        && let Some(range) = object.remove("range")
        && let Some(display) = public_range(&range)
    {
        object.insert("displayRange".into(), display);
    }
    hover
}

/// Public workspace symbol (`SymbolInformation`/`WorkspaceSymbol`): named
/// kind, flattened `uri`, and the one-based `displayRange` locations use.
pub(super) fn public_workspace_symbol(symbol: &Value) -> Value {
    let mut public = serde_json::Map::new();
    if let Some(name) = symbol.get("name") {
        public.insert("symbolName".into(), name.clone());
    }
    public.insert("kind".into(), json!(symbol_kind_name(symbol.get("kind"))));
    if let Some(container) = symbol
        .get("containerName")
        .and_then(Value::as_str)
        .filter(|container| !container.is_empty())
    {
        public.insert("containerName".into(), json!(container));
    }
    if let Some(uri) = symbol.pointer("/location/uri").and_then(Value::as_str) {
        public.insert("path".into(), json!(uri_to_path(uri)));
    }
    if let Some(mut display) = symbol.pointer("/location/range").and_then(public_range) {
        // A `SymbolInformation` range starts at the declaration (`export`),
        // not at the name: no column, which would mislocate the symbol.
        if let Some(display) = display.as_object_mut() {
            display.remove("startCharacter");
        }
        public.insert("displayRange".into(), display);
    }
    Value::Object(public)
}

pub(super) async fn locations(
    query: &LspSearchQuery,
    sources: &mut SourceCache<'_>,
    kind: &str,
    provider: &str,
    snippets: Vec<impl serde::Serialize>,
    budget: usize,
    response_window: Option<usize>,
) -> Value {
    salted_locations(
        query,
        sources,
        kind,
        provider,
        snippets,
        None,
        budget,
        response_window,
    )
    .await
}

/// [`locations`] whose snapshot also covers `salt` (the importer candidate
/// digest), so a later page goes stale when the salt changes.
pub(super) async fn salted_locations(
    query: &LspSearchQuery,
    sources: &mut SourceCache<'_>,
    kind: &str,
    provider: &str,
    snippets: Vec<impl serde::Serialize>,
    salt: Option<&str>,
    budget: usize,
    _response_window: Option<usize>,
) -> Value {
    // Defense in depth: the engine already refused unauthorized snippet
    // reads; any location still naming an unauthorized path is dropped.
    let mut out_of_policy = 0usize;
    let mut locations = snippets
        .into_iter()
        .map(|snippet| serde_json::to_value(snippet).unwrap_or(Value::Null))
        .filter(|location| {
            let authorized = location
                .get("uri")
                .and_then(Value::as_str)
                .is_some_and(|uri| sources.uri_authorized(uri));
            out_of_policy += usize::from(!authorized);
            authorized
        })
        .collect::<Vec<_>>();
    // Internal shape (exact provider ranges, without engine-only fields) drives
    // ordering, snapshots and grouping; `public_location` shapes emitted rows.
    locations = locations.into_iter().map(compact_location).collect();
    locations.sort_by_key(location_sort_key);
    locations.dedup_by(|a, b| location_sort_key(a) == location_sort_key(b));
    if locations.is_empty() {
        // Locations exist but policy withheld them: not an absence.
        if out_of_policy > 0 {
            return withheld(query, out_of_policy);
        }
        return no_answer(
            query,
            "noLocations",
            &format!("{provider} returned no locations"),
        );
    }
    let snapshot = salted_snapshot(query, kind, &locations, salt);
    if snapshot_mismatch(query, &snapshot) {
        return snapshot_changed(query);
    }
    // The files view summarizes per file, so
    // the page unit becomes a file summary.
    let grouped = lsp_search_query::view(query) == "files";
    let entries = if grouped {
        group_by_file(&locations)
    } else {
        locations.clone()
    };
    let sizes = location_sizes(&entries, grouped, lsp_search_query::context_lines(query));
    let (page, pagination, next_page) = paginate(
        &entries,
        lsp_search_query::page(query).unwrap_or(1),
        &Pages::of(budget, &sizes, &snapshot),
    );
    let mut declaration_reads: Vec<Value> = Vec::new();
    let (key, rows) = if grouped {
        ("files", json!(page))
    } else {
        // Context lines are read for this page only, not the whole set.
        let mut page = page;
        if let Some(context_lines) = lsp_search_query::context_lines(query) {
            for location in &mut page {
                apply_context_lines(location, context_lines, sources).await;
            }
        } else {
            declaration_reads = page
                .iter_mut()
                .filter_map(cap_declaration_content)
                .collect();
        }
        let page = page.into_iter().map(public_location).collect::<Vec<_>>();
        ("matches", json!(page))
    };
    let mut row = location_payload(kind, provider, key, rows, &locations, pagination);
    offer_next_page(query, &mut row, next_page);
    // A clipped body is a partial row with its reason; the reads reach the
    // omitted lines.
    if !declaration_reads.is_empty() {
        crate::response::paging::mark_partial(&mut row, DECLARATION_CLIPPED, &[]);
    }
    // One read per capped body: `readDeclaration`, `readDeclaration2`, ….
    for (index, read) in declaration_reads.into_iter().enumerate() {
        let key = match index {
            0 => "readDeclaration".to_owned(),
            n => format!("readDeclaration{}", n + 1),
        };
        row["next"][key] = read;
    }
    row
}

/// The empty row of an answer whose every location the read policy
/// withheld (outside the allowed read roots).
pub(super) fn withheld(query: &LspSearchQuery, withheld: usize) -> Value {
    empty(
        query,
        "locationsWithheld",
        &format!(
            "{withheld} location(s) outside the allowed read roots were omitted; add the path to ALLOWED_PATHS to read them."
        ),
        true,
    )
}
