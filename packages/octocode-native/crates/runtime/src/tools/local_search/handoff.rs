//! Read leads out of a search page: the declaration or hits a localFetch
//! call reads next.

use super::enclosing::*;
use super::local_search_query;
use super::types::*;
use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
use crate::id::ToolId;
use crate::response::result::Continuation;
use serde_json::{Value, json};

/// The searched symbol's declaration among the shown hits.
pub(super) struct Definition {
    /// Absolute path of the declaring file.
    pub(super) source: String,
    /// The declaration's name line.
    pub(super) line: u32,
    pub(super) kind: String,
    /// Its first and last line.
    pub(super) start: u32,
    pub(super) end: u32,
}

/// localFetch read of a shown declaration whole, from its first line to its
/// last; localFetch pages a long one.
pub(super) fn declaration_read(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    file: &SearchFile,
    start: u32,
    end: u32,
) -> Value {
    Continuation::new(
        ToolId::LocalFetch,
        json!({"path": joined_path(query, root, file), "ranges": [format!("{start}-{end}")]}),
    )
    .build()
}

/// A hit whose value was clipped out of a line longer than one read page:
/// a line-range read of it shows the line's start, not the hit.
pub(super) fn is_long_line_hit(hit: &SearchMatch) -> bool {
    hit.clipped && hit.original_bytes.is_some_and(passes_line_page)
}

/// Whether a value of `bytes` UTF-8 bytes passes one localFetch line page
/// ([`LINE_PAGE_BYTES`], also counted in bytes).
///
/// [`LINE_PAGE_BYTES`]: crate::tools::source_view::LINE_PAGE_BYTES
pub(super) fn passes_line_page(bytes: usize) -> bool {
    bytes > crate::tools::source_view::LINE_PAGE_BYTES
}

/// A byte window of the hit's line, starting at the hit.
pub(super) fn long_line_read(path: &str, hit: &SearchMatch) -> Value {
    Continuation::new(
        ToolId::LocalFetch,
        json!({
            "path": path,
            "ranges": [format!("{0}-{0}", hit.line)],
            "unit": "bytes",
            "offset": hit.byte_offset,
        }),
    )
    .build()
}

/// A shown file's path joined to the caller's own `path`, so a read
/// resolves wherever the search did.
pub(super) fn joined_path(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    file: &SearchFile,
) -> String {
    if root.is_file() {
        query.path.to_string()
    } else {
        std::path::Path::new(query.path.as_str())
            .join(&file.path)
            .to_string_lossy()
            .into_owned()
    }
}

/// Hits a handed-off read may cover (each opens a ±6-line window).
pub(super) const READ_HANDOFF_MAX_HITS: usize = 20;

/// A range end past any file: localFetch clamps it to the last line, so the
/// read runs from its start to the end of the file, paged.
pub(super) const READ_TO_END_LINE: usize = 1_000_000;

/// Lines a handed-off read shows on each side of a hit.
pub(super) const READ_HANDOFF_CONTEXT: u32 = 6;

/// `(start, end)` windows of `context` lines around each line, merged where
/// they overlap or touch, in line order.
pub(super) fn hit_windows(
    lines: impl IntoIterator<Item = u32>,
    context: SearchContext,
) -> Vec<(u32, u32)> {
    crate::tools::line_spans::merge_spans(lines.into_iter().map(|line| {
        (
            line.saturating_sub(context.before).max(1),
            line.saturating_add(context.after),
        )
    }))
}

/// localFetch read of one file's hits in context (the file declaring the
/// searched symbol, else the top file): ±6-line `ranges`
/// around exactly the shown hits, or (when they need more windows than one
/// read holds) the search text as `matchString`. Paths join the caller's
/// own `path`, so the read resolves wherever the search did.
pub(super) fn read_handoff(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    top: &SearchFile,
) -> Option<Value> {
    let hits = top.matches.as_ref()?;
    if hits.is_empty() || hits.len() > READ_HANDOFF_MAX_HITS {
        return None;
    }
    let path = joined_path(query, root, top);
    // A line longer than a read page is read from the hit, not from its
    // start: that hit alone, when no other hit is a plain line.
    let (long, plain): (Vec<_>, Vec<_>) = hits.iter().partition(|hit| is_long_line_hit(hit));
    if plain.is_empty()
        && let Some(hit) = long.first()
    {
        return Some(long_line_read(&path, hit));
    }
    let windows = hit_windows(
        plain.into_iter().flat_map(row_lines),
        SearchContext::symmetric(READ_HANDOFF_CONTEXT),
    );
    if windows.len() <= RANGES_MAX_ITEMS {
        let ranges = windows
            .iter()
            .map(|(start, end)| format!("{start}-{end}"))
            .collect::<Vec<_>>();
        return Some(
            Continuation::new(ToolId::LocalFetch, json!({"path": path, "ranges": ranges})).build(),
        );
    }
    let mut read = json!({
        "path": path,
        "matchString": local_search_query::pattern(query),
        "contextLines": READ_HANDOFF_CONTEXT,
    });
    if local_search_query::regex_mode(query) != LocalSearchQueryRegex::Literal
        && regex::escape(local_search_query::pattern(query)) != local_search_query::pattern(query)
    {
        read["regex"] = json!("rust");
    }
    if query.case_mode != LocalSearchQueryCaseMode::Insensitive {
        read["caseMode"] = json!(query.case_mode);
    }
    Some(Continuation::new(ToolId::LocalFetch, read).build())
}
