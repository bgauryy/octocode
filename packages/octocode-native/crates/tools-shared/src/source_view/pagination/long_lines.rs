//! Lines too long for one byte page, in a raw view held whole in memory:
//! the byte walk that shows a line's matches first, and the line page that
//! fills up with the first bytes of the long line it stops at.
use super::{LineBudget, Page};
use crate::source_view::extraction::{LineOffsets, OMISSION_LINE, line_count};
use crate::source_view::types::Pagination;
use octocode_contract_types::ReadProgressUnit;

/// How a raw view held whole in memory pages a line too long for one byte
/// page: the view bytes around each match in such a line (sorted, disjoint,
/// each inside one line, every edge a clean cut), which its byte walk shows
/// before the rest of the line ([`MatchFirst`]). A line page that stops at
/// such a line with room left takes the line's first bytes, unless matches
/// lead its walk.
#[derive(Clone, Copy)]
pub(in crate::source_view) struct LongLines<'a> {
    pub(in crate::source_view) match_windows: &'a [(usize, usize)],
    /// Where the view's bytes sit in its source, for a selection of it: the
    /// byte spans a page names are the source's.
    pub(in crate::source_view) placement: Option<&'a Placement<'a>>,
}

impl<'a> LongLines<'a> {
    /// The policy of a raw view of a source held whole (a streamed window's
    /// line may reach past the window, and a transformed view is not the
    /// source's bytes).
    pub(in crate::source_view) fn of(
        ext: &'a crate::source_view::extraction::Extraction,
        placement: Option<&'a Placement<'a>>,
    ) -> Self {
        Self {
            match_windows: &ext.match_windows,
            placement,
        }
    }
}

/// Where the bytes of a selection view (the lines of a match or range read,
/// joined with omission markers) sit in the source they were selected from,
/// so a byte page of it names source bytes as a plain read of the same file
/// does: one file, one byte count.
pub(in crate::source_view) struct Placement<'a> {
    view: LineOffsets,
    /// Each view line's 1-based source line ([`OMISSION_LINE`]: a marker).
    selected: &'a [usize],
    source: &'a LineOffsets,
    /// The source's size in bytes.
    pub(in crate::source_view) total: usize,
}

impl<'a> Placement<'a> {
    pub(in crate::source_view) fn new(
        view: &str,
        selected: &'a [usize],
        source: &'a LineOffsets,
        total: usize,
    ) -> Self {
        Self {
            view: LineOffsets::new(view),
            selected,
            source,
            total,
        }
    }

    /// The source byte view byte `at` is; `None` inside an omission marker.
    pub(in crate::source_view) fn source_byte(&self, at: usize) -> Option<usize> {
        let line = self.view.line_at(at);
        let source_line = *self.selected.get(line)?;
        (source_line != OMISSION_LINE)
            .then(|| self.source.start(source_line - 1) + (at - self.view.start(line)))
    }
}

/// The byte walk of one line too long for one byte page that holds matches:
/// each match's window first, in source order, then every other byte of the
/// line, in source order. A page states its offset: a page of the rest is one
/// contiguous run of the line, and a page of windows is a run that may go on
/// with the next windows whole, each behind a [`gap_marker`] naming the bytes
/// it skips, so the pages are the line exactly once and the reader places
/// each. Where a page starts decides what it is, so any byte of the line
/// continues the same walk.
///
/// Windows closer than the row around a page
/// ([`octocode_runtime_base::response::page_windows::ROW_SKELETON_CHARS`]) are one run: the
/// bytes between them cost less shown with them than marked and paged apart.
/// A line dense with matches is therefore walked in full pages from its
/// first match, and a line with a few shows them on one page.
pub(super) struct MatchFirst {
    /// The line's bytes in the view, its line break included, and its match
    /// windows.
    walk: LineWalk,
    /// The source byte the line starts at, for a selection view: the gap
    /// markers name source bytes, as the page's placed offset does.
    source_start: Option<usize>,
}

/// The order of [`MatchFirst`]'s byte walk of one line, as byte positions
/// alone: each match window, in source order, then every other byte of the
/// line, in source order. A source held whole walks its view's bytes; a
/// streamed one (`tools::large_source`) walks the line's own bytes with the
/// same steps, so both serve one contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LineWalk {
    /// The line's bytes, its line break included.
    line: std::ops::Range<usize>,
    /// The line's match windows: sorted, disjoint, inside the line, every
    /// edge a clean cut, near ones joined ([`join_window`]).
    windows: Vec<(usize, usize)>,
}

/// Add `window`, which starts at or after the last of `windows`, joined with
/// that last one when closer than the row around a page
/// ([`octocode_runtime_base::response::page_windows::ROW_SKELETON_CHARS`]): the bytes between
/// them cost less shown with them than marked and paged apart.
pub(crate) fn join_window(windows: &mut Vec<(usize, usize)>, (from, to): (usize, usize)) {
    match windows.last_mut() {
        Some(before)
            if from.saturating_sub(before.1)
                < octocode_runtime_base::response::page_windows::ROW_SKELETON_CHARS =>
        {
            before.1 = before.1.max(to);
        }
        _ => windows.push((from, to)),
    }
}

impl LineWalk {
    /// The walk of the bytes `line` with the match `windows` (sorted,
    /// disjoint, inside the line, clean cuts), near ones joined.
    pub(crate) fn new(
        line: std::ops::Range<usize>,
        windows: impl IntoIterator<Item = (usize, usize)>,
    ) -> Self {
        let mut joined = Vec::new();
        for window in windows {
            join_window(&mut joined, window);
        }
        Self {
            line,
            windows: joined,
        }
    }

    /// The line's match windows, near ones joined.
    pub(crate) fn windows(&self) -> &[(usize, usize)] {
        &self.windows
    }

    /// The first byte the walk shows: its first match window's.
    pub(crate) fn first(&self) -> usize {
        self.windows
            .first()
            .map_or(self.line.start, |window| window.0)
    }

    /// The match window holding byte `at`, by its index.
    pub(crate) fn window_at(&self, at: usize) -> Option<usize> {
        let index = self.windows.partition_point(|window| window.1 <= at);
        self.windows
            .get(index)
            .is_some_and(|window| window.0 <= at)
            .then_some(index)
    }

    /// The byte the walk shows after the windows before index `taken`: the
    /// next window's first, else the first byte no window holds; `None` when
    /// the line has none left.
    pub(crate) fn after_windows(&self, taken: usize) -> Option<usize> {
        self.windows
            .get(taken)
            .map(|window| window.0)
            .or_else(|| self.rest_from(self.line.start))
    }

    /// The first byte at or after `at` that no window holds; `None` when the
    /// line has none left.
    fn rest_from(&self, mut at: usize) -> Option<usize> {
        let from = self.windows.partition_point(|window| window.1 <= at);
        for window in self.windows.get(from..).unwrap_or_default() {
            if window.0 > at {
                break;
            }
            at = window.1;
        }
        (at < self.line.end).then_some(at)
    }

    /// Where the run of the walk holding byte `at` ends (a match window, or
    /// the bytes between two), and the byte the walk shows after it (`None`:
    /// the line is done).
    pub(crate) fn run(&self, at: usize) -> (usize, Option<usize>) {
        let next = self.windows.partition_point(|window| window.1 <= at);
        match self.windows.get(next) {
            Some(&(start, end)) if start <= at => (end, self.after_windows(next + 1)),
            later => {
                let end = later.map_or(self.line.end, |window| window.0);
                (end, self.rest_from(end))
            }
        }
    }
}

impl MatchFirst {
    /// The walk of the line holding byte `at`: `None` when no match window
    /// sits in it, or the whole line fits `room` (one byte page shows it).
    pub(super) fn of(
        content: &str,
        long_lines: Option<LongLines<'_>>,
        at: usize,
        room: usize,
    ) -> Option<Self> {
        let long_lines = long_lines?;
        let all = long_lines.match_windows;
        let bytes = content.as_bytes();
        let start = memchr::memrchr(b'\n', bytes.get(..at)?).map_or(0, |at| at + 1);
        let end = memchr::memchr(b'\n', bytes.get(at..)?).map_or(bytes.len(), |end| at + end + 1);
        let from = all.partition_point(|window| window.1 <= start);
        let to = all.partition_point(|window| window.0 < end);
        let own = all.get(from..to).filter(|windows| !windows.is_empty())?;
        if octocode_runtime_base::response::cut::window_end(content, start, room) >= end {
            return None;
        }
        Some(Self {
            walk: LineWalk::new(start..end, own.iter().copied()),
            source_start: long_lines
                .placement
                .and_then(|placement| placement.source_byte(start)),
        })
    }

    /// View byte `at` of the line as the bytes a page names: the source's
    /// for a selection view, else the view's.
    fn named(&self, at: usize) -> usize {
        self.source_start
            .map_or(at, |start| start + (at - self.walk.line.start))
    }

    /// The first byte the walk shows: its first match window's.
    pub(super) fn first(&self) -> usize {
        self.walk.first()
    }

    /// The byte page at `at`: to the end of its run, or as far as `room`
    /// holds. A page that ends a match window goes on with the windows after
    /// it that `room` still holds whole, each behind a [`gap_marker`] naming
    /// the bytes it skips (they come with the rest of the line), so one page
    /// shows as many hits as fit; its length counts the bytes it shows. After
    /// the line's last byte, or after a first page that holds every window of
    /// the line (the rest is the line's optional read), line paging resumes.
    pub(super) fn page(&self, content: &str, at: usize, room: usize) -> Page {
        let (run_end, after) = self.walk.run(at);
        let end = octocode_runtime_base::response::cut::window_end(content, at, room).min(run_end);
        // `at` and `end` are character boundaries (`window_end` walks chars).
        let mut text = content.get(at..end).unwrap_or_default().to_owned();
        let mut next = if end < run_end { Some(end) } else { after };
        let mut last = end;
        // The line's bytes the page shows: its runs, not the gaps between
        // them, which later pages show.
        let mut shown = end - at;
        let windows = self.walk.windows();
        // The windows after the first one this page holds, each with the
        // bytes it skips to reach it.
        let mut joined: Vec<((usize, usize), &str)> = Vec::new();
        let mut every_window = false;
        if end == run_end
            && let Some(held) = self.walk.window_at(at)
        {
            let mut left =
                room.saturating_sub(octocode_runtime_base::response::cut::escaped_len(&text));
            let mut taken = held + 1;
            for &(from, to) in windows.get(taken..).unwrap_or_default() {
                let marker = gap_marker(self.named(last), self.named(from));
                let window = content.get(from..to).unwrap_or_default();
                let cost = octocode_runtime_base::response::cut::escaped_len(&marker)
                    + octocode_runtime_base::response::cut::escaped_len(window);
                if cost > left {
                    break;
                }
                left -= cost;
                joined.push(((self.named(last), self.named(from)), window));
                shown += to - from;
                (last, taken) = (to, taken + 1);
            }
            next = self.walk.after_windows(taken);
            every_window = held == 0 && taken == windows.len();
        }
        // The first page of the walk holds every match window of the line:
        // the rest of that line answers nothing asked, so it is an optional
        // read of the whole line, not pages every reader must take before the
        // view's next lines (which hold the next matches).
        let rest_of_line = every_window && at == self.walk.first() && next.is_some();
        for ((from, to), window) in joined {
            if rest_of_line {
                text.push_str(&unshown_marker(from, to));
            } else {
                text.push_str(&gap_marker(from, to));
            }
            text.push_str(window);
        }
        if rest_of_line {
            next = None;
        }
        let line = octocode_engine::text::line_containing(content, at);
        let (next_offset, resume_line) = match next {
            Some(byte) => (Some(byte), None),
            None if self.walk.line.end < content.len() => (Some(self.walk.line.end), Some(line)),
            None => (None, None),
        };
        Page {
            text,
            pagination: Pagination {
                unit: ReadProgressUnit::Bytes,
                offset: at,
                length: shown,
                window: room,
                total_lines: line_count(content),
                total_bytes: content.len(),
                has_more: next_offset.is_some(),
                next_offset,
                resume_line,
                placed: None,
            },
            view_lines: (line, line),
            out_of_range: false,
            rest_of_line,
            opens_inside: at > self.walk.line.start,
        }
    }
}

/// The marker between two match windows a byte page shows together: the
/// bytes `start..end` between them (the source's, for a selection view; as
/// the page's offset names them), which the line's later pages show.
/// It sits on a line of its own, so the windows around it stay verbatim.
pub(crate) fn gap_marker(start: usize, end: usize) -> String {
    format!("\n... [bytes {start}-{end} on later pages] ...\n")
}

/// [`gap_marker`] on a page whose line's rest is an optional read: the bytes
/// between two windows are not on a later page of this read. Never longer
/// than the gap marker the page was sized with.
fn unshown_marker(start: usize, end: usize) -> String {
    format!("\n... [bytes {start}-{end} not shown] ...\n")
}

/// A byte page at view byte `at` as the view bytes it shows: its text split
/// at each [`gap_marker`], each piece at the view byte it starts at.
#[cfg(any(test, feature = "test-support"))]
#[expect(
    clippy::expect_used,
    reason = "fixtures for tests, compiled only for them (`test`, `test-support`): a broken fixture fails its test at once"
)]
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::string_slice,
        reason = "a test fixture slicing at the ASCII marker it found"
    )
)]
pub fn placed_pieces(at: usize, text: &str) -> Vec<(usize, &str)> {
    const OPEN: &str = "\n... [bytes ";
    const CLOSES: [&str; 2] = [" on later pages] ...\n", " not shown] ...\n"];
    let (mut pieces, mut place, mut rest) = (Vec::new(), at, text);
    while let Some(open) = rest.find(OPEN) {
        let after = &rest[open + OPEN.len()..];
        let (close, closer) = CLOSES
            .iter()
            .filter_map(|closer| after.find(closer).map(|close| (close, closer.len())))
            .min()
            .expect("a whole marker");
        let (from, to) = after[..close].split_once('-').expect("a byte span");
        let from: usize = from.parse().expect("a byte");
        assert_eq!(place + open, from, "the marker starts where its piece ends");
        pieces.push((place, &rest[..open]));
        place = to.parse().expect("a byte");
        rest = &after[close + closer..];
    }
    pieces.push((place, rest));
    pieces
}

/// A line page that stopped at a line too long for it (`long`, the byte that
/// line starts at) with room left: the byte page of the same lines and the
/// long line's first bytes, so the page is full and the byte walk of that
/// line goes on from its end. `None` when the lines fill a byte page, the
/// rest of the room holds the whole line (it is the next line page), or
/// matches lead the line's walk.
pub(super) fn filled_page(
    content: &str,
    lines: std::ops::Range<usize>,
    (first, taken): (usize, usize),
    budget: LineBudget,
    long_lines: Option<LongLines<'_>>,
) -> Option<Page> {
    long_lines?;
    let room = budget.line.max(super::MIN_BYTE_PAGE);
    // Each line the page shows, and the long line it ends with, renders
    // behind its gutter.
    let gutters = (taken + 1).saturating_mul(budget.gutter);
    let left = room.checked_sub(
        octocode_runtime_base::response::cut::escaped_len(content.get(lines.clone())?) + gutters,
    )?;
    if left == 0 || MatchFirst::of(content, long_lines, lines.end, room).is_some() {
        return None;
    }
    let end = octocode_runtime_base::response::cut::window_end(content, lines.end, left);
    // A window that ends on a line break holds the whole long line.
    if end >= content.len() || content.as_bytes().get(end.checked_sub(1)?) == Some(&b'\n') {
        return None;
    }
    let text = content.get(lines.start..end)?.to_owned();
    Some(Page {
        text,
        pagination: Pagination {
            unit: ReadProgressUnit::Bytes,
            offset: lines.start,
            length: end - lines.start,
            window: room,
            total_lines: line_count(content),
            total_bytes: content.len(),
            has_more: true,
            next_offset: Some(end),
            resume_line: None,
            placed: None,
        },
        view_lines: (first + 1, first + taken + 1),
        out_of_range: false,
        rest_of_line: false,
        opens_inside: false,
    })
}

/// Where a byte page that reaches past the end of an oversized line (one
/// over the line page's `line_page_bytes`, the only reason a line read
/// pages by bytes) stops instead: that line's end, and the 0-based line line
/// paging resumes at. `None` when the line fits a line page, the page ends
/// inside it, or it is the view's last line. `edges` places `content` in the
/// view it was cut from: a line that starts before it counts those bytes too.
pub(super) fn oversized_line_end(
    content: &str,
    line: usize,
    offset: usize,
    end: usize,
    line_page_bytes: usize,
    edges: super::Edges,
) -> Option<(usize, usize)> {
    let bytes = content.as_bytes();
    // Only a line that ends within the page matters, and only its length
    // against the line page: the scans stop at the page's end and, behind
    // the offset, a line page's length. A line start further back than that
    // makes the line oversized whatever its exact start, so it is not found.
    let behind = offset.saturating_sub(line_page_bytes);
    let before = match bytes[behind..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
    {
        Some(at) => offset - (behind + at + 1),
        // The text's first line: it started `lead` bytes before the text.
        None if behind == 0 => offset + edges.lead,
        None => offset - behind,
    };
    let line_end = bytes[offset..end.min(bytes.len())]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|at| offset + at + 1)?;
    // The 1-based line at `offset` is also the 0-based index of the next.
    (before + (line_end - offset) > line_page_bytes && (line_end < bytes.len() || edges.more))
        .then_some((line_end, line))
}
