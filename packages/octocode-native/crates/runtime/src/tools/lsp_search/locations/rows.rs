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
/// row per listed file, in listed order, grouped into admitted batches,
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
                        .map(|(start, end)| json!({"startLine": start,"endLine":end}))
                        .collect::<Vec<_>>(),
                })
            })
        })
        .collect::<Vec<_>>();
    if reads.is_empty() {
        return;
    }
    for reads in reads.chunks(crate::response::result::rows_per_call(ToolId::LocalFetch)) {
        crate::response::result::attach_related(
            row,
            crate::response::result::Continuation::related_input(
                ToolId::LocalFetch,
                json!({"queries": reads}),
            )
            .build(),
        );
    }
}
