//! Optional source reads of matched structural locations.
use super::types::Scope;
use serde_json::Value;

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
    // A window past the file's last line is clipped by the read itself.
    let windows = octocode_tools_shared::line_spans::merge_spans(
        hits.into_iter()
            .map(|(line, end)| (line.saturating_sub(READ_CONTEXT).max(1), end + READ_CONTEXT)),
    );
    if windows.is_empty() || windows.len() > READ_MAX_RANGES {
        return None;
    }
    let read = octocode_tools_shared::source_view::local_fetch_query::ranges_lead(
        &file.to_string_lossy(),
        &windows,
    )?;
    Some(octocode_runtime_base::response::result::Continuation::related_query(read).build())
}
