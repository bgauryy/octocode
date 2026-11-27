//! The `grep_searcher` sink that collects one file's matches, spans and contexts.

use super::mode::MatchWork;
use super::state::{Charge, Held, context_weight, match_weight, raw_weight};
use crate::num::saturating_u32;
use crate::search::declaration;
use crate::search::text_parser::{FileEntry, RawMatch, strip_trailing_newline};
use crate::text::interior_offset;
use crate::text::utf8_offsets::byte_to_utf16_offset;
use crate::types::TextSearchMatch;
use grep_matcher::Matcher;
use grep_searcher::{Searcher, Sink, SinkContext, SinkMatch};
use std::sync::atomic::{AtomicBool, Ordering};

mod retained;
pub(super) use retained::keep_span;

mod columns;
mod long_line;
mod row_window;
#[cfg(test)]
pub(super) use columns::stepped;
use columns::{SpanColumns, block_text, span_value};

/// Whether `found`, a match in a matched block's `bytes`, is an empty match
/// past the block's last line break: the start of a line the block does not
/// hold, so no match of the block (rg's rule: a line's matches end with the
/// line, an empty one at its end included).
fn past_block_end(bytes: &[u8], found: grep_matcher::Match) -> bool {
    found.is_empty() && bytes.ends_with(b"\n") && found.start() >= bytes.len()
}

/// One row's part of a matched block: its whole lines (`bytes`, a range of
/// the block), its first line's index in the block, and the block bytes
/// its first match covers.
struct LineGroup {
    bytes: std::ops::Range<usize>,
    first_line: u32,
    first_match: Option<(usize, usize)>,
}

/// A matched block's rows. Multiline mode joins matches on adjacent lines
/// into one block; a row is one match, or the matches that share a line (as
/// a line holding several hits is one row), each with its own lines, so a
/// match's extent is never the block's. A block with no enumerated match is
/// one row.
fn line_groups(bytes: &[u8], spans: &[(usize, usize)]) -> Vec<LineGroup> {
    // Each line's first byte in the block.
    let starts = std::iter::once(0)
        .chain(memchr::memchr_iter(b'\n', bytes).map(|at| at + 1))
        .filter(|&at| at < bytes.len())
        .collect::<Vec<_>>();
    let line_at = |at: usize| {
        starts
            .partition_point(|&start| start <= at)
            .saturating_sub(1)
    };
    let end_of = |line: usize| starts.get(line + 1).copied().unwrap_or(bytes.len());
    let mut groups: Vec<(usize, usize, (usize, usize))> = Vec::new();
    for &(start, end) in spans {
        // A match ending in its line's newline ends on that line.
        let (first, last) = (line_at(start), line_at(end.saturating_sub(1).max(start)));
        match groups.last_mut() {
            Some(group) if first <= group.1 => group.1 = group.1.max(last),
            _ => groups.push((first, last, (start, end))),
        }
    }
    if groups.is_empty() {
        return vec![LineGroup {
            bytes: 0..bytes.len(),
            first_line: 0,
            first_match: None,
        }];
    }
    groups
        .into_iter()
        .map(|(first, last, matched)| LineGroup {
            bytes: starts[first]..end_of(last),
            first_line: saturating_u32(first),
            first_match: Some(matched),
        })
        .collect()
}

/// `grep_searcher::Sink` that accumulates matches/contexts for one file and,
/// using the matcher, derives the 0-based UTF-16 column of the first submatch.
pub(super) struct CollectSink<'a, M: Matcher> {
    pub(super) matcher: &'a M,
    pub(super) entry: FileEntry,
    pub(super) submatches: u32,
    pub(super) matched_lines: u32,
    pub(super) work: MatchWork,
    /// Accumulated only-matching spans for this file: every one, as line
    /// mode keeps every matched line, so the result's pages reach each.
    pub(super) om_matches: Vec<TextSearchMatch>,
    /// Search-wide stop flag, polled between matched lines.
    pub(super) stop: &'a AtomicBool,
    /// The caller's cancellation, where it stops files mid-way: polled while
    /// a line's matches are walked and every few matched lines, so a file
    /// is not searched to its end after the caller left.
    pub(super) cancelled: Option<&'a (dyn Fn() -> bool + Sync)>,
    /// Matched lines seen, to poll `cancelled` every few of them.
    ticks: u32,
    /// What this file's kept rows hold of the search's bound; past it the
    /// sink raises `stop` and keeps nothing more.
    pub(super) charge: Charge,
    /// Absolute offset of the first NUL byte when the file was quit as binary.
    /// The searcher stops there, so bytes after it are unsearched.
    pub(super) binary_offset: Option<u64>,
    /// A matched or context line was not UTF-8, so its value is decoded
    /// text, not verbatim bytes ([`block_text`]).
    pub(super) not_utf8: bool,
    /// One matcher per searched term (empty: none counted).
    pub(super) terms: &'a [M],
    /// Per term of `terms`: its matches (when submatches are enumerated)
    /// or the matched lines holding it.
    pub(super) term_counts: Vec<u32>,
    /// The search's test-code scope and this file's path: matched lines
    /// on the other side are left out.
    pub(super) test_code: Option<(&'a crate::types::TestCodeScope, &'a std::path::Path)>,
    /// The search's line scope and this file's path: matched lines outside
    /// its spans are left out.
    pub(super) line_scope: Option<(&'a crate::types::LineScope, &'a std::path::Path)>,
    /// This file's kept spans, read on its first matched line.
    pub(super) scope_spans: Option<std::sync::Arc<Vec<[u32; 2]>>>,
    /// The search's deferred-line inspector and this file's path.
    pub(super) inspect_deferred: Option<(&'a crate::types::DeferredInspector, &'a std::path::Path)>,
    /// This file's test-only spans, read on its first matched line.
    pub(super) test_spans: Option<std::sync::Arc<Vec<[u32; 2]>>>,
    /// Matched lines the scope left out.
    pub(super) test_lines_excluded: u32,
    /// This file's path, which says what a long line's secrets are.
    pub(super) source: Option<&'a std::path::Path>,
    /// The ranges of the searched text its judge wrote
    /// ([`crate::types::ServedText::written`]): a match taking any of their
    /// bytes is no match (empty for a file searched as its own bytes).
    pub(super) written: &'a [std::ops::Range<usize>],
    /// An inverted search of a judged text ran over the text with `written`
    /// made filler, so a line whose only match is a placeholder's spelling
    /// is no match: the whole text as served, from which each line reads.
    pub(super) inverted_over: Option<&'a [u8]>,
}

impl<'a, M: Matcher> CollectSink<'a, M> {
    /// The line the searcher read at `at`, as the text was served: an
    /// inverted search read it with the judge's writes made filler.
    fn served_line<'b>(&self, line: &'b [u8], at: usize) -> &'b [u8]
    where
        'a: 'b,
    {
        self.inverted_over
            .and_then(|served| served.get(at..at.checked_add(line.len())?))
            .unwrap_or(line)
    }

    pub(super) fn new(
        matcher: &'a M,
        work: MatchWork,
        (stop, held): (&'a AtomicBool, &std::sync::Arc<Held>),
        terms: &'a [M],
    ) -> Self {
        Self {
            charge: Charge::new(held),
            terms,
            term_counts: vec![0; terms.len()],
            test_code: None,
            line_scope: None,
            scope_spans: None,
            inspect_deferred: None,
            test_spans: None,
            test_lines_excluded: 0,
            source: None,
            written: &[],
            inverted_over: None,
            matcher,
            entry: FileEntry::new(),
            submatches: 0,
            matched_lines: 0,
            work,
            om_matches: Vec::new(),
            stop,
            cancelled: None,
            ticks: 0,
            binary_offset: None,
            not_utf8: false,
        }
    }
}

impl<M: Matcher> CollectSink<'_, M> {
    /// One span per submatch of a matched block, each with its own UTF-16
    /// column, rather than one whole-line match: how many it holds.
    fn collect_spans(
        &mut self,
        bytes: &[u8],
        line_number: u32,
        (first_start, hidden): (&mut Option<usize>, &[std::ops::Range<usize>]),
    ) -> std::io::Result<u32> {
        let mut count = 0u32;
        let (line_text, not_utf8) = block_text(bytes);
        self.not_utf8 |= not_utf8;
        // find_iter yields non-overlapping matches L→R.
        let (matcher, stop, cancelled) = (self.matcher, self.stop, self.cancelled);
        let (om, charge) = (&mut self.om_matches, &mut self.charge);
        let mut columns = SpanColumns::new(bytes, &line_text);
        super::written::each_own(matcher, bytes, hidden, |m| {
            if past_block_end(bytes, m) {
                return true;
            }
            count = count.saturating_add(1);
            if !keep_going(cancelled, stop, count) {
                return false;
            }
            first_start.get_or_insert(m.start());
            let (start, end) = (columns.lossy(m.start()), columns.lossy(m.end()));
            let value = span_value(&line_text, start, end);
            let (line_delta, column, byte_offset) = columns.position(start);
            let match_len = saturating_u32(end.saturating_sub(start));
            let line = line_number.saturating_add(line_delta);
            let lines = saturating_u32(
                value
                    .as_bytes()
                    .split_inclusive(|&b| b == b'\n')
                    .count()
                    .max(1),
            );
            let span = TextSearchMatch {
                deferred: None,
                line,
                first_line: line,
                last_line: (lines > 1).then(|| line.saturating_add(lines - 1)),
                column,
                byte_offset,
                match_len,
                value_digest: TextSearchMatch::digest_of(&value),
                value,
                count: None,
                rank: None,
                window_start: None,
            };
            keep_span((om, charge, stop), span)
        })
        .map_err(|error| std::io::Error::other(error.to_string()))?;
        // A matched line with no enumerable submatch (e.g. zero-width or
        // multiline block) still yields one span: the whole line.
        if count == 0 {
            count = 1;
            let line_text = strip_trailing_newline(line_text);
            let span = TextSearchMatch {
                deferred: None,
                line: line_number,
                first_line: line_number,
                last_line: None,
                column: 0,
                byte_offset: 0,
                match_len: 0,
                value_digest: TextSearchMatch::digest_of(&line_text),
                value: line_text,
                count: None,
                rank: None,
                window_start: None,
            };
            keep_span((&mut self.om_matches, &mut self.charge, self.stop), span);
        }
        Ok(count)
    }

    /// The row of `bytes`, whole lines from `line_number` holding one group
    /// of matches ([`line_groups`]), its first hit covering `first_match`.
    fn line_row(&mut self, bytes: &[u8], line_number: u32, first_match: Option<(usize, usize)>) {
        let (line_text, not_utf8) = block_text(bytes);
        self.not_utf8 |= not_utf8;
        let line_text = strip_trailing_newline(line_text);
        let (start, end) = first_match.unwrap_or((0, 0));
        let byte = interior_offset(bytes, start).min(line_text.len());
        let column = saturating_u32(byte_to_utf16_offset(&line_text, byte));
        let byte_offset = saturating_u32(byte);
        let match_len = saturating_u32(interior_offset(bytes, end).saturating_sub(byte));
        if self.work.deferred_line {
            let mut hit = crate::search::text_parser::line_match(
                &line_text,
                line_number,
                column,
                (byte_offset, match_len),
            );
            if let Some((inspector, path)) = self.inspect_deferred {
                (inspector.0)(path, &mut hit);
            } else {
                hit.value = String::new();
                hit.deferred = Some(0);
            }
            self.entry.prepared.push(hit);
        } else {
            self.entry.raw_matches.push(RawMatch {
                line_text,
                line_number,
                column,
                byte_offset,
                match_len,
                rank: declaration::line_rank(bytes, start),
                window_start: None,
            });
        }
    }

    /// Hold `bytes` more of this file's rows; past the search's bound the
    /// whole search stops (and is refused), so nothing more is kept.
    fn hold(&mut self, bytes: usize) -> bool {
        let fits = self.charge.add(bytes);
        if !fits {
            self.stop.store(true, Ordering::Relaxed);
        }
        fits
    }

    /// Whether the line scope and the test-code scope keep the matched block
    /// at 1-based `line`; one the test-code scope leaves out is counted.
    /// Each scope's spans are read on the file's first matched line, so a
    /// file without hits is never parsed.
    fn keeps_line(&mut self, line: u32, bytes: &[u8]) -> bool {
        if let Some((scope, path)) = self.line_scope {
            let spans = self.scope_spans.get_or_insert_with(|| (scope.spans)(path));
            if !crate::types::LineScope::keeps(spans, line) {
                return false;
            }
        }
        let Some((scope, path)) = self.test_code else {
            return true;
        };
        // A test path is test code whole; any other file's spans say.
        let spans = self.test_spans.get_or_insert_with(|| {
            if (scope.test_path)(path) {
                std::sync::Arc::new(vec![crate::types::WHOLE_FILE])
            } else {
                (scope.spans)(path)
            }
        });
        let test = spans
            .iter()
            .any(|[start, end]| (*start..=*end).contains(&line));
        if test == scope.keep_test {
            return true;
        }
        let lines = saturating_u32(bytes.split_inclusive(|&b| b == b'\n').count().max(1));
        self.test_lines_excluded = self.test_lines_excluded.saturating_add(lines);
        false
    }

    /// One matched block's per-term counts, each term counted as a search of
    /// it alone counts it (`rg -o` per term): its own matches in the block,
    /// so a term found only inside another term's match still counts and the
    /// counts may sum past the block's matches. Count-lines counts lines
    /// ([`MatchWork::terms_by_line`]): each term the block holds counts the
    /// block's lines. Every line holding a term's match is a matched line, so
    /// the matched blocks hold every match of every term.
    fn count_terms(
        &mut self,
        bytes: &[u8],
        hidden: &[std::ops::Range<usize>],
    ) -> std::io::Result<()> {
        let failed = |error: M::Error| std::io::Error::other(error.to_string());
        let terms = self.terms;
        if self.work.terms_by_line {
            let lines = saturating_u32(bytes.split_inclusive(|&b| b == b'\n').count().max(1));
            for (index, term) in terms.iter().enumerate() {
                let holds = if hidden.is_empty() {
                    term.is_match(bytes).map_err(failed)?
                } else {
                    own_match(term, bytes, hidden)?.unwrap_or(false)
                };
                if holds {
                    self.term_counts[index] = self.term_counts[index].saturating_add(lines);
                }
            }
            return Ok(());
        }
        // A block's terms count whole, page cut or not; only the caller's
        // cancellation ends a long line's count early.
        let (stop, cancelled) = (self.stop, self.cancelled);
        for (index, term) in terms.iter().enumerate() {
            let mut found: u32 = 0;
            super::written::each_own(term, bytes, hidden, |matched| {
                if past_block_end(bytes, matched) {
                    return true;
                }
                found = found.saturating_add(1);
                if found.is_multiple_of(MATCHES_PER_POLL) && cancelled.is_some_and(|gone| gone()) {
                    stop.store(true, Ordering::Relaxed);
                    return false;
                }
                true
            })
            .map_err(failed)?;
            self.term_counts[index] = self.term_counts[index].saturating_add(found);
        }
        Ok(())
    }
}

/// Matches walked between two looks at the caller's cancellation.
const MATCHES_PER_POLL: u32 = 1024;
/// Matched lines between two looks at it.
const MATCHED_LINES_PER_POLL: u32 = 64;

/// Whether `matcher` matches `bytes`, a matched block, with text the judge
/// wrote there left out (`hidden`, ranges of the block): `Some(true)` when a
/// match takes none of it, `Some(false)` when every match takes some, `None`
/// when no match of the block alone is enumerable (a block the searcher
/// matched across lines), which is kept as the searcher matched it.
fn own_match<M: Matcher>(
    matcher: &M,
    bytes: &[u8],
    hidden: &[std::ops::Range<usize>],
) -> std::io::Result<Option<bool>> {
    let failed = |error: M::Error| std::io::Error::other(error.to_string());
    let mut any = false;
    matcher
        .find_iter(bytes, |matched| {
            any = !past_block_end(bytes, matched);
            !any
        })
        .map_err(failed)?;
    if !any {
        return Ok(None);
    }
    let mut own = false;
    super::written::each_own(matcher, bytes, hidden, |matched| {
        own = !past_block_end(bytes, matched);
        !own
    })
    .map_err(failed)?;
    Ok(Some(own))
}

/// Whether the walk of a line's matches goes on: it ends once the search was
/// stopped, or the caller cancelled (which stops the search).
pub(super) fn keep_going(
    cancelled: Option<&(dyn Fn() -> bool + Sync)>,
    stop: &AtomicBool,
    matches: u32,
) -> bool {
    if matches.is_multiple_of(MATCHES_PER_POLL) && cancelled.is_some_and(|gone| gone()) {
        stop.store(true, Ordering::Relaxed);
    }
    !stop.load(Ordering::Relaxed)
}

impl<M: Matcher> Sink for CollectSink<'_, M> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> std::io::Result<bool> {
        // Stop searching this file *before* doing more per-line work. Returning
        // Ok(false) ends the search cleanly and keeps the matches collected so far.
        self.ticks = self.ticks.wrapping_add(1);
        if self.ticks % MATCHED_LINES_PER_POLL == 1 && self.cancelled.is_some_and(|gone| gone()) {
            self.stop.store(true, Ordering::Relaxed);
        }
        if self.stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let line_number = saturating_u32(mat.line_number().unwrap_or(0));
        let at = usize::try_from(mat.absolute_byte_offset()).unwrap_or(usize::MAX);
        let bytes = self.served_line(mat.bytes(), at);
        // What the judge wrote in this block: a block whose every match takes
        // some of it holds no match of the source's text. An inverted search
        // already left such lines out of its matches.
        let hidden = super::written::within(self.written, at..at.saturating_add(bytes.len()));
        if self.inverted_over.is_none()
            && !hidden.is_empty()
            && own_match(self.matcher, bytes, &hidden)? == Some(false)
        {
            return Ok(true);
        }
        if !self.keeps_line(line_number, bytes) {
            return Ok(true);
        }
        let mut count: u32 = 0;
        let mut first_start = None;
        let mut spans = Vec::new();
        let long_line = long_line::is_long(bytes);
        let (raw_rows, prepared_rows) = (self.entry.raw_matches.len(), self.entry.prepared.len());

        if self.work.collect_spans && long_line {
            count = self.collect_long_spans(bytes, line_number, (&mut first_start, &hidden))?;
        } else if self.work.collect_spans {
            count = self.collect_spans(bytes, line_number, (&mut first_start, &hidden))?;
        } else if self.work.enumerate_submatches {
            let keeps_spans = self.work.materialize_line;
            let (stop, cancelled) = (self.stop, self.cancelled);
            super::written::each_own(self.matcher, bytes, &hidden, |matched| {
                if past_block_end(bytes, matched) {
                    return true;
                }
                count = count.saturating_add(1);
                if !keep_going(cancelled, stop, count) {
                    return false;
                }
                first_start.get_or_insert(matched.start());
                if keeps_spans {
                    spans.push((matched.start(), matched.end()));
                }
                true
            })
            .map_err(|error| std::io::Error::other(error.to_string()))?;
            if self.work.materialize_line {
                // One row per group of matches; a group that is one line
                // past a row's bound is shown as a line of one-line mode is.
                for group in line_groups(bytes, &spans) {
                    let first = group
                        .first_match
                        .map(|(start, end)| (start - group.bytes.start, end - group.bytes.start));
                    let hidden = super::written::within(&hidden, group.bytes.clone());
                    let (bytes, line) = (
                        &bytes[group.bytes],
                        line_number.saturating_add(group.first_line),
                    );
                    if long_line::is_long(bytes) {
                        self.long_line_row(bytes, line, first)?;
                    } else if !self.window_rows(bytes, line, &hidden)? {
                        self.line_row(bytes, line, first);
                    }
                }
            }
        }

        if !self.terms.is_empty() {
            self.count_terms(bytes, &hidden)?;
        }
        self.submatches = self.submatches.saturating_add(count.max(1));
        let lines = saturating_u32(bytes.split_inclusive(|&b| b == b'\n').count().max(1));
        self.matched_lines = self.matched_lines.saturating_add(lines);
        // The block's line row, when one was kept; its spans held as kept.
        let rows = self.entry.raw_matches[raw_rows..]
            .iter()
            .map(raw_weight)
            .sum::<usize>()
            + self.entry.prepared[prepared_rows..]
                .iter()
                .map(match_weight)
                .sum::<usize>();
        Ok(self.hold(rows))
    }

    fn context(&mut self, _searcher: &Searcher, ctx: &SinkContext<'_>) -> std::io::Result<bool> {
        let line_number = saturating_u32(ctx.line_number().unwrap_or(0));
        let at = usize::try_from(ctx.absolute_byte_offset()).unwrap_or(usize::MAX);
        let ctx_bytes = self.served_line(ctx.bytes(), at);
        let line_text = if long_line::is_long(ctx_bytes) {
            self.long_line_context(ctx_bytes)
        } else {
            let (line_text, not_utf8) = block_text(ctx_bytes);
            self.not_utf8 |= not_utf8;
            strip_trailing_newline(line_text)
        };
        let weight = context_weight(&line_text);
        self.entry.contexts.insert(line_number, line_text);
        Ok(self.hold(weight))
    }

    fn binary_data(
        &mut self,
        _searcher: &Searcher,
        binary_byte_offset: u64,
    ) -> std::io::Result<bool> {
        // A NUL byte was detected; with `BinaryDetection::quit` the searcher
        // stops here. Record the offset so the pre-NUL prefix can be searched
        // and the file reported as partially covered (`binaryQuit`).
        self.binary_offset = Some(binary_byte_offset);
        Ok(true)
    }
}
