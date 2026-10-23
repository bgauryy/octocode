//! Ordering and grouping location rows by file, and the read lead over the
//! shown sites.
use super::*;

pub(super) fn location_sort_key(location: &Value) -> (String, u64, u64, u64, u64) {
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
pub(in super::super) fn compact_rows_by_file(page: &[Value]) -> Vec<Value> {
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

/// Per-file reference summaries `{path, matchCount, lines}` in path order:
/// `matchCount` counts every location, `lines` names each one-based start
/// line once (two references on one line share it). Paths stay absolute, like every location,
/// so the response envelope relativizes them against the same `base`;
/// pre-relativizing here (to the workspace root) made `base + path` point at
/// files that do not exist whenever `base` was the anchor's directory.
pub(in super::super) fn group_by_file(locations: &[Value]) -> Vec<Value> {
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
            let match_count = lines.len();
            lines.sort_unstable();
            lines.dedup();
            json!({ "path": path, "matchCount": match_count, "lines": lines })
        })
        .collect()
}

/// Files one read lead covers: the per-call row limit.
pub(super) const READ_LEAD_FILES: usize = 5;

/// Lines of context on each side of a site in the read lead.
pub(super) const READ_LEAD_CONTEXT: u64 = 6;

/// The one-based lines a compact row points at: a call row's `sites`, else
/// its `line`.
pub(in super::super) fn site_lines(row: &Value) -> Vec<u64> {
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
pub(in super::super) fn attach_read_lead(row: &mut Value) {
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
pub(super) fn without_project(query: &LspSearchQuery) -> bool {
    lsp_search_query::path(query)
        .map(super::super::render::uri_to_path)
        .and_then(|path| {
            std::path::Path::new(&path)
                .parent()
                .map(std::path::Path::to_path_buf)
        })
        .is_some_and(|dir| octocode_engine::lsp::workspace::project_root_of_dir(&dir).is_none())
}
