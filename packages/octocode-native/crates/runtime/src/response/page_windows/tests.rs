use super::*;

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
        Paging::Rows {
            rows: 1,
            chars: usize::MAX,
        },
        Paging::Rows {
            rows: 20,
            chars: usize::MAX,
        },
        Paging::Rows {
            rows: 1_000,
            chars: usize::MAX,
        },
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
fn a_limit_bounds_rows_within_the_budget() {
    assert_eq!(
        Paging::of(Some(3), 16_000),
        Paging::Rows {
            rows: 3,
            chars: 16_000
        }
    );
    assert_eq!(Paging::of(None, 16_000), Paging::Chars(16_000));
    assert_eq!(
        Paging::of(Some(0), 16_000),
        Paging::Rows {
            rows: 1,
            chars: 16_000
        },
        "at least one row per page"
    );
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
        Paging::Rows {
            rows: 1,
            chars: usize::MAX,
        },
    ] {
        let windows = paging.windows(&sizes);
        let covered: usize = windows.iter().map(std::iter::ExactSizeIterator::len).sum();
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
            let windows = Paging::Rows {
                rows: size,
                chars: usize::MAX,
            }
            .windows(&(0..total).map(|_| 1).collect::<Vec<_>>());
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
    let budget = page_budget(Some(20_000));
    assert!((10_000..20_000).contains(&budget), "{budget}");
    assert_eq!(page_budget(None), FALLBACK_PAGE_CHARS);
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
    let sizes = vec![500; 120];
    let small = Paging::of(None, page_budget(Some(24_000))).windows(&sizes);
    let large = Paging::of(None, page_budget(Some(50_000))).windows(&sizes);
    assert!(large.len() < small.len());
    assert_eq!(
        large
            .iter()
            .map(std::iter::ExactSizeIterator::len)
            .sum::<usize>(),
        sizes.len()
    );
    for budget in [24_000, 50_000] {
        let explicit = Paging::of(Some(17), page_budget(Some(budget))).windows(&sizes);
        assert!(
            explicit
                .iter()
                .take(explicit.len() - 1)
                .all(|window| window.len() == 17)
        );
    }
}

/// One selection for both cuts: the page's range, the page count and the
/// facts it states, and one past-end rule (a page after the last is the
/// empty range at the end, flagged, never a clamped repeat; page 1 of an
/// empty set is a plain page).
#[test]
fn select_cuts_counted_and_sized_pages_with_one_past_end_rule() {
    let sizes = [400, 400, 400, 900, 100];
    for (paging, pages, ranges) in [
        (
            Paging::Rows {
                rows: 2,
                chars: usize::MAX,
            },
            3,
            vec![0..2, 2..4, 4..5],
        ),
        (Paging::Chars(1_000), 3, vec![0..2, 2..3, 3..5]),
    ] {
        for (index, range) in ranges.iter().enumerate() {
            let cut = select(paging, &sizes, index + 1);
            assert_eq!(cut.range, *range, "{paging:?} page {}", index + 1);
            assert_eq!(cut.pages, pages, "{paging:?}");
            assert_eq!(cut.more, index + 1 < pages, "{paging:?}");
            assert!(cut.facts.past_end().is_none(), "{paging:?}");
            assert!(cut.facts.to_value().get("outOfRange").is_none());
        }
        // Page 0 reads as the first page.
        assert_eq!(select(paging, &sizes, 0), select(paging, &sizes, 1));
        let past = select(paging, &sizes, pages + 1);
        assert_eq!(past.range, 5..5, "{paging:?}");
        assert!(!past.more, "{paging:?}");
        let block = past.facts.to_value();
        assert_eq!(block["outOfRange"], true, "{block}");
        assert_eq!(block["hasMore"], false, "{block}");
        assert_eq!(block["totalPages"], 3, "{block}");
        assert_eq!(block["page"], 4, "{block}");
    }
    for paging in [
        Paging::Rows {
            rows: 2,
            chars: usize::MAX,
        },
        Paging::Chars(1_000),
    ] {
        let first = select(paging, &[], 1);
        assert_eq!(first.range, 0..0);
        assert_eq!((first.pages, first.more), (1, false));
        assert!(first.facts.to_value().get("outOfRange").is_none());
        let second = select(paging, &[], 2);
        assert_eq!(second.facts.to_value()["outOfRange"], true);
    }
}

/// `limit` (rows) and the character budget both end a page, whichever comes
/// first; a page holds at least one item, and a walk at a small limit lists
/// every item exactly once.
#[test]
fn a_page_ends_at_the_limit_or_the_budget_whichever_comes_first() {
    let sizes = [10, 10, 10, 500, 10, 10, 10, 10];
    let rows = Paging::of(Some(3), 1_000).windows(&sizes);
    assert_eq!(rows, vec![0..3, 3..6, 6..8], "limit first");
    let chars = Paging::of(Some(100), 30).windows(&sizes);
    assert_eq!(chars, vec![0..3, 3..4, 4..7, 7..8], "budget first");
    for limit in 1..=sizes.len() {
        let windows = Paging::of(Some(limit), 40).windows(&sizes);
        assert!(windows.iter().all(|w| !w.is_empty() && w.len() <= limit));
        let walked = windows.iter().flat_map(Clone::clone).collect::<Vec<_>>();
        assert_eq!(
            walked,
            (0..sizes.len()).collect::<Vec<_>>(),
            "limit {limit}"
        );
    }
}

/// A weighted item (a file of several match rows) counts its rows against
/// the limit; one heavier than the limit still takes a page alone.
#[test]
fn weighted_items_count_their_rows_against_the_limit() {
    let sizes = [1, 1, 1, 1];
    let rows = [2, 2, 5, 1];
    let windows = Paging::of(Some(4), usize::MAX).weighted_windows(&sizes, Some(&rows));
    assert_eq!(windows, vec![0..2, 2..3, 3..4]);
    let cut = select_weighted(Paging::of(Some(4), usize::MAX), &sizes, Some(&rows), 2);
    assert_eq!((cut.range, cut.pages, cut.more), (2..3, 3, true));
}
