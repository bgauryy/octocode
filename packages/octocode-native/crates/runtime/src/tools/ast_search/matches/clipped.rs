//! Reads of the lines and values a page clipped (`next.expandValues`,
//! `next.read`).
use super::types::{Group, Scope};
use crate::id::ToolId;
use serde_json::{Value, json};

/// One `expandValues` lead whose rows read each file's clipped lines, within
/// localFetch's batch limit; a warning names every range left out.
pub(super) fn read_clipped_lines(scope: &Scope, out: &mut Value, page_groups: &[Group]) {
    let limit = crate::response::result::rows_per_call(ToolId::LocalFetch);
    let (mut rows, mut left) = (Vec::new(), Vec::new());
    for group in page_groups {
        let Some(reads) = read_clipped_values(scope, group) else {
            continue;
        };
        let shown = group.row["path"].as_str().unwrap_or_default();
        for row in reads.rows {
            if rows.len() < limit {
                rows.push(row);
            } else {
                left.push(format!("{shown} {}", ranges_text(&row["ranges"])));
            }
        }
        if !reads.long.is_empty() {
            let long = reads
                .long
                .iter()
                .map(|(start, end)| format!("{start}-{end}"))
                .collect::<Vec<_>>();
            left.push(format!("{shown} {}", long.join(", ")));
        }
    }
    if !left.is_empty() {
        crate::response::paging::push_warnings(
            out,
            [format!(
                "Clipped values past this page's read batch (read them with localFetch): {}.",
                left.join("; ")
            )],
        );
    }
    if !rows.is_empty() {
        out["next"]["expandValues"] = crate::response::result::Continuation::input(
            ToolId::LocalFetch,
            json!({ "queries": rows }),
        )
        .build();
    }
}

/// Lines of context around each hit in the `read` lead.
pub(super) const READ_CONTEXT: u64 = 3;
/// Hit windows one `read` lead may name.
pub(super) const READ_MAX_RANGES: usize = 5;
/// Longest match the top read shows whole.
pub(super) const READ_MAX_SPAN: u64 = 400;

/// The file a match row names.
pub(super) fn row_file(scope: &Scope, row: &Value) -> Option<std::path::PathBuf> {
    let shown = row["path"].as_str()?;
    Some(if scope.is_file {
        scope.canonical.clone()
    } else {
        scope
            .canonical
            .parent()
            .unwrap_or(&scope.canonical)
            .join(shown)
    })
}

/// One file's clipped rows: localFetch rows reading their merged line
/// spans, at most
/// [`RANGES_MAX_ITEMS`](crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS)
/// per row, and the spans longer than [`READ_MAX_SPAN`] lines (a whole
/// file's match) left out: whole, they would put the file on the page.
pub(super) struct ClippedReads {
    pub(super) rows: Vec<Value>,
    pub(super) long: Vec<(u64, u64)>,
}

pub(super) fn read_clipped_values(scope: &Scope, group: &Group) -> Option<ClippedReads> {
    let file = row_file(scope, &group.row).filter(|_| !group.clipped.is_empty())?;
    let (short, long): (Vec<_>, Vec<_>) = group
        .clipped
        .iter()
        .copied()
        .partition(|(start, end)| end.saturating_sub(*start) < READ_MAX_SPAN);
    let rows = crate::tools::line_spans::merge_spans(short)
        .chunks(crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS)
        .map(|chunk| {
            let ranges = chunk
                .iter()
                .map(|(start, end)| format!("{start}-{end}"))
                .collect::<Vec<_>>();
            json!({"path": file.to_string_lossy(), "ranges": ranges})
        })
        .collect();
    Some(ClippedReads {
        rows,
        long: crate::tools::line_spans::merge_spans(long),
    })
}

/// A read row's `ranges` as `a-b, c-d`.
pub(super) fn ranges_text(ranges: &Value) -> String {
    ranges
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The natural next step after a match: read the top file's hit lines in
/// context. Offered when they fit [`READ_MAX_RANGES`] windows.
pub(super) fn read_top_hits(scope: &Scope, row: &Value) -> Option<Value> {
    let file = row_file(scope, row)?;
    // A row shows a multi-line match only up to its header, so its window
    // spans the whole match; past READ_MAX_SPAN lines, its first line.
    let mut hits = vec![];
    for hit in row["matches"].as_array()? {
        let line = hit["line"].as_u64()?;
        let end = hit["endLine"]
            .as_u64()
            .filter(|end| *end >= line && end - line < READ_MAX_SPAN)
            .unwrap_or(line);
        hits.push((line, end));
    }
    let windows = crate::tools::line_spans::merge_spans(
        hits.into_iter()
            .map(|(line, end)| (line.saturating_sub(READ_CONTEXT).max(1), end + READ_CONTEXT)),
    );
    if windows.is_empty() || windows.len() > READ_MAX_RANGES {
        return None;
    }
    let ranges = windows
        .iter()
        .map(|(start, end)| format!("{start}-{end}"))
        .collect::<Vec<_>>();
    Some(
        crate::response::result::Continuation::new(
            ToolId::LocalFetch,
            json!({"path": file.to_string_lossy(), "ranges": ranges}),
        )
        .build(),
    )
}
