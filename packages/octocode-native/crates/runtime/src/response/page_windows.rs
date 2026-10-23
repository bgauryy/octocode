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
use crate::response::result::{CURSOR_FIELDS, INTENT_FIELDS};

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
pub(crate) fn page_budget(response_window: Option<usize>, query: &serde_json::Value) -> usize {
    page_chars(response_window, reserve_chars(query, 2))
}

/// A complete held row needs no repeated-query reserve for its own next page.
/// Keep the common envelope reserve; later response shaping still enforces the window.
pub(crate) fn terminal_row_fits(response_window: Option<usize>, row: &serde_json::Value) -> bool {
    response_window.is_some_and(|window| {
        octocode_engine::json_size::utf16_units_or(row, usize::MAX)
            .saturating_add(ROW_SKELETON_CHARS)
            <= window
    })
}

/// Fallback when a caller has no configured response window. Runtime calls
/// carry their window and reserve the surrounding row from that budget.
const FALLBACK_PAGE_CHARS: usize = 24_000;

/// Room for everything a response row holds besides its listed entries:
/// the envelope, stats, pagination, warnings, and the fixed text of row
/// continuations and handoffs.
const ROW_SKELETON_CHARS: usize = 2_000;

/// Room for the caller's `mainGoal` and `reasoning`, which row continuations
/// copy. They are budgeted at a fixed size rather than measured, so a walk
/// whose continuation rewords them still cuts every page at the same rows.
const FREE_TEXT_CHARS: usize = 1_200;

/// Whether a query field leaves the page boundaries unchanged: a cursor (it
/// names where a walk stands, [`CURSOR_FIELDS`]), an intent field (it steers
/// how a call reports, [`INTENT_FIELDS`]) or `clasifyContext`. The one
/// derivation from the contract's field sets, so a new cursor never drifts a
/// continuation's budget from its first page's. The final response pager
/// charges their serialized size, including context removed by an unfiltered
/// recovery over the same captured source.
fn is_unsized_field(name: &str) -> bool {
    CURSOR_FIELDS.contains(&name) || INTENT_FIELDS.contains(&name) || name == "clasifyContext"
}

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

/// What a response row needs besides its entries, for a row whose
/// continuations copy `query` `copies` times. Only fields that select rows
/// are measured, so every page of one walk gets the same budget.
fn reserve_chars(query: &serde_json::Value, copies: usize) -> usize {
    let measured = match query.as_object() {
        Some(fields) => fields
            .iter()
            .filter(|(name, _)| !is_unsized_field(name))
            .map(|(name, value)| {
                octocode_engine::json_size::string_body_units(name)
                    .saturating_add(octocode_engine::json_size::utf16_units_or(
                        value,
                        usize::MAX,
                    ))
                    .saturating_add(4)
            })
            .fold(0_usize, usize::saturating_add),
        None => octocode_engine::json_size::utf16_units_or(query, usize::MAX),
    };
    ROW_SKELETON_CHARS
        .saturating_add(FREE_TEXT_CHARS)
        .saturating_add(copies.saturating_mul(measured))
}

#[cfg(test)]
mod tests;
