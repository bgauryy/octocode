//! Page windows over a result set held whole: by a character budget (the
//! default, so a page fills the response window like localSearch and
//! structureSearch) or by a caller's explicit item count. The budget
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
    /// `n` items per page: the caller's explicit `pageSize`.
    Count(usize),
    /// As many items as fit the character budget; at least one, so a walk
    /// always moves.
    Chars(usize),
}

impl Paging {
    /// The cut a request asks for: its explicit count, else the budget.
    pub(crate) fn of(count: Option<usize>, budget: usize) -> Self {
        count.map_or(Self::Chars(budget.max(1)), |count| {
            Self::Count(count.max(1))
        })
    }

    /// Every page's item range over `sizes` (each item's characters).
    pub(crate) fn windows(self, sizes: &[usize]) -> Vec<Range<usize>> {
        let mut windows = Vec::new();
        let mut start = 0;
        while start < sizes.len() {
            let end = match self {
                Self::Count(count) => start.saturating_add(count).min(sizes.len()),
                Self::Chars(budget) => {
                    let mut used = 0usize;
                    let mut end = start;
                    while let Some(size) = sizes.get(end) {
                        if end > start && used.saturating_add(*size) > budget {
                            break;
                        }
                        used = used.saturating_add(*size);
                        end += 1;
                    }
                    end
                }
            };
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
    let total = sizes.len();
    let page = page.max(1);
    let (range, pages) = match paging {
        Paging::Count(size) => (count_window(page, size, total), count_pages(total, size)),
        Paging::Chars(_) => {
            let windows = paging.windows(sizes);
            let range = nth_page(&windows, page).cloned().unwrap_or(total..total);
            (range, windows.len().max(1))
        }
    };
    let more = page < pages;
    let facts = match paging {
        Paging::Count(size) => PageFacts::counted(page, size, total),
        Paging::Chars(_) => PageFacts::sized(page, pages, total, more),
    };
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
/// less the row around them, which re-states the query in each continuation.
pub(crate) fn page_budget(response_window: Option<usize>, _query: &serde_json::Value) -> usize {
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
