//! Page windows over a result set held whole: by a character budget (the
//! default, so a page fills the response window like localSearch and
//! structureSearch), ending sooner at a caller's `limit` rows. The budget
//! ([`page_budget`]) is the one page sizing owner: a default-layout page is
//! cut so the whole response row fits the runtime's automatic response window
//! (`auto_page_chars`); otherwise the response pager
//! splits the page into row parts and every page walk costs extra calls.
//!
//! The result set is the same on every page of a walk (a snapshot pins it),
//! so the cut is a pure function of the item sizes and the budget: page `n`
//! starts where page `n - 1` ended, every item lands on exactly one page, and
//! the page count is the same however the walk is entered.

use std::ops::Range;

use crate::response::pages::PageFacts;

/// How a result set is cut into pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Paging {
    /// As many items as fit the character budget; at least one, so a walk
    /// always moves.
    Chars(usize),
    /// The caller's `limit`: a page ends at `rows` rows or at the `chars`
    /// budget, whichever comes first; at least one item.
    Rows { rows: usize, chars: usize },
}

impl Paging {
    /// The cut a request asks for: its `limit` rows within the budget, else
    /// the budget alone.
    pub(crate) fn of(limit: Option<usize>, budget: usize) -> Self {
        let chars = budget.max(1);
        limit.map_or(Self::Chars(chars), |rows| Self::Rows {
            rows: rows.max(1),
            chars,
        })
    }

    /// Every page's item range over `sizes` (each item's characters), each
    /// item one row.
    #[cfg(test)]
    pub(crate) fn windows(self, sizes: &[usize]) -> Vec<Range<usize>> {
        self.weighted_windows(sizes, None)
    }

    /// [`Self::windows`] where item `i` counts `rows[i]` rows against the
    /// row limit (a file entry holding several match rows). An item heavier
    /// than the limit still takes a page alone, so a walk always moves.
    pub(crate) fn weighted_windows(
        self,
        sizes: &[usize],
        rows: Option<&[usize]>,
    ) -> Vec<Range<usize>> {
        let (budget, limit) = match self {
            Self::Chars(budget) => (budget, usize::MAX),
            Self::Rows { rows, chars } => (chars, rows),
        };
        let weight = |index: usize| rows.and_then(|rows| rows.get(index)).copied().unwrap_or(1);
        let mut windows = Vec::new();
        let mut start = 0;
        while start < sizes.len() {
            let (mut used, mut counted, mut end) = (0usize, 0usize, start);
            while let Some(size) = sizes.get(end) {
                let row = weight(end);
                if end > start
                    && (used.saturating_add(*size) > budget || counted.saturating_add(row) > limit)
                {
                    break;
                }
                used = used.saturating_add(*size);
                counted = counted.saturating_add(row);
                end += 1;
            }
            windows.push(start..end);
            start = end;
        }
        windows
    }
}

/// Item range of count page `page` (1-based) over `total` items of `size`
/// each. A page past the end is the empty range at `total`; page 0 reads as
/// the first page and size 0 as one item, so no caller can underflow.
pub(crate) fn count_window(page: usize, size: usize, total: usize) -> Range<usize> {
    let size = size.max(1);
    let start = page
        .max(1)
        .saturating_sub(1)
        .saturating_mul(size)
        .min(total);
    start..start.saturating_add(size).min(total)
}

/// The window of 1-based `page` among cut `windows`; page 0 reads as the
/// first page, and a page past the last window is `None`.
pub(crate) fn nth_page(windows: &[Range<usize>], page: usize) -> Option<&Range<usize>> {
    windows.get(page.max(1) - 1)
}

/// One selected page of a held result set: where it sits, how many pages the
/// cut has and the facts its `pagination` block states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PageCut {
    /// The page's item range; empty at the end of the set for a page past it.
    pub(crate) range: Range<usize>,
    /// Pages in the cut; an empty set is one page.
    pub(crate) pages: usize,
    /// Whether a page follows this one.
    pub(crate) more: bool,
    /// The page's facts, flagged `outOfRange` for a page past the last one.
    pub(crate) facts: PageFacts,
}

/// Page `page` (1-based; 0 reads as the first) of the items `sizes` describe,
/// cut by `paging`: the one place a held result set is windowed, counted and
/// checked for a request past its end. A page past the last one (page 1 of an
/// empty set never is) is the empty range at the end and flagged out of
/// range, never clamped to the last page, which would repeat rows the caller
/// has ([`PageFacts::past_end`] is the rule).
pub(crate) fn select(paging: Paging, sizes: &[usize], page: usize) -> PageCut {
    select_weighted(paging, sizes, None, page)
}

/// [`select`] with each item counting `rows[i]` rows against the row limit
/// ([`Paging::weighted_windows`]).
pub(crate) fn select_weighted(
    paging: Paging,
    sizes: &[usize],
    rows: Option<&[usize]>,
    page: usize,
) -> PageCut {
    let total = sizes.len();
    let page = page.max(1);
    let windows = paging.weighted_windows(sizes, rows);
    let range = nth_page(&windows, page).cloned().unwrap_or(total..total);
    let pages = windows.len().max(1);
    let more = page < pages;
    let facts = PageFacts::sized(page, pages, total, more);
    let past_end = facts.past_end().is_some();
    PageCut {
        range,
        pages,
        more,
        facts: facts.out_of_range(past_end),
    }
}

/// Pages of `size` items over `total` items; an empty set is one page.
pub(crate) fn count_pages(total: usize, size: usize) -> usize {
    total.div_ceil(size.max(1)).max(1)
}

/// Characters one page of a row may spend on its items: the response window
/// less the row around them (a fixed skeleton charge, whatever the query).
pub(crate) fn page_budget(response_window: Option<usize>) -> usize {
    page_chars(response_window, ROW_SKELETON_CHARS)
}

/// Fallback when a caller has no configured response window. Runtime calls
/// carry their window and reserve the surrounding row from that budget.
const FALLBACK_PAGE_CHARS: usize = 24_000;

/// Room for everything a response row holds besides its listed entries:
/// the envelope, stats, pagination, warnings, and the fixed text of row
/// continuations and handoffs.
const ROW_SKELETON_CHARS: usize = 2_000;

/// Chars a streamed page's entries may take. With a response window, the
/// page leaves room for the row around it ([`reserve_chars`]); a tiny window
/// still gets half of itself, so a walk keeps moving (its pages then split
/// into row parts). Without one, pages take [`FALLBACK_PAGE_CHARS`].
fn page_chars(window: Option<usize>, reserve: usize) -> usize {
    match window {
        Some(window) => window.saturating_sub(reserve).max(window / 2).max(1),
        None => FALLBACK_PAGE_CHARS,
    }
}

#[cfg(test)]
mod tests;
