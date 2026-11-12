//! The pages of a match set: one stream of matches in walk order, cut into
//! pages of up to `limit` matches within the response budget, whichever
//! files hold them. A file's header is charged on each page its matches
//! appear on. The cut is a pure function of the scan's sizes, so page `n`
//! starts where page `n - 1` ended, every match lands on exactly one page,
//! and the page count holds however the walk is entered. The file views
//! (`files`, `countMatches`) page their files instead.
use super::query::MatchQuery;
use super::types::Group;
use crate::response::pages::PageFacts;
use serde_json::{Map, Value};
use std::ops::Range;

/// The page a query asks for.
pub(super) struct StreamPage {
    /// Its files, as a range of every file's groups.
    pub(super) window: std::ops::Range<usize>,
    /// The matches it shows of each file in `window`, in order; empty in
    /// the file views, which show each file whole.
    pub(super) slices: Vec<Range<usize>>,
    /// Pages of the whole set; an empty set is one page.
    pub(super) pages: usize,
    /// A page follows this one.
    pub(super) more: bool,
    /// A page outgrew the budget: the response pager splits it into row
    /// parts, so `pages` is not the number of calls the walk takes.
    pub(super) oversized: bool,
    /// Its pagination block.
    pub(super) pagination: Value,
}

/// A place in the stream: a file's group and a match in it.
type At = (usize, usize);

/// Every page's first match and the place after its last, and whether a
/// page spends more than `budget` (one match alone outgrows it).
fn cut(groups: &[Group], limit: usize, budget: usize) -> (Vec<(At, At)>, bool) {
    let (mut pages, mut oversized) = (Vec::new(), false);
    let (mut start, mut used, mut count, mut open) = ((0, 0), 0usize, 0usize, None);
    for (file, group) in groups.iter().enumerate() {
        for (index, size) in group.sizes.iter().enumerate() {
            let header = |open: Option<usize>| if open == Some(file) { 0 } else { group.chars };
            if count > 0 && (count >= limit || used.saturating_add(size + header(open)) > budget) {
                pages.push((start, (file, index)));
                oversized |= used > budget;
                (start, used, count, open) = ((file, index), 0, 0, None);
            }
            used = used.saturating_add(size + header(open));
            count += 1;
            open = Some(file);
        }
    }
    if count > 0 {
        pages.push((start, (groups.len(), 0)));
        oversized |= used > budget;
    }
    (pages, oversized)
}

/// Page `q.page()` of the matches `groups` hold (of their files, in the file
/// views): where it starts in the stream, the matches it shows and the
/// stream's total. A page past the last is empty and flagged out of range.
pub(super) fn stream_page(q: MatchQuery<'_>, groups: &[Group], budget: usize) -> StreamPage {
    let page = q.page().max(1) as usize;
    if super::file_list(q) {
        let paging = crate::response::page_windows::Paging::of(q.limit(), budget);
        let sizes = groups.iter().map(|group| group.chars).collect::<Vec<_>>();
        let cut = crate::response::page_windows::select(paging, &sizes, page);
        return StreamPage {
            window: cut.range,
            slices: Vec::new(),
            pages: cut.pages,
            more: cut.more,
            oversized: false,
            pagination: cut.facts.with_unit("files").to_value(),
        };
    }
    let limit = q.limit().map_or(usize::MAX, |limit| limit.max(1));
    let (cuts, oversized) = cut(groups, limit, budget.max(1));
    let pages = cuts.len().max(1);
    let total = groups.iter().map(|group| group.sizes.len()).sum::<usize>();
    let more = page < pages;
    let past_end = page > pages;
    let (window, slices) = cuts.get(page - 1).map_or_else(
        || (groups.len()..groups.len(), Vec::new()),
        |&(from, to)| slices(groups, from, to),
    );
    // The page starts after every match of the files before its first and
    // the matches of that file before its first shown one.
    let offset = cuts.get(page - 1).map_or(total, |&((file, index), _)| {
        groups[..file].iter().map(|group| group.sizes.len()).sum::<usize>() + index
    });
    let length = slices.iter().map(ExactSizeIterator::len).sum::<usize>();
    let facts = PageFacts::open(offset, length, more)
        .with_unit("matches")
        .with_total(total)
        .out_of_range(past_end);
    let block = match facts.to_value() {
        Value::Object(fields) => fields,
        _ => Map::new(),
    };
    StreamPage {
        window,
        slices,
        pages,
        more,
        oversized,
        pagination: Value::Object(block),
    }
}

/// The files a page from `from` up to `to` touches, and the matches it
/// shows of each.
fn slices(groups: &[Group], from: At, to: At) -> (Range<usize>, Vec<Range<usize>>) {
    let last = if to.1 == 0 { to.0 - 1 } else { to.0 };
    let slices = (from.0..=last)
        .map(|file| {
            let start = if file == from.0 { from.1 } else { 0 };
            let end = if file == to.0 {
                to.1
            } else {
                groups[file].sizes.len()
            };
            start..end
        })
        .collect();
    (from.0..last + 1, slices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn group(header: usize, sizes: &[usize]) -> Group {
        Group {
            row: json!({"path": "f"}),
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
                    used += groups[file].chars
                        + groups[file].sizes[slice.clone()].iter().sum::<usize>();
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
}
