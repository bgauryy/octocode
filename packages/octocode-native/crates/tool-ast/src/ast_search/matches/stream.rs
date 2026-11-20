//! The pages of a match set: one stream of matches in walk order, cut into
//! pages of up to `limit` matches within the response budget, whichever
//! files hold them. A file's header is charged on each page its matches
//! appear on. The cut is a pure function of the scan's sizes, so page `n`
//! starts where page `n - 1` ended, every match lands on exactly one page,
//! and the page count holds however the walk is entered. The file views
//! (`files`, `countMatches`) page their files instead.
use super::query::MatchQuery;
use super::types::Group;
use octocode_runtime_base::response::pages::PageFacts;
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

impl StreamPage {
    /// This page as a later window's: its offset and total count on after
    /// the `before` rows the windows before it listed.
    pub(super) fn count_from(&mut self, before: u64) {
        let Some(block) = self.pagination.as_object_mut().filter(|_| before > 0) else {
            return;
        };
        for key in ["offset", "total"] {
            if let Some(count) = block.get(key).and_then(Value::as_u64) {
                block.insert(key.to_owned(), Value::from(count.saturating_add(before)));
            }
        }
    }
}

/// A place in the stream: a file's group and a match in it.
type At = (usize, usize);

/// The page cut, fed one match at a time in stream order: a page closes
/// before the match that would pass `limit` matches or the `budget`. A
/// file's header is charged on each page its matches are on.
struct Cutter {
    limit: usize,
    budget: usize,
    /// Every closed page's first match and the place after its last.
    pages: Vec<(At, At)>,
    /// A page spent more than the budget (one match alone outgrows it).
    oversized: bool,
    start: At,
    used: usize,
    count: usize,
    /// The file the open page's last match is in.
    open: Option<usize>,
}

impl Cutter {
    fn new(limit: usize, budget: usize) -> Self {
        Self {
            limit,
            budget,
            pages: Vec::new(),
            oversized: false,
            start: (0, 0),
            used: 0,
            count: 0,
            open: None,
        }
    }

    /// Match `index` of file `file`, of `size`, whose file's header is
    /// `header`.
    fn push(&mut self, (file, index): At, header: usize, size: usize) {
        let header_on = |open: Option<usize>| if open == Some(file) { 0 } else { header };
        if self.count > 0
            && (self.count >= self.limit
                || self.used.saturating_add(size + header_on(self.open)) > self.budget)
        {
            self.pages.push((self.start, (file, index)));
            self.oversized |= self.used > self.budget;
            (self.used, self.count, self.open) = (0, 0, None);
        }
        // A page starts at its first match: the stream's first may lie past
        // a file's first ([`Bounds`]).
        if self.count == 0 {
            self.start = (file, index);
        }
        self.used = self.used.saturating_add(size + header_on(self.open));
        self.count += 1;
        self.open = Some(file);
    }

    /// Every page, the open one closed at `end`, the place after the
    /// stream's last match.
    fn finish(mut self, end: At) -> (Vec<(At, At)>, bool) {
        if self.count > 0 {
            self.pages.push((self.start, end));
            self.oversized |= self.used > self.budget;
        }
        (self.pages, self.oversized)
    }
}

/// Every page's first match and the place after its last, and whether a
/// page spends more than `budget` (one match alone outgrows it): the pages
/// of the matches `spans` names of each file.
fn cut(
    groups: &[Group],
    spans: &[Range<usize>],
    limit: usize,
    budget: usize,
) -> (Vec<(At, At)>, bool) {
    let mut cutter = Cutter::new(limit, budget);
    for (file, (group, span)) in groups.iter().zip(spans).enumerate() {
        for index in span.clone() {
            cutter.push((file, index), group.chars, group.sizes[index]);
        }
    }
    let end = match spans.last() {
        Some(last) if last.end < groups[spans.len() - 1].sizes.len() => (spans.len() - 1, last.end),
        _ => (groups.len(), 0),
    };
    cutter.finish(end)
}

/// Where a window's stream lies in the matches its files hold: past the
/// `carry` matches of its first file the window before listed (when it
/// reads that file again), and up to the page start inside its last file
/// it ended at (`split`, [`octocode_engine::structural::PagedWindow`]).
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Bounds {
    pub(super) carry: usize,
    pub(super) split: Option<usize>,
}

impl Bounds {
    /// The matches each file of `groups` holds in the stream.
    fn spans(self, groups: &[Group]) -> Vec<Range<usize>> {
        let last = groups.len().saturating_sub(1);
        groups
            .iter()
            .enumerate()
            .map(|(file, group)| {
                let all = group.sizes.len();
                let start = if file == 0 { self.carry.min(all) } else { 0 };
                let end = match self.split {
                    Some(split) if file == last => split.clamp(start, all),
                    _ => all,
                };
                start..end
            })
            .collect()
    }

    /// The matches of `groups` the stream holds.
    pub(super) fn total(self, groups: &[Group]) -> usize {
        self.spans(groups).iter().map(ExactSizeIterator::len).sum()
    }
}

/// The match limit a content page holds (`limit`, else no count bound).
fn page_limit(q: MatchQuery<'_>) -> usize {
    q.limit().map_or(usize::MAX, |limit| limit.max(1))
}

/// What fills a walk-order content window's pages: the cut every page takes
/// ([`stream_page`]). A scan in walk order lists its files in stream order,
/// so the files it evaluated so far say where its pages start: the scan can
/// end at one, and the files past it are another window's
/// ([`octocode_engine::structural::PagedWindow`]).
pub(super) struct PageFill {
    cutter: Cutter,
    /// The matches of the window's first file the window before listed.
    carry: usize,
    /// Each added file's position from the window's first candidate.
    positions: Vec<usize>,
    /// The page starts already reported.
    reported: usize,
}

impl PageFill {
    pub(super) fn of(q: MatchQuery<'_>, budget: usize, carry: usize) -> Self {
        Self {
            cutter: Cutter::new(page_limit(q), budget.max(1)),
            carry,
            positions: Vec::new(),
            reported: 0,
        }
    }

    /// Add the next file with matches, at `position` from the window's first
    /// candidate: its header and its matches' sizes (the window's first file
    /// from its carry on).
    pub(super) fn add(
        &mut self,
        position: usize,
        header: usize,
        sizes: impl IntoIterator<Item = usize>,
    ) {
        let from = if position == 0 { self.carry } else { 0 };
        let file = self.positions.len();
        for (index, size) in sizes.into_iter().enumerate().skip(from) {
            self.cutter.push((file, index), header, size);
        }
        self.positions.push(position);
    }

    /// The page starts past page 1 the files added since the last call hold,
    /// as (position, match index).
    pub(super) fn new_starts(&mut self) -> Vec<(usize, usize)> {
        let starts = self.cutter.pages[self.reported..]
            .iter()
            .map(|(_, (file, index))| (self.positions[*file], *index))
            .collect();
        self.reported = self.cutter.pages.len();
        starts
    }
}

/// Page `q.page()` of the matches `groups` hold within `bounds` (of their
/// files, in the file views): where it starts in the stream, the matches it
/// shows and the stream's total. A page past the last is empty and flagged
/// out of range.
pub(super) fn stream_page(
    q: MatchQuery<'_>,
    groups: &[Group],
    bounds: Bounds,
    budget: usize,
) -> StreamPage {
    let page = q.page().max(1) as usize;
    if super::file_list(q) {
        let paging = octocode_runtime_base::response::page_windows::Paging::of(q.limit(), budget);
        let sizes = groups.iter().map(|group| group.chars).collect::<Vec<_>>();
        let cut = octocode_runtime_base::response::page_windows::select(paging, &sizes, page);
        return StreamPage {
            window: cut.range,
            slices: Vec::new(),
            pages: cut.pages,
            more: cut.more,
            oversized: false,
            pagination: cut.facts.with_unit("files").to_value(),
        };
    }
    let spans = bounds.spans(groups);
    let (cuts, oversized) = cut(groups, &spans, page_limit(q), budget.max(1));
    let pages = cuts.len().max(1);
    let total = spans.iter().map(ExactSizeIterator::len).sum::<usize>();
    let more = page < pages;
    let past_end = page > pages;
    let (window, slices) = cuts.get(page - 1).map_or_else(
        || (groups.len()..groups.len(), Vec::new()),
        |&(from, to)| slices(groups, from, to),
    );
    // The page starts after every stream match of the files before its
    // first and the stream matches of that file before its first shown one.
    let offset = cuts.get(page - 1).map_or(total, |&((file, index), _)| {
        spans[..file]
            .iter()
            .map(ExactSizeIterator::len)
            .sum::<usize>()
            + index.saturating_sub(spans[file].start)
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
mod tests;
