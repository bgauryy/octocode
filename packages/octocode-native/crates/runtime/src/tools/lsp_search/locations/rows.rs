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
/// Adjacent location rows of one file under its `path`, in order, as a call
/// row's `files` are: `None` when a row names no path, so nothing is grouped
/// under a guess. A page is sorted by path, so each file appears once.
pub(in super::super) fn located_files(rows: Vec<Value>) -> Option<Vec<Value>> {
    let mut files: Vec<(String, Vec<Value>)> = Vec::new();
    for mut row in rows {
        let path = row.as_object_mut()?.remove("path")?;
        let path = path.as_str()?.to_owned();
        match files.last_mut() {
            Some((last, matches)) if *last == path => matches.push(row),
            _ => files.push((path, vec![row])),
        }
    }
    Some(
        files
            .into_iter()
            .map(|(path, matches)| json!({ "path": path, "matches": matches }))
            .collect(),
    )
}
