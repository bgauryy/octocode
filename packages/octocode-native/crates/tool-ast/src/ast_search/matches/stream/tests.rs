use super::*;
use octocode_test_support::json_fixture;

fn group(header: usize, sizes: &[usize]) -> Group {
    Group {
        row: json_fixture(r#"{"path": "f"}"#),
        source_descriptors: Vec::new(),
        chars: header,
        sizes: sizes.to_vec(),
        held: None,
        truncated_captures: false,
        priced: None,
    }
}

/// Every match lands on exactly one page, in walk order, and a page
/// holds at most `limit` matches; a header is charged once per page.
#[test]
fn every_match_lands_on_one_page_within_limit_and_budget() {
    let groups = [group(10, &[5, 5, 5]), group(10, &[5]), group(10, &[5, 5])];
    for (limit, budget) in [(1, 1000), (2, 1000), (4, 1000), (usize::MAX, 30), (3, 25)] {
        let (pages, oversized) = cut(&groups, limit, budget);
        assert!(!oversized, "{limit} {budget}");
        let mut seen = Vec::new();
        for &(from, to) in &pages {
            let (window, slices) = slices(&groups, from, to);
            let mut used = 0;
            for (file, slice) in window.zip(slices) {
                assert!(!slice.is_empty());
                used +=
                    groups[file].chars + groups[file].sizes[slice.clone()].iter().sum::<usize>();
                seen.extend(slice.map(|index| (file, index)));
            }
            assert!(used <= budget, "{limit} {budget}: {used}");
        }
        let all = [(0, 0), (0, 1), (0, 2), (1, 0), (2, 0), (2, 1)];
        assert_eq!(seen, all, "{limit} {budget}");
        assert!(pages.iter().all(|&(from, to)| {
            let (window, slices) = slices(&groups, from, to);
            window.len() == slices.len()
                && slices.iter().map(ExactSizeIterator::len).sum::<usize>() <= limit
        }));
    }
    assert_eq!(cut(&groups, 4, 1000).0.len(), 2);
}

/// A match larger than the budget still takes a page alone, and the cut
/// says a page outgrew it.
#[test]
fn a_match_over_the_budget_takes_a_page_alone_and_is_flagged() {
    let groups = [group(10, &[5, 500, 5])];
    let (pages, oversized) = cut(&groups, usize::MAX, 100);
    assert!(oversized);
    assert_eq!(
        pages,
        [((0, 0), (0, 1)), ((0, 1), (0, 2)), ((0, 2), (1, 0))]
    );
}
