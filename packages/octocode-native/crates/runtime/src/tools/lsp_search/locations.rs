//! Public result shaping for `lspSearch`: locations, symbol lists, and item
//! pages, with content-addressed snapshots and pagination.
//!
//! Coordinate convention (one choke point, every emitted coordinate): lines
//! are **one-based** and characters are **one-based UTF-16 code-unit
//! columns** (LSP `character + 1`). `displayRange` is
//! `{startLine, startCharacter, endLine}` and starts at the symbol itself;
//! an emitted `startLine` feeds back unchanged as the one-based `line`
//! input.

use super::LspSearchQuery;
use super::failure::{empty, offer_next_page};
use super::render::{
    PageCut, document_symbol_rows, flatten_document_symbol, into_items, page_window, paginate,
    plain_sizes, read_row_cursor, symbol_kind_name, uri_to_path,
};
use super::source::SourceCache;
use crate::id::ToolId;
use crate::response::result::Confidence;
use crate::tools::lsp_search::lsp_search_query;
use serde_json::{Value, json};

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
    if let Some(display) = symbol.pointer("/location/range").and_then(public_range) {
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
    response_window: Option<usize>,
) -> Value {
    // Defense in depth: the engine already refused unauthorized snippet
    // reads; any location still naming an unauthorized path is dropped.
    let mut locations = snippets
        .into_iter()
        .map(|snippet| serde_json::to_value(snippet).unwrap_or(Value::Null))
        .filter(|location| {
            location
                .get("uri")
                .and_then(Value::as_str)
                .is_some_and(|uri| sources.uri_authorized(uri))
        })
        .collect::<Vec<_>>();
    // Internal shape (exact provider ranges, without engine-only fields) drives
    // ordering, snapshots and grouping; `public_location` shapes emitted rows.
    locations = locations.into_iter().map(compact_location).collect();
    locations.sort_by_key(location_sort_key);
    locations.dedup_by(|a, b| location_sort_key(a) == location_sort_key(b));
    if locations.is_empty() {
        if without_project(query) {
            return empty(
                query,
                "noProject",
                "No project file (Cargo.toml, package.json, tsconfig.json, go.mod, pyproject.toml, …) is above this file, so the language server has no project to index; empty is not absence.",
                true,
            );
        }
        return empty(
            query,
            "noLocations",
            &format!("{provider} returned no locations"),
            true,
        );
    }
    let snapshot = salted_snapshot(query, kind, &locations, salt);
    if snapshot_mismatch(query, &snapshot) {
        return snapshot_changed(query);
    }
    // groupByFile summarizes per file INSTEAD of returning every location, so
    // the page unit becomes a file summary.
    let grouped = lsp_search_query::group_by_file(query) == Some(true);
    // A reference list without an explicit row form pages locations as usual
    // but prints each page as compact `line:col text` rows per file.
    let compact = kind == "references"
        && lsp_search_query::group_by_file(query).is_none()
        && lsp_search_query::context_lines(query).is_none();
    let entries = if grouped {
        group_by_file(&locations)
    } else {
        locations.clone()
    };
    let sizes = location_sizes(
        &entries,
        grouped,
        compact,
        lsp_search_query::context_lines(query),
    );
    let mut budget = budget;
    // Only context-free compact references are already held in their public
    // form. Detailed extras and source-context reads keep the conservative cut.
    if compact
        && lsp_search_query::page_size(query).is_none()
        && lsp_search_query::to_row(query)
            .get("extras")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
    {
        let full_budget = sizes
            .iter()
            .fold(0usize, |sum, size| sum.saturating_add(*size));
        // Avoid materializing an oversized candidate just to measure it.
        if full_budget > budget && response_window.is_some_and(|window| full_budget <= window) {
            let (_, pagination, _) = page_window(
                1,
                &PageCut {
                    page_size: None,
                    budget: full_budget,
                    sizes: &sizes,
                    snapshot: &snapshot,
                    resume: None,
                },
            );
            let full = location_payload(
                kind,
                provider,
                "files",
                json!(compact_rows_by_file(&entries)),
                &locations,
                pagination,
            );
            if crate::response::page_windows::terminal_row_fits(response_window, &full) {
                budget = budget.max(full_budget);
            }
        }
    }
    let (page, pagination, next_page) = paginate(
        &entries,
        lsp_search_query::page(query).unwrap_or(1),
        &PageCut {
            page_size: lsp_search_query::page_size(query),
            budget,
            sizes: &sizes,
            snapshot: &snapshot,
            resume: resume_at(query),
        },
    );
    let mut declaration_reads: Vec<Value> = Vec::new();
    let (key, rows) = if grouped {
        ("files", json!(page))
    } else if compact {
        ("files", json!(compact_rows_by_file(&page)))
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

fn location_payload(
    kind: &str,
    provider: &str,
    key: &str,
    rows: Value,
    locations: &[Value],
    pagination: Value,
) -> Value {
    let mut row = json!({"kind":kind});
    row[key] = rows;
    if kind == "references" {
        reference_totals(&mut row, locations);
    }
    row["lsp"] = json!({"serverAvailable":true,"provider":provider});
    row["pagination"] = pagination;
    row
}

/// Characters each location costs a page in the form it prints: a compact
/// `line:col text` row (its file's path once per run of rows), a file
/// summary, or the full row, plus the lines `contextLines` adds around it.
fn location_sizes(
    entries: &[Value],
    grouped: bool,
    compact: bool,
    context_lines: Option<u32>,
) -> Vec<usize> {
    use crate::response::stream_page::json_text_chars;
    let mut previous_uri: Option<&str> = None;
    entries
        .iter()
        .map(|entry| {
            if grouped {
                return octocode_engine::json_size::utf16_units(entry).unwrap_or(0) + 1;
            }
            if !compact {
                let line = entry
                    .get("content")
                    .and_then(Value::as_str)
                    .map_or(0, json_text_chars);
                let context = context_lines.map_or(0, |lines| 2 * lines as usize * line);
                return octocode_engine::json_size::utf16_units(&public_location(entry.clone()))
                    .unwrap_or(0)
                    + context
                    + 1;
            }
            let uri = entry.get("uri").and_then(Value::as_str);
            let row = compact_rows_by_file(std::slice::from_ref(entry));
            let matched = row.first().and_then(|file| file["matches"].get(0));
            let own = matched.map_or(0, |matched| {
                octocode_engine::json_size::utf16_units(matched).unwrap_or(0) + 1
            });
            let path = if uri == previous_uri {
                0
            } else {
                row.first()
                    .and_then(|file| file["path"].as_str())
                    .map_or(0, |path| {
                        octocode_engine::json_size::utf16_units(
                            &json!({"path": path, "matches": []}),
                        )
                        .unwrap_or(0)
                            + 1
                    })
            };
            previous_uri = uri;
            own + path
        })
        .collect()
}

fn reference_totals(row: &mut Value, locations: &[Value]) {
    let total_files = locations
        .iter()
        .filter_map(|location| location.get("uri").and_then(Value::as_str))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    // The reference total, whatever the page unit (files when grouped).
    row["matchCount"] = json!(locations.len());
    let recovered = locations
        .iter()
        .filter(|location| location.get("source").and_then(Value::as_str) == Some(RECOVERED_ALIAS))
        .count();
    if recovered > 0 {
        row["recoveredAliasReferences"] = json!(recovered);
    }
    row["matchedFiles"] = json!(total_files);
}

/// Widen a location's `content` to `context_lines` around its range, from
/// the request's source cache (one policy check, read, and line index per
/// file per request).
pub(super) async fn apply_context_lines(
    location: &mut Value,
    context_lines: u32,
    sources: &mut SourceCache<'_>,
) {
    let Some(path) = location.get("uri").and_then(Value::as_str).map(uri_to_path) else {
        return;
    };
    let Some(source) = sources.get(&path).await else {
        return;
    };
    let start_line = location
        .pointer("/range/start/line")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let range_end = location
        .pointer("/range/end/line")
        .and_then(Value::as_u64)
        .unwrap_or(start_line as u64) as usize;
    let end_character = location
        .pointer("/range/end/character")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let selected_end = if end_character == 0 && range_end > start_line {
        range_end - 1
    } else {
        range_end
    };
    if start_line >= source.line_count() {
        return;
    }
    let start = start_line.saturating_sub(context_lines as usize);
    let end = selected_end
        .saturating_add(context_lines as usize)
        .saturating_add(1)
        .min(source.line_count());
    location["content"] = json!(source.lines(start, end));
    location["displayRange"] = json!({
        "startLine": start + 1,
        "endLine": end,
    });
}

/// Partial reason of a row whose declaration body was clipped at
/// [`MAX_DECLARATION_CONTENT_LINES`].
const DECLARATION_CLIPPED: &str = "declarationClipped";

/// Most lines of an enclosing declaration a location carries by default.
/// `contextLines` replaces the body with an explicit window instead.
pub(super) const MAX_DECLARATION_CONTENT_LINES: usize = 60;

/// Keep the first [`MAX_DECLARATION_CONTENT_LINES`] of a location's default
/// declaration body and end it with a marker naming the omitted source lines,
/// so a class definition does not ship its whole body. Returns the localFetch
/// continuation that reads exactly the omitted lines.
pub(super) fn cap_declaration_content(location: &mut Value) -> Option<Value> {
    let content = location.get("content").and_then(Value::as_str)?;
    let lines = content.split('\n').collect::<Vec<_>>();
    if lines.len() <= MAX_DECLARATION_CONTENT_LINES {
        return None;
    }
    let omitted = lines.len() - MAX_DECLARATION_CONTENT_LINES;
    let start = location
        .pointer("/displayRange/startLine")
        .and_then(Value::as_u64)
        .map(|start| start as usize);
    let path = location.get("uri").and_then(Value::as_str).map(uri_to_path);
    let rest = start.map(|start| {
        (
            start + MAX_DECLARATION_CONTENT_LINES,
            start + lines.len() - 1,
        )
    });
    let read = match (&path, rest) {
        (Some(path), Some((from, to))) => Some(
            crate::response::result::Continuation::new(
                ToolId::LocalFetch,
                json!({"path": path, "ranges": [format!("{from}-{to}")]}),
            )
            .confidence(Confidence::Exact)
            .build(),
        ),
        _ => None,
    };
    let marker = match (rest, read.is_some()) {
        (Some((from, to)), true) => {
            format!("… {omitted} more lines omitted (source lines {from}-{to}).")
        }
        (Some((from, to)), false) => format!(
            "… {omitted} more lines omitted (source lines {from}-{to}); read them with localFetch ranges."
        ),
        (None, _) => format!("… {omitted} more lines omitted; read them with localFetch."),
    };
    let mut capped = lines[..MAX_DECLARATION_CONTENT_LINES].join("\n");
    capped.push('\n');
    capped.push_str(&marker);
    location["content"] = json!(capped);
    read
}

pub(super) fn compact_location(value: Value) -> Value {
    let mut compact = serde_json::Map::new();
    if let Some(uri) = value.get("uri").and_then(Value::as_str) {
        compact.insert("uri".into(), json!(uri));
    }
    if let Some(range) = value.get("range") {
        compact.insert("range".into(), range.clone());
    }
    if let Some(content) = value.get("content") {
        compact.insert("content".into(), content.clone());
    }
    let display_range = value
        .get("displayRange")
        .or_else(|| value.get("display_range"))
        .and_then(normalize_display_range)
        .or_else(|| {
            let start = value.pointer("/range/start/line")?.as_u64()?;
            let end = value.pointer("/range/end/line")?.as_u64()?;
            Some(json!({ "startLine": start + 1, "endLine": end + 1 }))
        });
    if let Some(display_range) = display_range {
        compact.insert("displayRange".into(), display_range);
    }
    let is_definition = value
        .get("isDefinition")
        .or_else(|| value.get("is_definition"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if is_definition {
        compact.insert("isDefinition".into(), json!(true));
    }
    if let Some(source) = value.get("source").and_then(Value::as_str) {
        compact.insert("source".into(), json!(source));
    }
    Value::Object(compact)
}

/// `source` of a reference found through an aliasing import rather than
/// reported by the language server.
pub(super) const RECOVERED_ALIAS: &str = "recoveredAlias";

/// `content` with each source line prefixed by its one-based number and a
/// tab (`<line>\t<text>`, as localFetch reads are numbered), from `start`. A
/// trailing omission marker (see [`cap_declaration_content`]) stays
/// unnumbered.
fn number_content(content: &str, start: u64) -> String {
    let mut out = String::with_capacity(content.len() + content.len() / 8);
    let mut line = start;
    for record in content.split_inclusive('\n') {
        if record.starts_with("… ") && record.contains(" more lines omitted") {
            out.push_str(record);
        } else {
            crate::response::numbered::push_numbered(&mut out, line, record);
            line += 1;
        }
    }
    out
}

/// Public location: one one-based `displayRange` (`startLine`,
/// `startCharacter`, `endLine`) for the symbol itself instead of the raw
/// zero-based LSP `range` plus a line-only copy. `content` carries its own
/// line numbers, from the widened window's first line when `contextLines`
/// widened it.
pub(super) fn public_location(internal: Value) -> Value {
    let Value::Object(mut internal) = internal else {
        return internal;
    };
    let range = internal.shift_remove("range");
    let window = internal.shift_remove("displayRange");
    let mut public = serde_json::Map::new();
    if let Some(uri) = internal
        .shift_remove("uri")
        .as_ref()
        .and_then(Value::as_str)
    {
        public.insert("path".into(), json!(uri_to_path(uri)));
    }
    let display = range
        .as_ref()
        .and_then(public_range)
        .or_else(|| window.clone());
    let symbol_start = display
        .as_ref()
        .and_then(|display| display.get("startLine"))
        .and_then(Value::as_u64);
    if let Some(display) = display {
        public.insert("displayRange".into(), display);
    }
    if let Some(content) = internal.shift_remove("content") {
        let start = window
            .as_ref()
            .and_then(|window| window.get("startLine"))
            .and_then(Value::as_u64)
            .or(symbol_start);
        match (content.as_str(), start) {
            // An unreadable file's location carries the reason, not source.
            (Some(text), Some(start))
                if !text.is_empty() && !text.starts_with("[content unavailable") =>
            {
                public.insert("content".into(), json!(number_content(text, start)));
            }
            _ => {
                public.insert("content".into(), content);
            }
        }
    }
    public.append(&mut internal);
    Value::Object(public)
}

fn normalize_display_range(value: &Value) -> Option<Value> {
    let start = value
        .get("startLine")
        .or_else(|| value.get("start_line"))
        .and_then(Value::as_u64)?;
    let end = value
        .get("endLine")
        .or_else(|| value.get("end_line"))
        .and_then(Value::as_u64)?;
    Some(json!({ "startLine": start, "endLine": end }))
}

fn location_sort_key(location: &Value) -> (String, u64, u64, u64, u64) {
    let point = |pointer: &str| {
        location
            .pointer(pointer)
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    (
        location
            .get("uri")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        point("/range/start/line"),
        point("/range/start/character"),
        point("/range/end/line"),
        point("/range/end/character"),
    )
}

/// One page of locations as `{path, matches}` per file, in page order. A
/// row is `{line, column, endLine?, value?}`: the one-based start
/// (UTF-16 column, as in `displayRange`), the last line when the range
/// spans lines, and the first trimmed line of the location's content. A
/// declaration row carries `declaration: true`; a row recovered outside the
/// server's own answer carries its label as `source`. Paths stay absolute
/// so the envelope relativizes them like every other path.
pub(super) fn compact_rows_by_file(page: &[Value]) -> Vec<Value> {
    let mut files: Vec<(String, serde_json::Map<String, Value>)> = Vec::new();
    for location in page {
        let path = location
            .get("uri")
            .and_then(Value::as_str)
            .map(uri_to_path)
            .unwrap_or_else(|| "unknown".to_owned());
        let point = |pointer: &str| location.pointer(pointer).and_then(Value::as_u64);
        let start = point("/range/start/line").unwrap_or(0) + 1;
        let end = point("/range/end/line").map_or(start, |end| end + 1);
        let column = point("/range/start/character").unwrap_or(0) + 1;
        let text = location
            .get("content")
            .and_then(Value::as_str)
            .and_then(|content| content.lines().next())
            .map(str::trim)
            .unwrap_or_default();
        let mut row = serde_json::Map::new();
        row.insert("line".into(), json!(start));
        row.insert("column".into(), json!(column));
        if end > start {
            row.insert("endLine".into(), json!(end));
        }
        if !text.is_empty() {
            row.insert("value".into(), json!(text));
        }
        if location.get("isDefinition").and_then(Value::as_bool) == Some(true) {
            row.insert("declaration".into(), json!(true));
        }
        if let Some(label) = location.get("source").and_then(Value::as_str) {
            row.insert("source".into(), json!(label));
        }
        let index = match files.iter().position(|(seen, _)| *seen == path) {
            Some(index) => index,
            None => {
                let mut entry = serde_json::Map::new();
                entry.insert("path".into(), json!(path));
                entry.insert("matches".into(), json!([]));
                files.push((path, entry));
                files.len() - 1
            }
        };
        if let Some(refs) = files[index]
            .1
            .get_mut("matches")
            .and_then(Value::as_array_mut)
        {
            refs.push(Value::Object(row));
        }
    }
    files
        .into_iter()
        .map(|(_, entry)| Value::Object(entry))
        .collect()
}

/// Per-file reference summaries `{path, matchCount, lines}` in path order,
/// with one-based start `lines`. Paths stay absolute, like every location,
/// so the response envelope relativizes them against the same `base`;
/// pre-relativizing here (to the workspace root) made `base + path` point at
/// files that do not exist whenever `base` was the anchor's directory.
pub(super) fn group_by_file(locations: &[Value]) -> Vec<Value> {
    let mut files: std::collections::BTreeMap<String, Vec<u64>> = std::collections::BTreeMap::new();
    for location in locations {
        let path = location
            .get("uri")
            .and_then(Value::as_str)
            .map(uri_to_path)
            .unwrap_or_else(|| "unknown".to_owned());
        let line = location
            .pointer("/range/start/line")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            + 1;
        files.entry(path).or_default().push(line);
    }
    files
        .into_iter()
        .map(|(path, mut lines)| {
            lines.sort_unstable();
            json!({ "path": path, "matchCount": lines.len(), "lines": lines })
        })
        .collect()
}

pub(super) fn semantic_snapshot(query: &LspSearchQuery, kind: &str, items: &[Value]) -> String {
    salted_snapshot(query, kind, items, None)
}

/// [`semantic_snapshot`] that also digests `salt` when there is one.
fn salted_snapshot(
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
    let canonical = octocode_engine::digest::canonicalize_dropping_nulls(identity);
    // v3: object rows, 1-based columns, matchCount totals, pageSize on
    // every page.
    format!("lsp-v3:{}", crate::digest::json_sha256(&canonical))
}

/// A later page whose `snapshot` no longer matches, or names a row its check
/// does not bind to this page; a page without one pins nothing and pages the
/// current answer.
pub(super) fn snapshot_mismatch(query: &LspSearchQuery, actual: &str) -> bool {
    let page = lsp_search_query::page(query).unwrap_or(1) as usize;
    page > 1
        && lsp_search_query::snapshot(query).is_some_and(|pinned| {
            read_row_cursor(pinned, page).is_none_or(|(digest, _)| digest != actual)
        })
}

/// The row a later page's copied cursor starts at, when it names one.
fn resume_at(query: &LspSearchQuery) -> Option<usize> {
    let page = lsp_search_query::page(query).unwrap_or(1) as usize;
    lsp_search_query::snapshot(query)
        .filter(|_| page > 1)
        .and_then(|pinned| read_row_cursor(pinned, page)?.1)
}

/// The stale-snapshot row; its restart walks from the first location page
/// of the first importer window.
pub(super) fn snapshot_changed(query: &LspSearchQuery) -> Value {
    crate::response::result::stale_snapshot(crate::response::result::restart_query(
        ToolId::LspSearch,
        &lsp_search_query::to_row(query),
    ))
}

pub(super) fn items_payload(
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
pub(super) fn salted_items_payload(
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
        &PageCut {
            page_size: lsp_search_query::page_size(query),
            budget,
            sizes: &sizes,
            snapshot: &snapshot,
            resume: resume_at(query),
        },
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

fn document_symbols_payload(query: &LspSearchQuery, raw_items: &[Value], budget: usize) -> Value {
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
        &PageCut {
            page_size: lsp_search_query::page_size(query),
            budget,
            sizes: &sizes,
            snapshot: &snapshot,
            resume: resume_at(query),
        },
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
        .confidence(Confidence::Medium)
        .build();
    }
    row
}

/// The line range of an outline page's first top-level multi-line
/// declaration (its name line to its last line), else its first symbol's.
fn outline_read(page: &[Value]) -> Option<String> {
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

/// Files one read lead covers: the per-call row limit.
const READ_LEAD_FILES: usize = 5;
/// Lines of context on each side of a site in the read lead.
const READ_LEAD_CONTEXT: u64 = 6;

/// The one-based lines a compact row points at: a call row's `sites`, else
/// its `line`.
pub(super) fn site_lines(row: &Value) -> Vec<u64> {
    match row.get("sites").and_then(Value::as_array) {
        Some(sites) => sites
            .iter()
            .filter_map(|site| site.get("line").and_then(Value::as_u64))
            .collect(),
        None => row
            .get("line")
            .and_then(Value::as_u64)
            .into_iter()
            .collect(),
    }
}

/// On a page of sites (`references`, `callers`), `next.read`: one localFetch
/// row per listed file, in listed order and at most [`READ_LEAD_FILES`],
/// reading each file's sites with [`READ_LEAD_CONTEXT`] lines around them
/// (overlapping windows merged).
pub(super) fn attach_read_lead(row: &mut Value) {
    if row.get("status").and_then(Value::as_str).is_some() {
        return;
    }
    let Some(files) = row.pointer("/files").and_then(Value::as_array) else {
        return;
    };
    let max_ranges = crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
    let reads = files
        .iter()
        .filter_map(|file| {
            let path = file.get("path")?.as_str()?;
            let windows = crate::tools::line_spans::merge_spans(
                file.get("matches")?
                    .as_array()?
                    .iter()
                    .flat_map(site_lines)
                    .map(|line| {
                        (
                            line.saturating_sub(READ_LEAD_CONTEXT).max(1),
                            line + READ_LEAD_CONTEXT,
                        )
                    }),
            );
            (!windows.is_empty() && windows.len() <= max_ranges).then(|| {
                json!({
                    "path": path,
                    "ranges": windows
                        .iter()
                        .map(|(start, end)| format!("{start}-{end}"))
                        .collect::<Vec<_>>(),
                })
            })
        })
        .take(READ_LEAD_FILES)
        .collect::<Vec<_>>();
    if reads.is_empty() {
        return;
    }
    row["next"]["read"] = crate::response::result::Continuation::input(
        ToolId::LocalFetch,
        json!({ "queries": reads }),
    )
    .confidence(Confidence::High)
    .build();
}

/// Whether no ancestor of the anchor file holds a project marker: the server
/// then indexes a bare directory, and an empty answer says nothing.
fn without_project(query: &LspSearchQuery) -> bool {
    lsp_search_query::path(query)
        .map(super::render::uri_to_path)
        .and_then(|path| {
            std::path::Path::new(&path)
                .parent()
                .map(std::path::Path::to_path_buf)
        })
        .is_some_and(|dir| octocode_engine::lsp::workspace::project_root_of_dir(&dir).is_none())
}
