//! Serialized sizes of one file entry and its rows, and the per-file
//! paging an entry reports.
use crate::tools::local_search::local_search_query::View;

use super::super::rows::project_shape;
use super::super::types::Paging;
use super::Layout;
use super::costs::{PageCosts, RowName};
use octocode_engine::json_size::string_body_units;

/// Per-file paging, reported only while it routes somewhere: more rows
/// remain, or the requested match page is past the end. It counts the
/// file's hit rows (`matches`, in the layout's row unit): the shown ones from `start`, of every row
/// the file has, so the rows on later pages are counted, not listed: the
/// page cursor reaches them.
pub(crate) fn file_pagination(
    layout: &Layout,
    total: u32,
    (start, end): (usize, usize),
    match_page: u32,
) -> Option<Paging> {
    if layout.list {
        return None;
    }
    let total = total as usize;
    let (start, end) = (start.min(total), end.min(total));
    let shown = (start, end - start, total);
    if layout.streamed {
        return (end < total).then(|| Paging::of(layout.row_unit, shown, true, false));
    }
    let total_pages =
        crate::response::page_windows::count_pages(total, layout.matches_per as usize);
    let has_more = (match_page as usize) < total_pages;
    let out_of_range = start >= total && total > 0;
    (has_more || out_of_range).then(|| Paging::of(layout.row_unit, shown, has_more, out_of_range))
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
    let (row, value) = project_shape(matched, span_columns);
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

/// Chars a streamed file entry's `pagination` takes at most: every count at
/// the file's row total, the longer row unit (`matches`), and the flag.
pub(super) fn pagination_chars(file: &octocode_engine::types::TextSearchFile) -> usize {
    let total = file.matches.len();
    let widest = Paging::of("matches", (total, total, total), true, false);
    ",\"pagination\":".len() + octocode_engine::json_size::utf16_units(&widest).unwrap_or(0)
}

/// Whether every file entry, with all its rows and their names, fits
/// `budget` chars. With `unpriced_free`, a name not priced yet costs
/// nothing: a lower bound of the exact fit. Pricing stops at the first row
/// past the budget, so a result far over it costs only what fits.
pub(super) fn fits_whole(
    files: &[octocode_engine::types::TextSearchFile],
    budget: usize,
    costs: &PageCosts,
    unpriced_free: bool,
) -> bool {
    let mut used = 0usize;
    files.iter().enumerate().all(|(index, file)| {
        used = used.saturating_add(costs.file(file));
        used <= budget
            && file.matches.iter().all(|m| {
                let name = match costs.name(index, file, m) {
                    RowName::Reserved if unpriced_free => RowName::None,
                    name => name,
                };
                used = used.saturating_add(costs.row(m) + name.chars());
                used <= budget
            })
    })
}
