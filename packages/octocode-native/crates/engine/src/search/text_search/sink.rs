//! The `grep_searcher` sink that collects one file's matches, spans, contexts, and line weights.

use super::mode::MatchWork;
use super::state::{Charge, Held, context_weight, match_weight, raw_weight};
use crate::num::saturating_u32;
use crate::search::relevance;
use crate::search::text_parser::{FileEntry, RawMatch, strip_trailing_newline};
use crate::text::interior_offset;
use crate::text::utf8_offsets::byte_to_utf16_offset;
use crate::types::TextSearchMatch;
use aho_corasick::AhoCorasick;
use grep_matcher::Matcher;
use grep_searcher::{Searcher, Sink, SinkContext, SinkMatch};
use std::sync::atomic::{AtomicBool, Ordering};

mod retained;
pub(super) use retained::keep_span;

mod columns;
mod long_line;
mod row_window;
use columns::{SpanColumns, block_text, span_value};

/// Whether `found`, a match in a matched block's `bytes`, is an empty match
/// past the block's last line break: the start of a line the block does not
/// hold, so no match of the block (rg's rule: a line's matches end with the
/// line, an empty one at its end included).
fn past_block_end(bytes: &[u8], found: grep_matcher::Match) -> bool {
    found.is_empty() && bytes.ends_with(b"\n") && found.start() >= bytes.len()
}

/// One row's part of a matched block: its whole lines (`bytes`, a range of
/// the block), its first line's index in the block, and where its first
/// match starts.
struct LineGroup {
    bytes: std::ops::Range<usize>,
    first_line: u32,
    first_start: Option<usize>,
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
    let mut groups: Vec<(usize, usize, usize)> = Vec::new();
    for &(start, end) in spans {
        // A match ending in its line's newline ends on that line.
        let (first, last) = (line_at(start), line_at(end.saturating_sub(1).max(start)));
        match groups.last_mut() {
            Some(group) if first <= group.1 => group.1 = group.1.max(last),
            _ => groups.push((first, last, start)),
        }
    }
    if groups.is_empty() {
        return vec![LineGroup {
            bytes: 0..bytes.len(),
            first_line: 0,
            first_start: None,
        }];
    }
    groups
        .into_iter()
        .map(|(first, last, start)| LineGroup {
            bytes: starts[first]..end_of(last),
            first_line: saturating_u32(first),
            first_start: Some(start),
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
    /// What this file's kept rows hold of the search's bound; past it the
    /// sink raises `stop` and keeps nothing more.
    pub(super) charge: Charge,
    /// Absolute offset of the first NUL byte when the file was quit as binary.
    /// The searcher stops there, so bytes after it are unsearched.
    pub(super) binary_offset: Option<u64>,
    /// Summed [`relevance::line_weight`] (only with `work.weigh_lines`).
    pub(super) line_weight: u32,
    /// A matched line declares the matched name (only with `work.weigh_lines`).
    pub(super) declares: bool,
    /// The alternation's words, when ranking by their coverage.
    pub(super) words: Option<&'a AhoCorasick>,
    /// Bit `i`: word `i` of `words` matched a line.
    pub(super) covered: u64,
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
}

impl<'a, M: Matcher> CollectSink<'a, M> {
    pub(super) fn new(
        matcher: &'a M,
        work: MatchWork,
        (stop, held): (&'a AtomicBool, &std::sync::Arc<Held>),
        (words, terms): (Option<&'a AhoCorasick>, &'a [M]),
    ) -> Self {
        Self {
            charge: Charge::new(held),
            words,
            terms,
            term_counts: vec![0; terms.len()],
            test_code: None,
            line_scope: None,
            scope_spans: None,
            inspect_deferred: None,
            test_spans: None,
            test_lines_excluded: 0,
            source: None,
            covered: 0,
            matcher,
            entry: FileEntry::new(),
            submatches: 0,
            matched_lines: 0,
            work,
            om_matches: Vec::new(),
            stop,
            binary_offset: None,
            line_weight: 0,
            declares: false,
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
        first_start: &mut Option<usize>,
    ) -> std::io::Result<u32> {
        let mut count = 0u32;
        let (line_text, not_utf8) = block_text(bytes);
        self.not_utf8 |= not_utf8;
        // find_iter yields non-overlapping matches L→R.
        let (matcher, stop) = (self.matcher, self.stop);
        let (om, charge) = (&mut self.om_matches, &mut self.charge);
        let mut columns = SpanColumns::new(bytes, &line_text);
        matcher
            .find_iter(bytes, |m| {
                if past_block_end(bytes, m) {
                    return true;
                }
                count = count.saturating_add(1);
                first_start.get_or_insert(m.start());
                let (start, end) = (columns.lossy(m.start()), columns.lossy(m.end()));
                let value = span_value(&line_text, start, end);
                let (line_delta, column, byte_offset) = columns.position(start);
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
    /// of matches ([`line_groups`]), its first hit at `first_start`.
    fn line_row(&mut self, bytes: &[u8], line_number: u32, first_start: Option<usize>) {
        let (line_text, not_utf8) = block_text(bytes);
        self.not_utf8 |= not_utf8;
        let line_text = strip_trailing_newline(line_text);
        let start = first_start.unwrap_or(0);
        let byte = interior_offset(bytes, start).min(line_text.len());
        let column = saturating_u32(byte_to_utf16_offset(&line_text, byte));
        let byte_offset = saturating_u32(byte);
        if self.work.deferred_line {
            let mut hit = crate::search::text_parser::line_match(
                &line_text,
                line_number,
                column,
                byte_offset,
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
                rank: relevance::line_rank(bytes, start),
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

    /// One matched block's per-term counts. With enumerated submatches
    /// (count-matches) each submatch counts once, for the first term whose
    /// own match there is that submatch, else the first that matches from its
    /// start (a block with no enumerable submatch counts for the first term
    /// it holds). Count-lines enumerates none: each term the block holds
    /// counts the block's lines.
    fn count_terms(&mut self, bytes: &[u8], spans: &[(usize, usize)]) -> std::io::Result<()> {
        let failed = |error: M::Error| std::io::Error::other(error.to_string());
        let terms = self.terms;
        if self.work.enumerate_submatches {
            if spans.is_empty() {
                for (index, term) in terms.iter().enumerate() {
                    if term.is_match(bytes).map_err(failed)? {
                        self.term_counts[index] = self.term_counts[index].saturating_add(1);
                        break;
                    }
                }
                return Ok(());
            }
            for &(start, end) in spans {
                let mut exact = None;
                let mut from_start = None;
                for (index, term) in terms.iter().enumerate() {
                    if let Some(found) = term.find_at(bytes, start).map_err(failed)?
                        && found.start() == start
                    {
                        from_start.get_or_insert(index);
                        if found.end() == end {
                            exact = Some(index);
                            break;
                        }
                    }
                }
                if let Some(index) = exact.or(from_start) {
                    self.term_counts[index] = self.term_counts[index].saturating_add(1);
                }
            }
            return Ok(());
        }
        let lines = saturating_u32(bytes.split_inclusive(|&b| b == b'\n').count().max(1));
        for (index, term) in terms.iter().enumerate() {
            if term.is_match(bytes).map_err(failed)? {
                self.term_counts[index] = self.term_counts[index].saturating_add(lines);
            }
        }
        Ok(())
    }
}

impl<M: Matcher> Sink for CollectSink<'_, M> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> std::io::Result<bool> {
        // Stop searching this file *before* doing more per-line work. Returning
        // Ok(false) ends the search cleanly and keeps the matches collected so far.
        if self.stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let line_number = saturating_u32(mat.line_number().unwrap_or(0));
        let bytes = mat.bytes();
        if !self.keeps_line(line_number, bytes) {
            return Ok(true);
        }
        let mut count: u32 = 0;
        let mut first_start = None;
        let mut spans = Vec::new();
        let long_line = long_line::is_long(bytes);
        let (raw_rows, prepared_rows) = (self.entry.raw_matches.len(), self.entry.prepared.len());

        if self.work.collect_spans && long_line {
            count = self.collect_long_spans(bytes, line_number, &mut first_start)?;
        } else if self.work.collect_spans {
            count = self.collect_spans(bytes, line_number, &mut first_start)?;
        } else if self.work.enumerate_submatches {
            let keeps_spans = !self.terms.is_empty() || self.work.materialize_line;
            self.matcher
                .find_iter(bytes, |matched| {
                    if past_block_end(bytes, matched) {
                        return true;
                    }
                    count = count.saturating_add(1);
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
                    let first = group.first_start.map(|start| start - group.bytes.start);
                    let (bytes, line) = (
                        &bytes[group.bytes],
                        line_number.saturating_add(group.first_line),
                    );
                    if long_line::is_long(bytes) {
                        self.long_line_row(bytes, line, first)?;
                    } else if !self.window_rows(bytes, line)? {
                        self.line_row(bytes, line, first);
                    }
                }
            }
        }

        if !self.terms.is_empty() {
            self.count_terms(bytes, &spans)?;
        }
        if self.work.weigh_lines {
            // Count-lines never enumerates submatches: find the first one here.
            let first = match first_start {
                Some(start) => start,
                None => self
                    .matcher
                    .find(bytes)
                    .map_err(|error| std::io::Error::other(error.to_string()))?
                    .map_or(0, |matched| matched.start()),
            };
            let weight = relevance::line_weight(bytes, first);
            self.declares |= weight == relevance::DECLARATION_WEIGHT;
            self.line_weight = self.line_weight.saturating_add(weight);
            if let Some(words) = self.words {
                for found in words.find_overlapping_iter(bytes) {
                    self.covered |= 1 << found.pattern().as_usize();
                }
            }
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
        let line_text = if long_line::is_long(ctx.bytes()) {
            self.long_line_context(ctx.bytes())
        } else {
            let (line_text, not_utf8) = block_text(ctx.bytes());
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
