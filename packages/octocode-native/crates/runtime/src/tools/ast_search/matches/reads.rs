//! Optional source reads of matched structural locations.
use super::types::Scope;
use crate::id::ToolId;
use serde_json::{Value, json};

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

/// The natural next step after a match: read the top file's hit lines in
/// context. Offered when they fit [`READ_MAX_RANGES`] windows.
pub(super) fn read_top_hits(scope: &Scope, row: &Value) -> Option<Value> {
    let file = row_file(scope, row)?;
    // A structural header can omit the body: read the whole match in
    // context; past READ_MAX_SPAN lines, anchor the read at its first line.
    let mut hits = vec![];
    for hit in row["matches"].as_array()? {
        let line = hit["line"].as_u64()?;
        let end = hit["endLine"]
            .as_u64()
            .filter(|end| *end >= line && end - line < READ_MAX_SPAN)
            .unwrap_or(line);
        hits.push((line, end));
    }
    // No window ends past the file's last line.
    let last = crate::tools::line_spans::file_lines(&file).map(u64::from);
    let windows = crate::tools::line_spans::merge_spans(hits.into_iter().map(|(line, end)| {
        let stop = end + READ_CONTEXT;
        (
            line.saturating_sub(READ_CONTEXT).max(1),
            last.map_or(stop, |last| stop.min(last.max(end))),
        )
    }));
    if windows.is_empty() || windows.len() > READ_MAX_RANGES {
        return None;
    }
    let ranges = windows
        .iter()
        .map(|(start, end)| json!({"startLine":start,"endLine":end}))
        .collect::<Vec<_>>();
    Some(
        crate::response::result::Continuation::related(
            ToolId::LocalFetch,
            json!({"path": file.to_string_lossy(), "ranges": ranges}),
        )
        .build(),
    )
}
