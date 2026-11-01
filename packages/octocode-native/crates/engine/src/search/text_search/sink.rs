//! The `grep_searcher` sink that collects one file's matches, spans, contexts, and line weights.

use super::mode::MatchWork;
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
use std::time::Instant;

mod columns;
use columns::{SpanColumns, block_text, span_value};

/// Cap on emitted spans per file in only-matching mode, so a pathological
/// file with a huge number of hits can't blow up the result. A long line is
/// reachable whole below it; the true submatch count is still reported in
/// stats.
pub(super) const MAX_ONLY_MATCHING_PER_FILE: u32 = 200_000;

/// `grep_searcher::Sink` that accumulates matches/contexts for one file and,
/// using the matcher, derives the 0-based UTF-16 column of the first submatch.
pub(super) struct CollectSink<'a, M: Matcher> {
    pub(super) matcher: &'a M,
    pub(super) entry: FileEntry,
    pub(super) submatches: u32,
    pub(super) matched_lines: u32,
    pub(super) work: MatchWork,
    /// Accumulated only-matching spans for this file.
    pub(super) om_matches: Vec<TextSearchMatch>,
    /// A retained span limit must never be reported as an exhaustive search.
    pub(super) span_cap_reached: bool,
    /// Wall-clock ceiling for this file's search (see [`PCRE2_SEARCH_DEADLINE`](super::pcre2::PCRE2_SEARCH_DEADLINE)).
    /// `None` disables the deadline (linear engines don't need it).
    pub(super) deadline: Option<Instant>,
    /// Search-wide stop flag, polled between matched lines.
    pub(super) stop: &'a AtomicBool,
    /// Set when `deadline` was hit and the search was stopped early.
    pub(super) deadline_hit: bool,
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
    /// This file's test-only spans, read on its first matched line.
    pub(super) test_spans: Option<std::sync::Arc<Vec<[u32; 2]>>>,
    /// Matched lines the scope left out.
    pub(super) test_lines_excluded: u32,
}

impl<'a, M: Matcher> CollectSink<'a, M> {
    pub(super) fn new(
        matcher: &'a M,
        work: MatchWork,
        deadline: Option<Instant>,
        stop: &'a AtomicBool,
        (words, terms): (Option<&'a AhoCorasick>, &'a [M]),
    ) -> Self {
        Self {
            words,
            terms,
            term_counts: vec![0; terms.len()],
            test_code: None,
            test_spans: None,
            test_lines_excluded: 0,
            covered: 0,
            matcher,
            entry: FileEntry::new(),
            submatches: 0,
            matched_lines: 0,
            work,
            om_matches: Vec::new(),
            span_cap_reached: false,
            deadline,
            stop,
            deadline_hit: false,
            binary_offset: None,
            line_weight: 0,
            declares: false,
            not_utf8: false,
        }
    }
}

impl<M: Matcher> CollectSink<'_, M> {
    /// Whether a test-code scope keeps the matched block at 1-based
    /// `line`; one it leaves out is counted. The file's spans are read on
    /// its first matched line, so a file without hits is never parsed.
    fn keeps_line(&mut self, line: u32, bytes: &[u8]) -> bool {
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
        if let Some(deadline) = self.deadline
            && Instant::now() >= deadline
        {
            self.deadline_hit = true;
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

        if self.work.collect_spans {
            let (line_text, not_utf8) = block_text(bytes);
            self.not_utf8 |= not_utf8;
            // Emit one span per submatch with its own UTF-16 column, rather than
            // one whole-line match. find_iter yields non-overlapping matches L→R.
            let matcher = self.matcher;
            let om = &mut self.om_matches;
            let mut columns = SpanColumns::new(bytes, &line_text);
            matcher
                .find_iter(bytes, |m| {
                    count = count.saturating_add(1);
                    first_start.get_or_insert(m.start());
                    if om.len() < MAX_ONLY_MATCHING_PER_FILE as usize {
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
                        om.push(TextSearchMatch {
                            deferred: None,
                            line,
                            last_line: (lines > 1).then(|| line.saturating_add(lines - 1)),
                            column,
                            byte_offset,
                            value_digest: TextSearchMatch::digest_of(&value),
                            value,
                            count: None,
                            rank: None,
                        });
                    } else {
                        self.span_cap_reached = true;
                    }
                    true
                })
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            // A matched line with no enumerable submatch (e.g. zero-width or
            // multiline block) still yields one span: the whole line.
            if count == 0 {
                count = 1;
                let line_text = strip_trailing_newline(line_text);
                self.om_matches.push(TextSearchMatch {
                    deferred: None,
                    line: line_number,
                    last_line: None,
                    column: 0,
                    byte_offset: 0,
                    value_digest: TextSearchMatch::digest_of(&line_text),
                    value: line_text,
                    count: None,
                    rank: None,
                });
            }
        } else if self.work.enumerate_submatches {
            let counts_terms = !self.terms.is_empty();
            self.matcher
                .find_iter(bytes, |matched| {
                    count = count.saturating_add(1);
                    first_start.get_or_insert(matched.start());
                    if counts_terms {
                        spans.push((matched.start(), matched.end()));
                    }
                    true
                })
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            if self.work.materialize_line {
                let (line_text, not_utf8) = block_text(bytes);
                self.not_utf8 |= not_utf8;
                let line_text = strip_trailing_newline(line_text);
                let column = saturating_u32(byte_to_utf16_offset(
                    &line_text,
                    interior_offset(bytes, first_start.unwrap_or(0)).min(line_text.len()),
                ));
                if self.work.deferred_line {
                    let mut hit =
                        crate::search::text_parser::line_match(&line_text, line_number, column);
                    hit.value = String::new();
                    hit.deferred = Some(0);
                    self.entry.prepared.push(hit);
                } else {
                    self.entry.raw_matches.push(RawMatch {
                        line_text,
                        line_number,
                        column,
                    });
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
        Ok(true)
    }

    fn context(&mut self, _searcher: &Searcher, ctx: &SinkContext<'_>) -> std::io::Result<bool> {
        let line_number = saturating_u32(ctx.line_number().unwrap_or(0));
        let (line_text, not_utf8) = block_text(ctx.bytes());
        self.not_utf8 |= not_utf8;
        let line_text = strip_trailing_newline(line_text);
        self.entry.contexts.insert(line_number, line_text);
        Ok(true)
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
