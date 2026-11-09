//! A source too large to scan whole, judged by what its structure says: a pass that
//! walks every byte beside the shown rows' lines, and masks those lines where the
//! walk found secrets. So only a row that holds a secret loses its value.

use super::guard::{HashingReader, drain_digest};
use super::lines::VerificationLines;
use super::*;
use crate::tools::local_search::sources::{Digest, Expected};
use memchr::memchr_iter;
use octocode_engine::security::{
    Edits, Named, Objects, SecretSpan, SecretSpans, SourceWalk, WholeSourcePatterns,
    redaction_placeholder,
};
use std::collections::BTreeMap;
use std::io::Read;

/// A pass that judged its source, shown lines masked in its `read`.
pub(super) struct Walked {
    pub(super) read: VerificationLines,
    pub(super) digest: Option<Digest>,
    /// The shown lines a span reached, each with what the walk replaced of it:
    /// its source line's bytes, as the line's decoded text counts them, to the
    /// masked text `read` keeps.
    pub(super) masked: BTreeMap<usize, Edits>,
}

/// A reader that walks the text it passes on, and marks where shown lines start.
struct Walking<'a, R> {
    inner: R,
    walk: SourceWalk,
    windows: &'a [(usize, usize)],
    at: usize,
    line: usize,
    starts: BTreeMap<usize, usize>,
    /// Where each shown line's bytes end, before its `\r\n` or `\n`.
    ends: BTreeMap<usize, usize>,
    /// The last byte passed on, for a `\r` that ends a chunk.
    last: u8,
}

impl<'a, R> Walking<'a, R> {
    fn new(inner: R, path: &std::path::Path, windows: &'a [(usize, usize)]) -> Self {
        let named = Named {
            objects: Objects::ALL,
            yaml: WholeSourcePatterns::for_path(&path.to_string_lossy()).is_some(),
        };
        let mut walking = Self {
            inner,
            walk: SourceWalk::new(named, octocode_engine::memory::MEMORY.limit()),
            windows,
            at: 0,
            line: 1,
            starts: BTreeMap::new(),
            ends: BTreeMap::new(),
            last: 0,
        };
        walking.mark(0);
        walking
    }

    fn mark(&mut self, start: usize) {
        let line = self.line;
        if self
            .windows
            .iter()
            .any(|&(lo, hi)| lo <= line && line <= hi)
        {
            self.starts.insert(line, start);
        }
    }
}

impl<R: Read> Read for Walking<'_, R> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(out)?;
        let bytes = &out[..read];
        self.walk.feed(bytes);
        for newline in memchr_iter(b'\n', bytes) {
            if self.starts.contains_key(&self.line) {
                let before = newline.checked_sub(1).map_or(self.last, |at| bytes[at]);
                let end = self.at + newline - usize::from(before == b'\r');
                self.ends.insert(self.line, end);
            }
            self.line += 1;
            self.mark(self.at + newline + 1);
        }
        if let Some(&last) = bytes.last() {
            self.last = last;
        }
        self.at += read;
        Ok(read)
    }
}

/// Read and walk the whole source `open` gives, keeping only `windows`' lines.
/// `None` when the source's structure outran the memory budget.
pub(super) fn walked_pass<R: Read>(
    open: impl Fn() -> std::io::Result<R>,
    expected: Expected,
    windows: &[(usize, usize)],
    last_line: usize,
    source: &std::path::Path,
    cancel: &impl CancellationCheck,
) -> Result<Option<Walked>, VerifyReadError> {
    let mut reader = std::io::BufReader::new(HashingReader {
        inner: open()?,
        hasher: matches!(expected, Expected::Stored(_))
            .then(octocode_engine::digest::HashWriter::default),
    });
    let judged = match decoded::byte_order(&mut reader)? {
        Some(order) => read_walking(
            decoded::Utf16Text::new(&mut reader, order),
            windows,
            last_line,
            source,
            cancel,
        )?,
        None => read_walking(&mut reader, windows, last_line, source, cancel)?,
    };
    let digest = drain_digest(reader, cancel)?;
    let Some((mut read, spans, bounds)) = judged else {
        return Ok(None);
    };
    let masked = mask_lines(&mut read, &spans, &bounds);
    Ok(Some(Walked {
        read,
        digest,
        masked,
    }))
}

/// Each shown line's raw byte start and, when a newline ended it, its end.
type Bounds = BTreeMap<usize, (usize, Option<usize>)>;

type Judged = (VerificationLines, Vec<SecretSpan>, Bounds);

fn read_walking<R: Read>(
    text: R,
    windows: &[(usize, usize)],
    last_line: usize,
    source: &std::path::Path,
    cancel: &impl CancellationCheck,
) -> Result<Option<Judged>, VerifyReadError> {
    let mut reader = std::io::BufReader::new(Walking::new(text, source, windows));
    let read = read_verification_lines(&mut reader, windows, last_line, None, cancel)?;
    // The rows are in hand; the walk needs the rest of the source.
    let mut chunk = vec![0u8; VERIFY_CHUNK_BYTES];
    loop {
        cancel.check().map_err(VerifyReadError::Cancelled)?;
        if reader.read(&mut chunk)? == 0 {
            break;
        }
    }
    let walking = reader.into_inner();
    Ok(match walking.walk.finish() {
        SecretSpans::Exact(spans) => {
            // The last line ends with the source when no newline ends it, before
            // a `\r` its read drops.
            let open = (
                walking.line,
                walking.at - usize::from(walking.last == b'\r'),
            );
            let bounds = walking
                .starts
                .iter()
                .map(|(&line, &start)| {
                    let end = walking.ends.get(&line).copied();
                    let end = end.or((line == open.0 && start < walking.at).then_some(open.1));
                    (line, (start, end))
                })
                .collect();
            Some((read, spans, bounds))
        }
        SecretSpans::TooMany => None,
    })
}

/// Hide what `spans` reach of `read`'s kept lines, whose raw bytes `bounds`
/// gives. A span's raw offsets land on the line's decoded text only through an
/// exact mapping ([`decoded_offsets`]); a line without one is hidden whole.
fn mask_lines(
    read: &mut VerificationLines,
    spans: &[SecretSpan],
    bounds: &Bounds,
) -> BTreeMap<usize, Edits> {
    let mut masked = BTreeMap::new();
    for (&line, &(start, end)) in bounds {
        let Some(text) = read.retained.get_mut(&line) else {
            continue;
        };
        let raw_end = end.unwrap_or(start + text.len());
        let first = spans.partition_point(|span| span.range.end <= start);
        let reached = spans[first..]
            .iter()
            .take_while(|span| span.range.start < raw_end.max(start + 1))
            .collect::<Vec<_>>();
        if reached.is_empty() || text.trim().is_empty() {
            continue;
        }
        let (shown, edits) = match decoded_offsets(text, end.map(|end| end - start)) {
            Some(offset) => {
                let ranges = reached.iter().map(|span| {
                    let lo = offset(span.range.start.saturating_sub(start));
                    let hi = offset(span.range.end.min(raw_end).saturating_sub(start));
                    (lo..hi, span.form.pattern())
                });
                masked_line(text, ranges)
            }
            None => {
                let placeholder = redaction_placeholder(reached[0].form.pattern());
                let edits = Edits::of_pass([(0..usize::MAX, placeholder.len())]);
                (placeholder, edits)
            }
        };
        *text = shown;
        masked.insert(line, edits);
    }
    masked
}

/// How raw byte offsets of a line map onto its decoded `text`, `raw_len` bytes
/// long when known, as the search decoded it: UTF-8 as itself, Latin-1 one
/// character per byte. `None` when no exact mapping is known (a U+FFFD
/// replacement, or a length that fits neither).
fn decoded_offsets(text: &str, raw_len: Option<usize>) -> Option<Box<dyn Fn(usize) -> usize + '_>> {
    if text.contains('\u{fffd}') {
        return None;
    }
    if text.is_ascii() || raw_len == Some(text.len()) {
        return Some(Box::new(move |at: usize| at.min(text.len())));
    }
    let chars = text
        .char_indices()
        .map(|(at, _)| at)
        .chain(std::iter::once(text.len()))
        .collect::<Vec<_>>();
    (raw_len == Some(chars.len() - 1)).then(|| {
        Box::new(move |at: usize| chars.get(at).copied().unwrap_or(text.len()))
            as Box<dyn Fn(usize) -> usize>
    })
}

/// `text` with each of `ranges` (its own byte offsets, with the pattern that
/// found them) replaced, its indentation kept, and what was replaced.
fn masked_line<'a>(
    text: &str,
    ranges: impl Iterator<Item = (std::ops::Range<usize>, &'a str)>,
) -> (String, Edits) {
    let mut out = String::with_capacity(text.len());
    let mut replaced = Vec::new();
    let mut from = 0;
    for (range, pattern) in ranges {
        let lo = text.floor_char_boundary(range.start.max(from));
        let hi = text.ceil_char_boundary(range.end.min(text.len()));
        if hi <= lo {
            continue;
        }
        let hidden = text.get(lo..hi).unwrap_or_default();
        let lo = lo + (hidden.len() - hidden.trim_start().len());
        out.push_str(text.get(from..lo).unwrap_or_default());
        if lo < hi {
            let placeholder = redaction_placeholder(pattern);
            replaced.push((lo..hi, placeholder.len()));
            out.push_str(&placeholder);
        }
        from = hi;
    }
    out.push_str(text.get(from..).unwrap_or_default());
    (out, Edits::of_pass(replaced))
}
