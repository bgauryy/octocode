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
        octocode_engine::json_size::utf16_units(row)
            .unwrap_or(0)
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

/// Instruction, cursor and reporting fields do not change source-page
/// boundaries (`extras` only adds optional row facts). The final response
/// pager charges their serialized size, including context removed by an
/// unfiltered recovery over the same captured source.
const UNSIZED_QUERY_FIELDS: [&str; 8] = [
    "extras",
    "mainGoal",
    "reasoning",
    "page",
    "matchPage",
    "snapshot",
    "selection",
    "clasifyContext",
];

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
            .filter(|(name, _)| !UNSIZED_QUERY_FIELDS.contains(&name.as_str()))
            .map(|(name, value)| {
                crate::response::stream_page::json_text_chars(name)
                    + octocode_engine::json_size::utf16_units(value).unwrap_or(0)
                    + 4
            })
            .sum::<usize>(),
        None => octocode_engine::json_size::utf16_units(query).unwrap_or(0),
    };
    ROW_SKELETON_CHARS + FREE_TEXT_CHARS + copies.saturating_mul(measured)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_row_uses_serialized_cost_and_keeps_the_envelope_reserve() {
        let row = serde_json::json!({"files": [{"path": "日本語.rs", "matches": [
            {"line": 7, "value": "quoted \"source\"\n😀"}
        ]}]});
        let needed =
            octocode_engine::json_size::utf16_units(&row).unwrap_or(0) + ROW_SKELETON_CHARS;
        assert!(terminal_row_fits(Some(needed), &row));
        assert!(!terminal_row_fits(Some(needed - 1), &row));
        assert!(!terminal_row_fits(Some(0), &row));
        assert!(!terminal_row_fits(None, &row));
    }

    /// Asking for optional row extras leaves the page cut unchanged, so a
    /// walk's pages and snapshots do not depend on them.
    #[test]
    fn extras_do_not_change_the_page_budget() {
        let plain = serde_json::json!({"path": "src", "matchString": "x"});
        let mut extras = plain.clone();
        extras["extras"] = serde_json::json!(["hints", "details", "stats", "receipts"]);
        assert_eq!(
            page_budget(Some(40_000), &plain),
            page_budget(Some(40_000), &extras)
        );
    }

    #[test]
    fn a_character_budget_fills_each_page_and_never_exceeds_it() {
        let sizes = [400, 400, 400, 900, 100, 100, 5_000, 10];
        let windows = Paging::Chars(1_000).windows(&sizes);
        assert_eq!(windows, vec![0..2, 2..3, 3..5, 5..6, 6..7, 7..8]);
        for window in &windows {
            let used: usize = sizes[window.clone()].iter().sum();
            // Only a single item larger than the budget overflows it.
            assert!(used <= 1_000 || window.len() == 1, "{window:?} {used}");
        }
    }

    #[test]
    fn every_item_lands_on_exactly_one_page_for_any_budget_or_count() {
        let sizes = (0..257)
            .map(|index| 1 + (index * 37) % 211)
            .collect::<Vec<_>>();
        for paging in [
            Paging::Chars(1),
            Paging::Chars(500),
            Paging::Chars(16_384),
            Paging::Chars(usize::MAX),
            Paging::Count(1),
            Paging::Count(20),
            Paging::Count(1_000),
        ] {
            let mut seen = vec![0u8; sizes.len()];
            let windows = paging.windows(&sizes);
            for window in &windows {
                assert!(!window.is_empty(), "{paging:?}");
                for item in window.clone() {
                    seen[item] += 1;
                }
            }
            assert!(seen.iter().all(|count| *count == 1), "{paging:?}");
            // Each page starts where the one before ended.
            assert!(windows.windows(2).all(|pair| pair[0].end == pair[1].start));
        }
    }

    #[test]
    fn a_set_that_fits_is_one_page_and_an_empty_set_has_none() {
        assert_eq!(Paging::Chars(100).windows(&[10, 10]), vec![0..2]);
        assert!(Paging::Chars(100).windows(&[]).is_empty());
    }

    #[test]
    fn an_explicit_count_overrides_the_budget() {
        assert_eq!(Paging::of(Some(3), 16_000), Paging::Count(3));
        assert_eq!(Paging::of(None, 16_000), Paging::Chars(16_000));
        assert_eq!(Paging::of(Some(0), 16_000), Paging::Count(1));
    }

    #[test]
    fn a_count_page_is_the_closed_form_window_and_past_the_end_is_empty() {
        assert_eq!(count_window(1, 3, 7), 0..3);
        assert_eq!(count_window(3, 3, 7), 6..7);
        assert_eq!(count_window(4, 3, 7), 7..7);
        assert_eq!(
            count_window(0, 3, 7),
            0..3,
            "page 0 reads as the first page"
        );
        assert_eq!(count_window(1, 0, 7), 0..1, "size 0 reads as one item");
        assert_eq!(count_window(2, 3, 0), 0..0);
    }

    #[test]
    fn count_arithmetic_survives_extreme_pages_and_sizes() {
        // Pages and sizes come from request input: none may overflow or panic.
        assert_eq!(
            count_window(usize::MAX, usize::MAX, usize::MAX),
            usize::MAX..usize::MAX
        );
        assert_eq!(count_window(usize::MAX, 1, 10), 10..10);
        assert_eq!(count_window(2, usize::MAX, 10), 10..10);
        assert_eq!(count_pages(usize::MAX, 1), usize::MAX);
        assert_eq!(count_pages(0, 0), 1);
        let one_window = [Range { start: 0, end: 1 }];
        assert!(nth_page(&one_window, usize::MAX).is_none());
    }

    #[test]
    fn windows_survive_items_larger_than_any_budget() {
        let sizes = [usize::MAX, 1, usize::MAX];
        for paging in [
            Paging::Chars(1),
            Paging::Chars(usize::MAX),
            Paging::Count(1),
        ] {
            let windows = paging.windows(&sizes);
            let covered: usize = windows.iter().map(|w| w.len()).sum();
            assert_eq!(covered, sizes.len(), "{paging:?}");
            assert!(windows.iter().all(|w| !w.is_empty()), "{paging:?}");
        }
    }

    #[test]
    fn nth_page_is_one_based_and_page_zero_is_the_first() {
        let windows = [0..2, 2..5];
        assert_eq!(nth_page(&windows, 0), Some(&(0..2)));
        assert_eq!(nth_page(&windows, 2), Some(&(2..5)));
        assert_eq!(nth_page(&windows, 3), None);
        assert_eq!(nth_page(&[], 1), None);
    }

    #[test]
    fn count_pages_agrees_with_the_windows_it_names() {
        for total in [0_usize, 1, 2, 7, 64] {
            for size in [1_usize, 3, 64] {
                let windows =
                    Paging::Count(size).windows(&(0..total).map(|_| 1).collect::<Vec<_>>());
                assert_eq!(
                    count_pages(total, size),
                    windows.len().max(1),
                    "{total}/{size}"
                );
            }
        }
    }

    #[test]
    fn the_budget_leaves_the_row_room_inside_the_window() {
        let query = serde_json::json!({"operation": "symbols", "path": "src"});
        let budget = page_budget(Some(20_000), &query);
        assert!((10_000..20_000).contains(&budget), "{budget}");
        assert_eq!(page_budget(None, &query), FALLBACK_PAGE_CHARS);
    }

    #[test]
    fn page_chars_leave_the_row_room_inside_the_window() {
        assert_eq!(page_chars(None, 5_000), FALLBACK_PAGE_CHARS);
        assert_eq!(page_chars(Some(50_000), 4_000), 46_000);
        assert_eq!(page_chars(Some(20_000), 4_000), 16_000);
        assert_eq!(page_chars(Some(1_000), 4_000), 500);
    }

    #[test]
    fn configured_windows_reduce_hops_without_changing_explicit_counts() {
        let query = serde_json::json!({"path":"src","operation":"symbols"});
        let sizes = vec![500; 120];
        let small = Paging::of(None, page_budget(Some(24_000), &query)).windows(&sizes);
        let large = Paging::of(None, page_budget(Some(50_000), &query)).windows(&sizes);
        assert!(large.len() < small.len());
        assert_eq!(
            large.iter().map(|window| window.len()).sum::<usize>(),
            sizes.len()
        );
        for budget in [24_000, 50_000] {
            let explicit = Paging::of(Some(17), page_budget(Some(budget), &query)).windows(&sizes);
            assert!(
                explicit
                    .iter()
                    .take(explicit.len() - 1)
                    .all(|window| window.len() == 17)
            );
        }
    }

    #[test]
    fn reserve_ignores_free_text_and_cursor_fields() {
        let first = serde_json::json!({"mainGoal": "g", "reasoning": "r", "path": "src", "matchString": "x"});
        let later = serde_json::json!({"mainGoal": "a much longer goal", "reasoning": "reworded",
            "path": "src", "matchString": "x", "page": 7, "snapshot": "lexical-live-v2:abc",
            "selection": "saved-order-added-after-the-first-page"});
        assert_eq!(reserve_chars(&first, 2), reserve_chars(&later, 2));
        let wider = serde_json::json!({"path": "src", "matchString": "a longer search"});
        assert!(reserve_chars(&wider, 2) > reserve_chars(&first, 2));
    }
}
