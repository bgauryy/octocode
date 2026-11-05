//! A source too large to scan whole, judged by what its structure says: a pass that
//! walks every byte beside the shown rows' lines, and masks those lines where the
//! walk found secrets. So only a row that holds a secret loses its value.

use super::guard::{HashingReader, drain_digest};
use super::lines::VerificationLines;
use super::*;
use crate::tools::local_search::sources::{Digest, Expected};
use memchr::memchr_iter;
use octocode_engine::security::{
    Named, SecretSpan, SecretSpans, SourceWalk, WholeSourcePatterns, redaction_placeholder,
};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

/// A pass that judged its source, shown lines masked in its `read`.
pub(super) struct Walked {
    pub(super) read: VerificationLines,
    pub(super) digest: Option<Digest>,
    /// The shown lines a span reached.
    pub(super) masked: BTreeSet<usize>,
}

/// A reader that walks the text it passes on, and marks where shown lines start.
struct Walking<'a, R> {
    inner: R,
    walk: SourceWalk,
    windows: &'a [(usize, usize)],
    at: usize,
    line: usize,
    starts: BTreeMap<usize, usize>,
}

impl<'a, R> Walking<'a, R> {
    fn new(inner: R, path: &std::path::Path, windows: &'a [(usize, usize)]) -> Self {
        let named = Named {
            objects: true,
            yaml: WholeSourcePatterns::for_path(&path.to_string_lossy()).is_some(),
        };
        let mut walking = Self {
            inner,
            walk: SourceWalk::new(named, octocode_engine::memory::MEMORY.limit()),
            windows,
            at: 0,
            line: 1,
            starts: BTreeMap::new(),
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
            self.line += 1;
            self.mark(self.at + newline + 1);
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
    let Some((mut read, spans, starts)) = judged else {
        return Ok(None);
    };
    let masked = mask_lines(&mut read, &spans, &starts);
    Ok(Some(Walked {
        read,
        digest,
        masked,
    }))
}

type Judged = (VerificationLines, Vec<SecretSpan>, BTreeMap<usize, usize>);

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
        SecretSpans::Exact(spans) => Some((read, spans, walking.starts)),
        SecretSpans::TooMany => None,
    })
}

/// Hide what `spans` reach of `read`'s kept lines, whose starts are `starts`. A
/// line a decode changed (it holds U+FFFD) has no sure offsets, so all of it goes.
fn mask_lines(
    read: &mut VerificationLines,
    spans: &[SecretSpan],
    starts: &BTreeMap<usize, usize>,
) -> BTreeSet<usize> {
    let mut masked = BTreeSet::new();
    for (&line, &start) in starts {
        let Some(text) = read.retained.get_mut(&line) else {
            continue;
        };
        let end = start + text.len();
        let first = spans.partition_point(|span| span.range.end <= start);
        let reached = spans[first..]
            .iter()
            .take_while(|span| span.range.start < end)
            .collect::<Vec<_>>();
        if reached.is_empty() || text.trim().is_empty() {
            continue;
        }
        *text = if text.contains('\u{fffd}') {
            redaction_placeholder(reached[0].form.pattern())
        } else {
            masked_line(text, start, &reached)
        };
        masked.insert(line);
    }
    masked
}

/// `text`, which starts at byte `start` of its source, with each span's part of it
/// replaced, its indentation kept.
fn masked_line(text: &str, start: usize, spans: &[&SecretSpan]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for span in spans {
        let lo = text.floor_char_boundary(span.range.start.saturating_sub(start).max(from));
        let hi = text.ceil_char_boundary(span.range.end.saturating_sub(start).min(text.len()));
        let hidden = &text[lo..hi];
        let lo = lo + (hidden.len() - hidden.trim_start().len());
        out.push_str(&text[from..lo]);
        if !hidden.trim().is_empty() {
            out.push_str(&redaction_placeholder(span.form.pattern()));
        }
        from = hi;
    }
    out.push_str(&text[from..]);
    out
}
