//! Read leads out of a search page: the declaration or hits a localFetch
//! call reads next.

use super::enclosing::*;
use super::local_search_query;
use super::types::*;
use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
use crate::response::result::Continuation;
use octocode_contract_types::{
    LocalFetchQueryContentMatchString, LocalFetchQueryContentMatchStringContextLines,
    LocalFetchQueryContentMatchStringView, LocalFetchQueryContentRanges,
    LocalFetchQueryContentRangesView, LocalSearchQuery, SourceLineRange,
};
use octocode_engine::types::SearchContext;
use serde_json::Value;
use std::num::NonZeroU64;

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
) -> Option<Value> {
    ranges_read(joined_path(query, root, file), &[(start, end)])
}

/// The inclusive line `windows` as localFetch `ranges`.
fn line_ranges(windows: &[(u32, u32)]) -> Option<Vec<SourceLineRange>> {
    windows
        .iter()
        .map(|&(start, end)| {
            Some(SourceLineRange {
                start_line: NonZeroU64::new(u64::from(start))?,
                end_line: NonZeroU64::new(u64::from(end))?,
            })
        })
        .collect()
}

/// localFetch read of `path` over the inclusive line `windows`.
fn ranges_read(path: String, windows: &[(u32, u32)]) -> Option<Value> {
    let ranges = line_ranges(windows)?;
    let query = LocalFetchQueryContentRanges {
        block: None,
        classification: None,
        limit: None,
        main_goal: None,
        path: path.try_into().ok()?,
        ranges,
        read_progress: None,
        reasoning: None,
        selection: None,
        snapshot: None,
        view: LocalFetchQueryContentRangesView::Content,
    };
    Some(Continuation::related_query(octocode_contract_types::LocalFetchQuery::from(query)).build())
}

/// A shown file's path joined to the caller's own `path`, so a read
/// resolves wherever the search did.
pub(super) fn joined_path(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    file: &SearchFile,
) -> String {
    if root.is_file() {
        local_search_query::path(query).to_string()
    } else {
        // A leading `.` (a search of `.`) is no part of the path: the read
        // names the file as its row does, not `./src/a.rs`.
        std::path::Path::new(local_search_query::path(query))
            .join(&file.path)
            .components()
            .filter(|part| *part != std::path::Component::CurDir)
            .collect::<std::path::PathBuf>()
            .to_string_lossy()
            .into_owned()
    }
}

/// Where a shown file is on disk.
pub(super) fn source_of(root: &std::path::Path, file: &SearchFile) -> std::path::PathBuf {
    if root.is_file() {
        root.to_path_buf()
    } else {
        root.join(&file.path)
    }
}

/// Lines a handed-off read shows on each side of a hit.
pub(super) const READ_HANDOFF_CONTEXT: u32 = 6;

/// `(start, end)` windows of `context` lines around each line, merged where
/// they overlap or touch, in line order, ending at the file's `last` line
/// when the search counted it. The file is never read again for its length:
/// a window of a file whose lines are unknown is clipped by the read it feeds.
pub(super) fn hit_windows(
    lines: impl IntoIterator<Item = u32>,
    context: SearchContext,
    last: Option<u32>,
) -> Vec<(u32, u32)> {
    crate::tools::line_spans::merge_spans(lines.into_iter().map(|line| {
        let end = line.saturating_add(context.after);
        (
            line.saturating_sub(context.before).max(1),
            last.map_or(end, |last| end.min(last).max(line)),
        )
    }))
}

/// localFetch read of one file's hits in context (the file declaring the
/// searched symbol, else the top file), however many: ±6-line `ranges`
/// around exactly the shown hits; or, when a hit is a window of a line too
/// long for a row, or the hits need more windows than one read holds, the
/// search's own `matchString` read, which localFetch pages and which opens
/// a long line at its hits instead of at its first byte (within the hits'
/// `ranges` when they fit). A window row's context is its own line: a read of
/// only window rows takes no context lines. A multiline search reads its
/// hits' lines, which localFetch's line matching would not find, while one
/// read holds their windows. Paths join the caller's own `path`, so the read
/// resolves wherever the search did.
pub(super) fn read_handoff(
    query: &LocalSearchQuery,
    root: &std::path::Path,
    top: &SearchFile,
) -> Option<Value> {
    let hits = top.matches.as_ref()?;
    if hits.is_empty() {
        return None;
    }
    let path = joined_path(query, root, top);
    let windowed = hits.iter().filter(|row| row.window_start.is_some()).count();
    let context = if windowed == hits.len() {
        0
    } else {
        READ_HANDOFF_CONTEXT
    };
    let windows = hit_windows(
        hits.iter().flat_map(row_lines),
        SearchContext::symmetric(context),
        top.lines,
    );
    let fits = windows.len() <= RANGES_MAX_ITEMS;
    // localFetch matches line by line: a multiline search's hits are not
    // its matches, so only its lines are read.
    let line_matches = local_search_query::multiline(query) == local_search_query::Multiline::Off;
    if (windowed == 0 || !line_matches) && fits {
        return ranges_read(path, &windows);
    }
    if !line_matches {
        return None;
    }
    let ranges = if fits {
        line_ranges(&windows)?
    } else {
        Vec::new()
    };
    let context = i64::from(context);
    let query = LocalFetchQueryContentMatchString {
        block: None,
        case_mode: local_search_query::case_mode(query),
        classification: None,
        context_lines: (context > 0).then_some(LocalFetchQueryContentMatchStringContextLines {
            after: context,
            before: context,
        }),
        limit: None,
        main_goal: None,
        match_string: local_search_query::patterns(query).clone(),
        path: path.try_into().ok()?,
        ranges,
        read_progress: None,
        reasoning: None,
        regex_engine: local_search_query::regex_mode(query),
        selection: None,
        snapshot: None,
        view: LocalFetchQueryContentMatchStringView::Content,
        whole_word: local_search_query::whole_word(query).unwrap_or(false),
    };
    Some(Continuation::related_query(octocode_contract_types::LocalFetchQuery::from(query)).build())
}
