//! The scan of a streamed `matchString` read: the source flows through a
//! sliding text window, judged by the whole-file matcher. A match may cross a
//! scan edge: each judgement owns the matches that start before its edge and
//! the next one starts there, behind a lookahead the match can reach into,
//! so every hit line is counted once whatever the source's line lengths.

use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
use crate::execution::cancel::CancellationCheck;
use crate::tools::source_view::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_engine::text::{line_at, line_starts};
use std::io::BufRead;

/// New source bytes each judgement of the matcher takes in. Sizes the work,
/// never the result.
const SCAN_CHUNK_BYTES: usize = 1024 * 1024;
/// Source bytes the matcher sees past a scan edge: a match that starts before
/// the edge and ends within them is found whole. A literal pattern's longest
/// match is known and always fits; a regex match longer than this, crossing
/// an edge, is disclosed rather than assumed ([`Scan::overlap`]).
const SCAN_OVERLAP_BYTES: usize = 256 * 1024;
/// The most bytes the one character of context before a scan edge takes.
#[cfg(test)]
const MARGIN_BYTES: usize = 4;
/// Source bytes read at once.
const READ_BYTES: usize = 64 * 1024;

/// The sizes of one scan's window.
#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub(super) chunk: usize,
    pub(super) overlap: usize,
    pub(super) read: usize,
}

impl Limits {
    /// The window sizes of `q`'s patterns: a literal's overlap covers its
    /// longest match in the source (case folding grows a character at most
    /// fourfold).
    pub(super) fn of(q: &LocalFetchQuery) -> Self {
        let literal = local_fetch_query::match_strings(q)
            .iter()
            .map(|pattern| pattern.len() * 4)
            .max()
            .filter(|_| !local_fetch_query::is_regex(q))
            .unwrap_or(0);
        Self {
            chunk: SCAN_CHUNK_BYTES,
            overlap: SCAN_OVERLAP_BYTES.max(literal),
            read: READ_BYTES,
        }
    }

    /// The most text the window holds at once.
    #[cfg(test)]
    pub(super) fn text_bound(&self) -> usize {
        MARGIN_BYTES + self.chunk + 2 * self.overlap + self.read
    }
}

/// What one scan found: every hit's count, the next page's merged windows
/// and hit lines, and whether windows remain past them.
#[derive(Default)]
pub(super) struct Scan {
    pub(super) count: usize,
    pub(super) windows: Vec<(usize, usize)>,
    pub(super) hits: Vec<usize>,
    pub(super) more: bool,
    pub(super) total_lines: usize,
    pub(super) binary: bool,
    /// The line a regex scan stopped at its match limit.
    pub(super) limited: Option<usize>,
    /// Scan edges the source crossed: at a line start, and inside a line (one
    /// too long to cut at a line start, or a match that reached over the edge).
    pub(super) line_edges: usize,
    pub(super) mid_edges: usize,
    /// The source bytes a match crossing an edge is followed for.
    pub(super) overlap: usize,
    /// What the pass proved of the whole source: its digest, and where the
    /// page's windows can be read from without a second pass.
    pub(super) proven: Option<super::proven::Proven>,
    /// The most text the window held at once.
    #[cfg(test)]
    pub(super) peak: usize,
    /// Every hit line counted, in order.
    #[cfg(test)]
    pub(super) lines: Vec<usize>,
}

impl Scan {
    /// Count hit `line` and keep its context `window` when the window reaches
    /// past `resume`, the last line earlier pages served.
    fn hit(&mut self, line: usize, window: (usize, usize), resume: usize) {
        self.count += 1;
        #[cfg(test)]
        self.lines.push(line);
        if window.1 <= resume {
            return;
        }
        let window = (window.0.max(resume + 1), window.1);
        let room = !self.more && self.windows.len() < RANGES_MAX_ITEMS;
        match self.windows.last_mut() {
            Some(last) if window.0 <= last.1 + 1 => {
                last.1 = last.1.max(window.1);
                self.hits.push(line);
            }
            _ if room => {
                self.windows.push(window);
                self.hits.push(line);
            }
            _ => self.more = true,
        }
    }
}

/// Judge the whole source behind `file` for `q`'s patterns, from its start.
pub(super) fn scan(
    q: &LocalFetchQuery,
    file: &std::fs::File,
    resume: usize,
    cancel: &impl CancellationCheck,
) -> Result<Scan, ExtractionError> {
    scan_with(q, file, resume, Limits::of(q), cancel)
}

pub(super) fn scan_with(
    q: &LocalFetchQuery,
    file: &std::fs::File,
    resume: usize,
    limits: Limits,
    cancel: &impl CancellationCheck,
) -> Result<Scan, ExtractionError> {
    let failed = |error: String| ExtractionError::new(ErrorCode::FileReadFailed, error);
    let mut reader = std::io::BufReader::with_capacity(limits.read, file);
    std::io::Seek::seek(&mut reader, std::io::SeekFrom::Start(0))
        .map_err(|error| failed(error.to_string()))?;
    let context = local_fetch_query::context_lines(q);
    let mut scanner = Scanner {
        q,
        cancel,
        limits,
        resume,
        before: octocode_engine::num::saturating_usize(context.before),
        after: octocode_engine::num::saturating_usize(context.after),
        regions: local_fetch_query::line_ranges(q)
            .iter()
            .map(|range| (range.start, range.end))
            .collect(),
        ranged: local_fetch_query::has_ranges(q),
        found: Scan {
            overlap: limits.overlap,
            ..Scan::default()
        },
        text: String::new(),
        pending: Vec::new(),
        line: 1,
        owned_from: 0,
        counted: 0,
        newlines: 0,
        last: None,
        proof: super::proven::Proof::new(file, &[]).map_err(failed)?,
    };
    loop {
        let ended = scanner.fill(&mut reader)?;
        if scanner.found.limited.is_none() {
            scanner.judge(ended)?;
        }
        if ended {
            break;
        }
    }
    Ok(scanner.finish())
}

/// One scan's sliding window over the source.
struct Scanner<'a, C: CancellationCheck> {
    q: &'a LocalFetchQuery,
    cancel: &'a C,
    limits: Limits,
    resume: usize,
    before: usize,
    after: usize,
    /// The read's ranges as first and last lines.
    regions: Vec<(usize, usize)>,
    ranged: bool,
    found: Scan,
    /// The window: the character before its own bytes, the lines (or a line's part) being judged and
    /// the lookahead behind them.
    text: String,
    /// The end of a character the window's last read cut.
    pending: Vec<u8>,
    /// The 1-based line `text` starts in.
    line: usize,
    /// Where the window's own bytes start, behind the one character of context.
    owned_from: usize,
    /// The last line counted as a hit.
    counted: usize,
    /// Newlines and the last byte of the source read so far.
    newlines: usize,
    last: Option<u8>,
    /// Every source byte, read once: the digest and window checkpoints.
    proof: super::proven::Proof,
}

impl<C: CancellationCheck> Scanner<'_, C> {
    /// Read the source into the window until it holds a full judgement, or
    /// the source ends (true). Once a regex limit stops the scan, the rest of
    /// the source is only counted.
    fn fill(&mut self, reader: &mut impl BufRead) -> Result<bool, ExtractionError> {
        let failed = |error: String| ExtractionError::new(ErrorCode::FileReadFailed, error);
        let want = self.owned_from + self.limits.chunk + 2 * self.limits.overlap;
        let mut since_check = 0;
        loop {
            let limited = self.found.limited.is_some();
            if !limited && self.text.len() >= want {
                return Ok(false);
            }
            let piece = reader
                .fill_buf()
                .map_err(|error| failed(error.to_string()))?;
            if piece.is_empty() {
                let rest = std::mem::take(&mut self.pending);
                self.text.push_str(&String::from_utf8_lossy(&rest));
                return Ok(true);
            }
            let read = piece.len();
            self.newlines += piece.iter().filter(|byte| **byte == b'\n').count();
            self.last = piece.last().copied();
            super::count_read(read);
            self.proof.feed(piece);
            if !limited {
                self.found.binary |= piece.contains(&0);
                self.decode(piece);
            }
            reader.consume(read);
            since_check += read;
            if since_check >= self.limits.chunk {
                since_check = 0;
                self.cancel.check().map_err(failed)?;
            }
        }
    }

    /// Append source bytes to the window as text, an incomplete character at
    /// their end waiting for the next read.
    fn decode(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    self.text.push_str(text);
                    self.pending.clear();
                    return;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    self.text
                        .push_str(std::str::from_utf8(&self.pending[..valid]).unwrap_or_default());
                    let Some(bad) = error.error_len() else {
                        self.pending.drain(..valid);
                        return;
                    };
                    self.text.push('\u{fffd}');
                    self.pending.drain(..valid + bad);
                }
            }
        }
    }

    /// The window's scan edge: the start of a line within the lookahead's
    /// reach, or the lookahead's start inside a line too long to cut at one.
    fn edge(&self) -> usize {
        let len = self.text.len();
        let ideal = self.boundary(len.saturating_sub(self.limits.overlap));
        let from = self.boundary(ideal.saturating_sub(self.limits.overlap));
        self.text.as_bytes()[from..ideal]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(ideal, |at| from + at + 1)
    }

    /// The character boundary at or before `at`.
    fn boundary(&self, mut at: usize) -> usize {
        while !self.text.is_char_boundary(at) {
            at -= 1;
        }
        at
    }

    /// Judge the window: record the hit lines of the matches that start in
    /// it before its edge, then slide the window to the edge.
    fn judge(&mut self, last: bool) -> Result<(), ExtractionError> {
        self.cancel
            .check()
            .map_err(|error| ExtractionError::new(ErrorCode::FileReadFailed, error))?;
        #[cfg(test)]
        {
            self.found.peak = self.found.peak.max(self.text.len());
        }
        let found = extraction::match_spans(self.q, &self.text)?;
        let mut edge = if last || found.truncated {
            self.text.len()
        } else {
            self.edge()
        };
        let starts = line_starts(&self.text);
        let (mut hit, owned_from) = (0, self.owned_from);
        for &(start, end) in found.spans.iter().filter(|span| span.0 >= owned_from) {
            if start >= edge && !(last || found.truncated) {
                break;
            }
            // A match reaching past the edge is the window's own.
            edge = edge.max(end.min(self.text.len()));
            if start >= self.text.len() {
                continue;
            }
            let first = self.line - 1 + line_at(&starts, start);
            let end_line = if end > start {
                self.line - 1 + line_at(&starts, end - 1)
            } else {
                first
            };
            for line in first.max(self.counted + 1)..=end_line {
                self.record(line);
                hit = line;
            }
            self.counted = self.counted.max(end_line);
        }
        if found.truncated {
            self.found.limited = Some(if hit > 0 { hit } else { self.line });
        }
        if !last && self.found.limited.is_none() {
            self.slide(edge);
        }
        Ok(())
    }

    /// Count hit `line` with its context window, within the read's ranges.
    fn record(&mut self, line: usize) {
        let mut window = (
            line.saturating_sub(self.before).max(1),
            line.saturating_add(self.after),
        );
        if self.ranged {
            let Some(region) = self
                .regions
                .iter()
                .find(|range| (range.0..=range.1).contains(&line))
            else {
                return;
            };
            window = (window.0.max(region.0), window.1.min(region.1));
        }
        self.found.hit(line, window, self.resume);
    }

    /// Drop the window's text before its `edge`, but the character before it:
    /// a match there already belongs to the judgement that owned it, and
    /// anchors and word boundaries read it as context.
    fn slide(&mut self, edge: usize) {
        if self.text.as_bytes()[edge - 1] == b'\n' {
            self.found.line_edges += 1;
        } else {
            self.found.mid_edges += 1;
        }
        let drop = self.boundary(edge - 1);
        self.line += self.text.as_bytes()[..drop]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count();
        self.text.drain(..drop);
        self.owned_from = edge - drop;
    }

    /// The scan's totals, once the source ended.
    fn finish(mut self) -> Scan {
        let lines = self.newlines + usize::from(self.last.is_some_and(|byte| byte != b'\n'));
        self.found.total_lines = lines;
        for window in &mut self.found.windows {
            window.1 = window.1.min(lines);
        }
        self.found.proven = Some(self.proof.finish().proven);
        self.found
    }
}
