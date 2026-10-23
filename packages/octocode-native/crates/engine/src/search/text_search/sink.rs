//! The `grep_searcher` sink that collects one file's matches, spans, contexts, and line weights.

use super::mode::MatchWork;
use crate::search::relevance;
use crate::search::text_parser::{FileEntry, RawMatch, strip_trailing_newline};
use crate::text::utf8_offsets::byte_to_utf16_offset;
use crate::types::TextSearchMatch;
use aho_corasick::AhoCorasick;
use grep_matcher::Matcher;
use grep_searcher::{Searcher, Sink, SinkContext, SinkMatch};
use sha2::{Digest as _, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Cap on emitted spans per file in only-matching mode, so a pathological
/// file with a huge number of hits can't blow up the result. A long line is
/// reachable whole below it; the true submatch count is still reported in
/// stats.
pub(super) const MAX_ONLY_MATCHING_PER_FILE: u32 = 200_000;

/// Map a byte offset in the raw line `bytes` to the matching byte offset in
/// `String::from_utf8_lossy(bytes)`. Each invalid byte becomes a 3-byte U+FFFD,
/// so raw offsets must not be applied to the lossy text directly.
fn lossy_offset(bytes: &[u8], offset: usize) -> usize {
    let offset = offset.min(bytes.len());
    match std::str::from_utf8(bytes) {
        Ok(_) => offset,
        Err(_) => String::from_utf8_lossy(&bytes[..offset]).len(),
    }
}

/// Maps raw submatch offsets on one line to lossy-text offsets and UTF-16
/// columns. `find_iter` yields non-overlapping matches left to right, so a
/// forward cursor keeps a line with k spans at O(line + k) instead of
/// rescanning the prefix per span (O(line · k)). The line's UTF-8 validity is
/// checked once; an invalid line keeps the exact per-span lossy mapping.
struct SpanColumns<'a> {
    pub(super) bytes: &'a [u8],
    text: &'a str,
    valid_utf8: bool,
    cursor_byte: usize,
    cursor_utf16: usize,
}

impl<'a> SpanColumns<'a> {
    pub(super) fn new(bytes: &'a [u8], text: &'a str) -> Self {
        Self {
            bytes,
            text,
            valid_utf8: std::str::from_utf8(bytes).is_ok(),
            cursor_byte: 0,
            cursor_utf16: 0,
        }
    }

    fn lossy(&self, raw: usize) -> usize {
        if self.valid_utf8 {
            raw.min(self.bytes.len())
        } else {
            lossy_offset(self.bytes, raw)
        }
    }

    /// UTF-16 column of lossy byte offset `start`, same as
    /// `byte_to_utf16_offset(text, start)`.
    pub(super) fn column(&mut self, start: usize) -> usize {
        let target = self.text.floor_char_boundary(start);
        if target < self.cursor_byte {
            return byte_to_utf16_offset(self.text, target);
        }
        self.cursor_utf16 += self.text[self.cursor_byte..target].encode_utf16().count();
        self.cursor_byte = target;
        self.cursor_utf16
    }
}

/// Slice the matched span `[start, end)` (byte offsets) out of `line` as a
/// valid UTF-8 substring.
fn span_value(line: &str, start: usize, end: usize) -> String {
    let start = line.floor_char_boundary(start);
    let end = line.ceil_char_boundary(end).max(start);
    line[start..end].to_owned()
}

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
}

impl<'a, M: Matcher> CollectSink<'a, M> {
    pub(super) fn new(
        matcher: &'a M,
        work: MatchWork,
        deadline: Option<Instant>,
        stop: &'a AtomicBool,
        words: Option<&'a AhoCorasick>,
    ) -> Self {
        Self {
            words,
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
        }
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
        let line_number = mat.line_number().unwrap_or(0) as u32;
        let bytes = mat.bytes();
        let mut count: u32 = 0;
        let mut first_start = None;

        if self.work.collect_spans {
            let line_cow = String::from_utf8_lossy(bytes);
            let line_text = strip_trailing_newline(line_cow.into_owned());
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
                        let column = columns.column(start) as u32;
                        om.push(TextSearchMatch {
                            deferred: None,
                            line: line_number,
                            last_line: None,
                            column,
                            byte_offset: start as u32,
                            value_digest: Sha256::digest(value.trim().as_bytes()).into(),
                            value,
                            count: None,
                            rank: None,
                            original_chars: None,
                            original_bytes: None,
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
                self.om_matches.push(TextSearchMatch {
                    deferred: None,
                    line: line_number,
                    last_line: None,
                    column: 0,
                    byte_offset: 0,
                    value_digest: Sha256::digest(line_text.trim().as_bytes()).into(),
                    value: line_text,
                    count: None,
                    rank: None,
                    original_chars: None,
                    original_bytes: None,
                });
            }
        } else if self.work.enumerate_submatches {
            self.matcher
                .find_iter(bytes, |matched| {
                    count = count.saturating_add(1);
                    first_start.get_or_insert(matched.start());
                    true
                })
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            if self.work.materialize_line {
                let line_cow = String::from_utf8_lossy(bytes);
                let line_text = strip_trailing_newline(line_cow.into_owned());
                let column = byte_to_utf16_offset(
                    &line_text,
                    lossy_offset(bytes, first_start.unwrap_or(0)).min(line_text.len()),
                ) as u32;
                if let Some(limit) = self.work.deferred_snippet {
                    let mut hit = crate::search::text_parser::line_match(
                        &line_text,
                        line_number,
                        column,
                        limit,
                    );
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
        self.matched_lines = self.matched_lines.saturating_add(1);
        Ok(true)
    }

    fn context(&mut self, _searcher: &Searcher, ctx: &SinkContext<'_>) -> std::io::Result<bool> {
        let line_number = ctx.line_number().unwrap_or(0) as u32;
        let line_cow = String::from_utf8_lossy(ctx.bytes());
        let line_text = strip_trailing_newline(line_cow.into_owned());
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
