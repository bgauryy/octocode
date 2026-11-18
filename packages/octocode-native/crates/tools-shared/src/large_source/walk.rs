//! The byte walk of a matched line too long for one byte page, in a streamed
//! source: its matches first, as a source held whole walks it
//! ([`LineWalk`]), so a read of a hit deep in a huge one-line file opens at
//! the hit, never at the line's first byte.
//!
//! The match scan ([`super::scan`]) runs over the text a read shows, so it
//! places each such line and its match windows in the shown bytes the line's
//! byte pages count ([`LongHits`]). A page is cut by the walk from the byte its
//! cursor names: the run of the walk there, read as one byte page of the line
//! (sealed per window, as every streamed page is), and, after a match window,
//! the next windows that still fit, each behind a gap marker naming the shown
//! bytes it skips. Where a page starts decides what it is, so the cursor is
//! the line's byte, and the pages are the line exactly once.

use super::held::Want;
use super::scan::Scan;
use super::window::{Opened, read_failed, serve_window, window_span};
use crate::source_view::pagination::{
    LineWalk, gap_marker, hit_page_room, join_window, result_counts,
};
use crate::source_view::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery, ReadProgress, ReadProgressUnit};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_runtime_base::response::cut::escaped_len;
use octocode_runtime_base::security::scan::ContentScan;

/// Shown bytes around a window edge its clean cut reads: far more than the
/// longest redaction marker, so a marker the edge falls inside is whole in them.
pub(super) const CUT_REACH: usize = 1024;

/// A hit line longer than a byte page holds whole, whatever its characters
/// ([`crate::source_view::extraction::FITS_ANY_BYTE_PAGE`]), as the scan
/// found it in the text a read shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LongHit {
    pub(super) line: usize,
    /// The shown byte of the whole source the line starts at.
    pub(super) start: usize,
    /// The line's shown bytes, its line break included; `None` while the scan
    /// has not reached its end.
    pub(super) bytes: Option<usize>,
    /// The line's match windows in shown bytes of the whole source: sorted,
    /// disjoint, every edge a clean cut, near ones joined.
    pub(super) windows: Vec<(usize, usize)>,
}

impl LongHit {
    /// The line's walk, in its own bytes.
    fn walk(&self, bytes: usize) -> LineWalk {
        LineWalk::new(
            0..bytes,
            self.windows
                .iter()
                .map(|&(from, to)| (from - self.start, to - self.start)),
        )
    }
}

/// The long hit lines one scan finds, in line order. Lines that outweigh the
/// memory budget could not be kept, so they are given up: their lines are
/// then walked from their first byte, every byte still once.
#[derive(Default)]
pub(super) struct LongHits {
    lines: Vec<LongHit>,
    weight: usize,
    given_up: bool,
}

impl LongHits {
    pub(super) fn given_up(&self) -> bool {
        self.given_up
    }

    /// The last window of `line`, when it is the line last recorded.
    pub(super) fn last_window(&self, line: usize) -> Option<(usize, usize)> {
        self.lines
            .last()
            .filter(|hit| hit.line == line)
            .and_then(|hit| hit.windows.last().copied())
    }

    /// The line recorded whose end the scan has not reached: its number and
    /// the shown byte it starts at.
    pub(super) fn open(&self) -> Option<(usize, usize)> {
        self.lines
            .last()
            .filter(|hit| hit.bytes.is_none())
            .map(|hit| (hit.line, hit.start))
    }

    /// The open line ends before shown byte `end`.
    pub(super) fn close(&mut self, end: usize) {
        if let Some(hit) = self.lines.last_mut().filter(|hit| hit.bytes.is_none()) {
            hit.bytes = Some(end - hit.start);
        }
    }

    /// Record `window` of `line`, which starts at shown byte `start` and ends
    /// before `end` when the scan has seen its end: a window reaching back
    /// into the line's last one extends it.
    pub(super) fn add(
        &mut self,
        line: usize,
        start: usize,
        end: Option<usize>,
        window: (usize, usize),
    ) {
        if self.given_up {
            return;
        }
        match self.lines.last_mut().filter(|hit| hit.line == line) {
            Some(hit) => {
                hit.bytes = hit.bytes.or(end.map(|end| end - start));
                match hit.windows.last_mut() {
                    Some(last) if window.0 <= last.1 => last.1 = last.1.max(window.1),
                    _ => {
                        let before = hit.windows.len();
                        join_window(&mut hit.windows, window);
                        self.weight += (hit.windows.len() - before) * Self::WINDOW;
                    }
                }
            }
            None => {
                self.lines.push(LongHit {
                    line,
                    start,
                    bytes: end.map(|end| end - start),
                    windows: vec![window],
                });
                self.weight += std::mem::size_of::<LongHit>() + Self::WINDOW;
            }
        }
        if self.weight >= octocode_engine::memory::MEMORY.limit() {
            self.given_up = true;
            self.lines = Vec::new();
        }
    }

    const WINDOW: usize = std::mem::size_of::<(usize, usize)>();

    /// The lines whose end the scan reached: a line it stopped inside (at a
    /// regex limit) has no known end, so it is walked from its first byte.
    pub(super) fn finish(mut self) -> Vec<LongHit> {
        self.lines.retain(|hit| hit.bytes.is_some());
        self.lines
    }

    /// About how many bytes `lines` hold.
    pub(super) fn weight_of(lines: &[LongHit]) -> usize {
        lines
            .iter()
            .map(|hit| std::mem::size_of::<LongHit>() + hit.windows.len() * Self::WINDOW)
            .sum()
    }
}

/// The page's line spans: each of `found`'s windows, with every long hit line
/// a line page cannot hold (over `line_bytes`) on its own span, which its walk
/// pages; the index of that line's [`LongHit`] beside it.
pub(super) fn page_spans(found: &Scan, line_bytes: usize) -> Vec<((usize, usize), Option<usize>)> {
    let mut spans = Vec::with_capacity(found.windows.len());
    for &(first, last) in &found.windows {
        let mut from = first;
        let start = found.long.partition_point(|hit| hit.line < first);
        for (index, hit) in found.long.iter().enumerate().skip(start) {
            if hit.line > last {
                break;
            }
            if hit.bytes.is_none_or(|bytes| bytes <= line_bytes) {
                continue;
            }
            if from < hit.line {
                spans.push(((from, hit.line - 1), None));
            }
            spans.push(((hit.line, hit.line), Some(index)));
            from = hit.line + 1;
        }
        if from <= last {
            spans.push(((from, last), None));
        }
    }
    spans
}

/// The byte page of `line` alone a match read cuts at shown byte `at` of it
/// (`None`: from its first byte), with the read's progress.
fn line_page(
    q: &LocalFetchQuery,
    line: usize,
    at: Option<usize>,
) -> Result<LocalFetchQuery, serde_json::Error> {
    let mut page = local_fetch_query::updated(
        q,
        serde_json::json!({"matchString":null,"regexEngine":null,"caseMode":null,"wholeWord":null,"contextLines":null,"block":null,"readProgress":null}),
    )?;
    local_fetch_query::set_line_span(&mut page, line, line)?;
    local_fetch_query::set_progress(
        &mut page,
        at.map(|offset| ReadProgress {
            unit: ReadProgressUnit::Bytes,
            offset: wire_count(offset),
        }),
    );
    Ok(page)
}

/// Source bytes a packed match window's read holds beyond the window's shown
/// bytes: room for the secrets its placeholders stand for.
const PACK_SLACK: usize = 4 * 1024;

/// The page of `found`'s long hit line `hit` at shown byte `into` of it
/// (`None`: the read reaches the line: its walk's first byte), from `source`,
/// proved by the read's scan. Its offsets are the line's shown bytes, and its
/// placed offset and gap markers the whole source's, as a match read of a
/// source held whole states them; `next.nextPage` reads on from the walk's
/// next byte.
pub(super) fn walk_page(
    q: &LocalFetchQuery,
    source: &Opened<'_>,
    found: &Scan,
    (hit, into): (&LongHit, Option<usize>),
    share: Option<usize>,
    security: &impl ContentScan,
    cancel: &impl CancellationCheck,
) -> LocalFetchResult {
    let path = local_fetch_query::path(q);
    let failed =
        |error: String| LocalFetchResult::error(path.to_owned(), ErrorCode::ExecutionFailed, error);
    let Some(bytes) = hit.bytes else {
        return failed("the long line's end was not scanned".to_owned());
    };
    let walk = hit.walk(bytes);
    let room = hit_page_room(share);
    // A line a byte page holds whole is that page, as a read of a source held
    // whole shows it: a page's room never takes more than three bytes for an
    // escaped unit.
    if into.is_none() && bytes <= 3 * room {
        let whole = match line_page(q, hit.line, None) {
            Ok(page) => super::window::stream_window(&page, source, share, security, cancel),
            Err(error) => return failed(error.to_string()),
        };
        if whole.status == ReadStatus::Error
            || whole
                .next
                .as_ref()
                .is_none_or(|next| next.continue_window.is_none())
        {
            return whole;
        }
    }
    let at = into.unwrap_or_else(|| walk.first());
    let (run_end, after) = walk.run(at);
    // The windows after the one at `at` a page could still show: at most a
    // page's room of them, each at least a third of a unit per byte.
    let held = walk.window_at(at);
    let mut candidates = Vec::new();
    if let Some(index) = held {
        let mut cost = 0usize;
        for &(from, to) in walk.windows().get(index + 1..).unwrap_or_default() {
            cost += (to - from).div_ceil(3) + gap_marker(hit.start + from, hit.start + to).len();
            if cost > room {
                break;
            }
            candidates.push((from, to));
        }
    }
    let wants = std::iter::once(Want {
        first: hit.line,
        last: hit.line,
        from: at,
        room: super::byte_window_bytes(share),
    })
    .chain(candidates.iter().map(|&(from, to)| Want {
        first: hit.line,
        last: hit.line,
        from,
        room: 2 * (to - from) + PACK_SLACK,
    }))
    .collect::<Vec<_>>();
    let mut windows = match source.windows(&wants, cancel) {
        Ok(windows) => windows.into_iter(),
        Err(error) => return read_failed(q, error),
    };
    let Some(mut main) = windows.next() else {
        return failed("no window streamed".to_owned());
    };
    if main.origin.total != bytes {
        // The scan's shown text is not the line's: every page of the read
        // sees as much, and walks the line in order from where it asked.
        return match line_page(q, hit.line, into) {
            Ok(page) => super::window::stream_window(&page, source, share, security, cancel),
            Err(error) => failed(error.to_string()),
        };
    }
    // The page ends where its run does: the line's next bytes are another run.
    if let Some(cut) = run_end.checked_sub(main.origin.base) {
        #[cfg_attr(
            octocode_clippy_ratchet,
            expect(
                clippy::disallowed_methods,
                reason = "Trim an over-read buffer at the current walk run; the next cursor reaches every remaining byte"
            )
        )]
        main.window.truncate(cut);
    }
    let serve_at = |from: usize, window| match line_page(q, hit.line, Some(from)) {
        Ok(page) => {
            let span = window_span(&page);
            serve_window(&page, window, span, source, share, security, cancel)
        }
        Err(error) => failed(error.to_string()),
    };
    let mut result = serve_at(at, main);
    if result.status == ReadStatus::Error {
        return result;
    }
    let Some(page) = result.pagination.clone() else {
        return failed("a byte page without its place".to_owned());
    };
    let end = page.offset + page.length;
    let mut text = result.content.take().unwrap_or_default();
    let mut shown = page.length;
    let mut next = if end < run_end { Some(end) } else { after };
    if end == run_end
        && let Some(index) = held
    {
        // A page that ends a match window goes on with the windows after it
        // that its room still holds whole, as a source held whole pages them.
        let mut left = room.saturating_sub(escaped_len(&text));
        let (mut last, mut taken) = (end, index + 1);
        for (&(from, to), mut window) in candidates.iter().zip(windows) {
            if window.origin.base != from || window.window.len() < to - from {
                break;
            }
            #[cfg_attr(
                octocode_clippy_ratchet,
                expect(
                    clippy::disallowed_methods,
                    reason = "Trim over-read bytes to this match window; LineWalk and its continuation retain every other window and gap"
                )
            )]
            window.window.truncate(to - from);
            let part = serve_at(from, window);
            if part.status == ReadStatus::Error
                || part.pagination.as_ref().map(|page| page.length) != Some(to - from)
            {
                break;
            }
            let marker = gap_marker(hit.start + last, hit.start + from);
            let shown_window = part.content.as_deref().unwrap_or_default();
            let cost = escaped_len(&marker) + escaped_len(shown_window);
            if cost > left {
                break;
            }
            left -= cost;
            text.push_str(&marker);
            text.push_str(shown_window);
            shown += to - from;
            (last, taken) = (to, taken + 1);
            for warning in part.warnings {
                super::page::join_warning(&mut result.warnings, warning);
            }
        }
        next = walk.after_windows(taken);
    }
    let (_, returned_bytes, returned_lines) = result_counts(&text);
    result.content = Some(text);
    result.returned_bytes = Some(returned_bytes);
    result.returned_lines = Some(returned_lines);
    result.pagination = Some(Pagination {
        unit: ReadProgressUnit::Bytes,
        offset: at,
        length: shown,
        window: room,
        total_lines: 1,
        total_bytes: bytes,
        has_more: next.is_some(),
        next_offset: next,
        resume_line: None,
        placed: Some((hit.start + at, found.shown_bytes)),
    });
    let rest = match next
        .map(|offset| line_page(q, hit.line, Some(offset)))
        .transpose()
    {
        Ok(rest) => rest,
        Err(error) => return failed(error.to_string()),
    };
    let continuation = match rest {
        Some(mut rest) => {
            if let Err(error) =
                local_fetch_query::set_snapshot(&mut rest, result.source_sha256.as_deref())
            {
                return failed(error.to_string());
            }
            Some(FetchCall::page(rest))
        }
        None => None,
    };
    if continuation.is_none() && result.partial_reasons.is_empty() {
        result.is_partial = None;
    }
    result
        .next
        .get_or_insert_with(NextCalls::default)
        .continue_window = continuation;
    result
}
