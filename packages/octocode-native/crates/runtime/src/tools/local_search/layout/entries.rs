//! Serialized sizes of one file entry and its rows, and the per-file
//! paging an entry reports.
use crate::tools::local_search::local_search_query::View;

use super::super::rows::project_shape;
use super::super::types::ItemPagination;
use super::Layout;
use super::costs::{PageCosts, RowName};
use crate::num::u32_of_usize;
use octocode_engine::json_size::string_body_units;

/// Per-file paging, reported only while it routes somewhere: more rows
/// remain, or the requested match page is past the end. `shown` counts the
/// rows the file entry carries.
pub(crate) fn file_pagination(
    layout: &Layout,
    total: u32,
    (start, end, shown): (usize, usize, usize),
    later: &[u32],
    match_page: u32,
) -> Option<ItemPagination> {
    if layout.list {
        return None;
    }
    if layout.streamed {
        return (end < total as usize).then(|| {
            let (lines, unlisted) = crate::tools::line_spans::more_lines(later, MAX_MORE_LINES);
            let named = shown + lines.len() + unlisted.map_or(0, |n| n as usize);
            ItemPagination {
                page: None,
                total_pages: None,
                total_matches: (named != total as usize).then_some(total),
                has_more: true,
                more_lines: Some(lines),
                more_lines_unlisted: unlisted,
                out_of_range: false,
            }
        });
    }
    let total_pages = u32_of_usize(crate::response::page_windows::count_pages(
        total as usize,
        layout.matches_per as usize,
    ));
    let has_more = match_page < total_pages;
    let out_of_range = start >= total as usize && total > 0;
    let (more_lines, more_lines_unlisted) = if has_more && !later.is_empty() {
        let (lines, unlisted) = crate::tools::line_spans::more_lines(later, MAX_MORE_LINES);
        (Some(lines), unlisted)
    } else {
        (None, None)
    };
    (has_more || out_of_range).then_some(ItemPagination {
        page: Some(match_page),
        total_pages: Some(total_pages),
        total_matches: Some(total),
        has_more,
        more_lines,
        more_lines_unlisted,
        out_of_range,
    })
}

/// Serialized chars of one file entry around its rows and path.
pub(super) const FILE_ENTRY_CHARS: usize = "{\"path\":\"\",\"matches\":[]},".len();

/// Serialized chars of one match row as a page shows it, with its separator.
/// The value is priced in place, never copied.
pub(crate) fn row_chars(
    matched: &octocode_engine::types::TextSearchMatch,
    span_columns: bool,
) -> usize {
    if let Some(price) = matched.deferred.filter(|price| *price > 0) {
        return price as usize;
    }
    let (mut row, value) = project_shape(matched);
    if !span_columns {
        row.column = None;
    }
    octocode_engine::json_size::utf16_units(&row).unwrap_or(0)
        + octocode_engine::json_size::string_body_units(value)
        + 1
}

/// Serialized chars of one listed file (`files` and count views): its path,
/// with the count `view` shows, priced in place. Equal to the length of the
/// [`SearchFile`](super::super::types::SearchFile) the view renders, which is never built here.
pub(super) fn listed_file_chars(
    file: &octocode_engine::types::TextSearchFile,
    view: View,
) -> usize {
    let count = match view {
        View::CountMatches => ",\"matchCount\":".len(),
        View::CountLines => ",\"matchedLineCount\":".len(),
        _ => return "{\"path\":\"\"}".len() + string_body_units(&file.path),
    };
    // `,"termCounts":[a,b,…]` when the view counts two or more terms.
    let terms = if file.term_counts.is_empty() {
        0
    } else {
        ",\"termCounts\":[]".len()
            + file.term_counts.len().saturating_sub(1)
            + file
                .term_counts
                .iter()
                .map(|count| digits(*count))
                .sum::<usize>()
    };
    "{\"path\":\"\"}".len()
        + string_body_units(&file.path)
        + count
        + digits(file.match_count)
        + terms
}

/// Serialized chars of `n` in decimal.
pub(super) fn digits(n: impl Into<u64>) -> usize {
    n.into().checked_ilog10().map_or(1, |log| log as usize + 1)
}

/// Most chars a streamed file entry's `pagination` can take: its later rows
/// are named as at most [`MAX_MORE_LINES`] lines plus a count.
pub(super) fn pagination_chars(file: &octocode_engine::types::TextSearchFile) -> usize {
    let total = file.matches.len();
    let widest_line = file.matches.iter().map(|m| m.line).max().unwrap_or(0);
    let listed = total.saturating_sub(1).min(MAX_MORE_LINES);
    let more_lines =
        listed * (digits(widest_line) + 1) + ",\"moreLinesUnlisted\":".len() + digits(total as u64);
    let empty = ItemPagination {
        page: None,
        total_pages: None,
        total_matches: Some(u32_of_usize(total)),
        has_more: true,
        more_lines: Some(Vec::new()),
        more_lines_unlisted: None,
        out_of_range: false,
    };
    ",\"pagination\":".len()
        + octocode_engine::json_size::utf16_units(&empty).unwrap_or(0)
        + more_lines
}

/// Whether every file entry (by index), with all its rows, fits `budget`
/// chars.
pub(super) fn fits_within(
    files: &[octocode_engine::types::TextSearchFile],
    budget: usize,
    mut entry_chars: impl FnMut(usize, &octocode_engine::types::TextSearchFile) -> usize,
) -> bool {
    let mut used = 0usize;
    files.iter().enumerate().all(|(index, file)| {
        used = used.saturating_add(entry_chars(index, file));
        used <= budget
    })
}

/// Whether every file entry, with all its rows and their names, fits
/// `budget` chars. With `unpriced_free`, a name not priced yet costs
/// nothing: a lower bound of the exact fit.
pub(super) fn fits_whole(
    files: &[octocode_engine::types::TextSearchFile],
    budget: usize,
    costs: &PageCosts,
    unpriced_free: bool,
) -> bool {
    fits_within(files, budget, |index, file| {
        costs.file(file)
            + file
                .matches
                .iter()
                .map(|m| {
                    let name = match costs.name(index, file, m) {
                        RowName::Reserved if unpriced_free => RowName::None,
                        name => name,
                    };
                    costs.row(m) + name.chars()
                })
                .sum::<usize>()
    })
}

/// Line runs `moreLines` names before it summarizes the rest as a count.
pub(super) const MAX_MORE_LINES: usize = 24;
