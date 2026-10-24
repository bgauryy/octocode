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
            let windows = Paging::Count(size).windows(&(0..total).map(|_| 1).collect::<Vec<_>>());
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
        large
            .iter()
            .map(std::iter::ExactSizeIterator::len)
            .sum::<usize>(),
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

/// One selection for both cuts: the page's range, the page count and the
/// facts it states, and one past-end rule (a page after the last is the
/// empty range at the end, flagged, never a clamped repeat; page 1 of an
/// empty set is a plain page).
#[test]
fn select_cuts_counted_and_sized_pages_with_one_past_end_rule() {
    let sizes = [400, 400, 400, 900, 100];
    for (paging, pages, ranges) in [
        (Paging::Count(2), 3, vec![0..2, 2..4, 4..5]),
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
    for paging in [Paging::Count(2), Paging::Chars(1_000)] {
        let first = select(paging, &[], 1);
        assert_eq!(first.range, 0..0);
        assert_eq!((first.pages, first.more), (1, false));
        assert!(first.facts.to_value().get("outOfRange").is_none());
        let second = select(paging, &[], 2);
        assert_eq!(second.facts.to_value()["outOfRange"], true);
    }
}
