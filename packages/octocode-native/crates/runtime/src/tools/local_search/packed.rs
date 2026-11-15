//! Bounded in-memory continuation scans. Ordinary rows carry source anchors,
//! exact prices and duplicate equivalence classes, without snippet text. Class
//! IDs preserve equality and ranking; they are not source identities. The
//! initial snapshot hashes full value digests and source digests separately.
//! Other views retain their values. No scan is written to disk.
//!
//! A scan is kept in one of two shapes, both deflated only when the page's
//! cursor keeps the scan (a one-page answer compresses nothing):
//!
//! - **Whole** ([`Encoded::encode_checked`]): every file in scan order, one
//!   deflate stream, encoded before the page reorders it. A continuation
//!   inflates and decodes all of it and cuts its page again. Classification,
//!   list and match-page grid pages keep this shape: a classification
//!   filters rows before its cut, a list page shows no rows, and a grid is
//!   reached by no public call.
//! - **Seekable** ([`FrameWriter`]): one frame per page entry of the
//!   page's streamed cut, in page order, each its own deflate stream, with an index of
//!   where each frame sits and which entries make each page ([`PageIndex`]).
//!   A continuation inflates and decodes only its own page's frames.
//!
//! ## Page-entry frame
//!
//! One frame holds one file entry of one page, raw bytes in the packed
//! encoding ([`write`]: LEB128 numbers, length-prefixed values):
//!
//! 1. the file as [`write::Writer::file`] writes it (path, counts, source
//!    stamps, line count and digest), holding only the entry's rows, in rank order;
//!    deferred values' duplicate classes are numbered per file across all
//!    of its frames;
//! 2. the entry's facts ([`super::page_view::EntryFacts`]): the file's row
//!    total, the entry's rank range and the neighbouring context windows;
//! 3. the entry's rows whose value redaction changed (indexes into the
//!    entry), and whether the file had a key-block scan.
//!
//! A frame never splits a value, so a long line is many frames of one file
//! and Unicode is never cut. The frame layout is local to localSearch; a
//! tool that pages a cursor snapshot the same way can reuse it.
//!
//! The encoding itself is written by [`write`] and read back by [`read`].

mod read;
mod write;

use octocode_engine::types::{TextSearchFile, TextSearchMatch, TextSearchResult, TextSearchStats};
use read::Reader;
use std::io::{Read, Write};
pub use write::Identity;
use write::Writer;

/// Which page entries a seekable scan's pages are made of, and the totals
/// a page reports, so a continuation cuts nothing again. Only the default
/// (streamed) layout is seekable: a match-page grid is reached by no
/// public call (`matchPage` is private progress, and only a grid page
/// issues a next match page), so a grid scan is kept whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PageIndex {
    /// Page `p` is the frames `pages[p - 1]`.
    pub(super) pages: Vec<std::ops::Range<u32>>,
    /// Page `p` starts at row `starts[p - 1]` of every file's rows.
    pub(super) starts: Vec<u32>,
    /// Files the scan matched.
    pub(super) total_files: u32,
    /// Rows of every file.
    pub(super) total_rows: u32,
    /// Rows one match page holds (see `Layout::matches_per`).
    pub(super) matches_per: u32,
    /// Some file has more rows than one match page.
    pub(super) over_matches_per: bool,
}

impl PageIndex {
    fn weight(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.pages.len() * std::mem::size_of::<std::ops::Range<u32>>()
            + self.starts.len() * std::mem::size_of::<u32>()
    }
}

/// One page entry as a frame holds it.
pub(super) struct Entry {
    /// The file with the entry's rows only.
    pub(super) file: TextSearchFile,
    pub(super) facts: super::page_view::EntryFacts,
    /// Rows of the entry whose value redaction changed.
    pub(super) redacted: Vec<u32>,
    /// The file's snippets looked like key material and its key-block scan
    /// read its bytes.
    pub(super) key_scan: bool,
}

/// Where a frame's raw bytes end, and the file (by position in the
/// encoding's file order) it is an entry of.
#[derive(Clone, Copy, Debug)]
struct FrameEnd {
    end: usize,
    file: u32,
}

/// A scan in the packed encoding, not yet deflated: what a fresh page holds
/// until its cursor decides whether the scan is kept.
pub struct Encoded {
    raw: Vec<u8>,
    files: usize,
    stats: TextSearchStats,
    /// A seekable scan's frames and pages; `None` for a whole one.
    seek: Option<(Vec<FrameEnd>, PageIndex)>,
}

/// Writes a seekable scan's frames ([`Encoded::frames`]).
pub(super) struct FrameWriter {
    out: Writer<Vec<u8>>,
    ends: Vec<FrameEnd>,
    files: usize,
    /// Each file's deferred-value classes, shared by its frames.
    classes: std::collections::HashMap<usize, std::collections::HashMap<[u8; 32], u64>>,
}

impl FrameWriter {
    /// A writer of `files` files' frames that refuses to pass `max_raw` raw
    /// bytes (a page would never inflate more than the whole scan).
    pub(super) fn new(files: usize, max_raw: usize) -> Self {
        Self {
            out: Writer::new(Vec::new(), max_raw),
            ends: Vec::new(),
            files,
            classes: std::collections::HashMap::new(),
        }
    }

    /// Frames written so far.
    pub(super) fn len(&self) -> u32 {
        octocode_engine::num::saturating_u32(self.ends.len())
    }

    /// One entry of file `index` (`file`, showing `rows`), its facts and
    /// flags; `None` past the bound or once `cancelled`.
    pub(super) fn entry(
        &mut self,
        (index, file): (usize, &TextSearchFile),
        rows: &[TextSearchMatch],
        facts: &super::page_view::EntryFacts,
        (redacted, key_scan): (&[u32], bool),
        cancelled: &dyn Fn() -> bool,
    ) -> Option<()> {
        let classes = self.classes.entry(index).or_default();
        self.out.file_rows(file, rows, true, classes, cancelled)?;
        self.out.facts(facts)?;
        self.out.number(redacted.len() as u64)?;
        for row in redacted {
            self.out.number(u64::from(*row))?;
        }
        self.out.number(u64::from(key_scan))?;
        self.ends.push(FrameEnd {
            end: self.out.written(),
            file: u32::try_from(index).ok()?,
        });
        Some(())
    }

    /// The frames written, as a seekable encoding with `index` and `stats`.
    pub(super) fn finish(self, index: PageIndex, stats: TextSearchStats) -> Encoded {
        Encoded {
            raw: self.out.into_sink(),
            files: self.files,
            stats,
            seek: Some((self.ends, index)),
        }
    }
}

impl Encoded {
    /// `scan` encoded whole, or `None` when its files take more than
    /// `max_raw` bytes (a page would inflate that much to read it back).
    /// Encoding stops at the file that passes the bound, so a scan too large
    /// to keep costs no more than the bound to refuse.
    pub fn encode_checked(
        scan: &TextSearchResult,
        max_raw: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Self> {
        let mut out = Writer::new(Vec::new(), max_raw);
        for file in &scan.files {
            out.file(file, true, cancelled)?;
        }
        Some(Self {
            raw: out.into_sink(),
            files: scan.files.len(),
            stats: scan.stats.clone(),
            seek: None,
        })
    }

    /// Whether a continuation reads this scan by page.
    pub(super) fn seekable(&self) -> bool {
        self.seek.is_some()
    }

    /// What storing the scan needs of each file, in order: its path and
    /// source. A file whose source the search hashed has no hits here (see
    /// [`Reader::sourced_file`]), so the scan is never decoded whole beside
    /// the buffer it was encoded into; a file without one keeps every hit
    /// of every frame it has. `None` when the buffer is not exactly this
    /// encoding, or `cancelled`.
    pub fn sourced_files(&self, cancelled: &dyn Fn() -> bool) -> Option<Vec<TextSearchFile>> {
        let Some((ends, _)) = &self.seek else {
            let mut reader = Reader::new(&self.raw);
            let files = (0..self.files)
                .map(|_| reader.sourced_file(cancelled))
                .collect::<Option<Vec<_>>>()?;
            return reader.finished().then_some(files);
        };
        let mut files: Vec<Option<TextSearchFile>> = vec![None; self.files];
        let mut start = 0;
        for frame in ends {
            let mut reader = Reader::new(self.raw.get(start..frame.end)?);
            start = frame.end;
            let slot = files.get_mut(frame.file as usize)?;
            let file = reader.sourced_file(cancelled)?;
            reader.entry_tail()?;
            if !reader.finished() {
                return None;
            }
            match slot {
                Some(kept) => kept.matches.extend(file.matches),
                None => *slot = Some(file),
            }
        }
        (start == self.raw.len()).then_some(())?;
        files.into_iter().collect()
    }

    /// Deflated for storage, or `None` when the deflated bytes pass
    /// `max_packed` (checked as they grow, so an oversized scan stops early).
    pub fn deflate_checked(
        self,
        max_packed: usize,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Packed> {
        let (ends, seek) = match self.seek {
            Some((ends, index)) => {
                let files = ends.iter().map(|frame| frame.file).collect::<Vec<_>>();
                (
                    ends.iter().map(|frame| frame.end).collect(),
                    Some((files, index)),
                )
            }
            None => (vec![self.raw.len()], None),
        };
        let mut bytes = Vec::new();
        let mut frames = Vec::with_capacity(ends.len());
        let mut start = 0;
        for end in ends {
            let raw = self.raw.get(start..end)?;
            let at = bytes.len();
            let mut encoder = flate2::write::DeflateEncoder::new(
                std::mem::take(&mut bytes),
                flate2::Compression::fast(),
            );
            for block in raw.chunks(64 * 1024) {
                if cancelled() {
                    return None;
                }
                encoder.write_all(block).ok()?;
                // The deflated bytes so far only grow.
                (encoder.get_ref().len() <= max_packed).then_some(())?;
            }
            bytes = encoder.finish().ok()?;
            (bytes.len() <= max_packed).then_some(())?;
            frames.push(Frame {
                at,
                len: bytes.len() - at,
                raw: raw.len(),
            });
            start = end;
        }
        #[cfg(test)]
        DEFLATED.with(|deflated| deflated.set(deflated.get() + self.raw.len()));
        Some(Packed {
            bytes,
            frames,
            raw_len: self.raw.len(),
            files: self.files,
            stats: self.stats,
            seek,
        })
    }
}

/// Where one frame sits in [`Packed::bytes`], and its raw length.
#[derive(Clone, Copy, Debug)]
struct Frame {
    at: usize,
    len: usize,
    raw: usize,
}

/// A scan deflated by [`Encoded::deflate_checked`].
pub struct Packed {
    /// The deflated frames, one after another.
    bytes: Vec<u8>,
    frames: Vec<Frame>,
    /// Bytes every frame takes unpacked.
    raw_len: usize,
    files: usize,
    stats: TextSearchStats,
    /// A seekable scan's file of each frame, and its pages.
    seek: Option<(Vec<u32>, PageIndex)>,
}

impl Packed {
    /// `scan` encoded within `max_raw` bytes and deflated within
    /// `max_packed` (see [`Encoded`]).
    #[cfg(test)]
    pub fn pack(scan: &TextSearchResult, max_raw: usize, max_packed: usize) -> Option<Self> {
        Encoded::encode_checked(scan, max_raw, &|| false)?.deflate_checked(max_packed, &|| false)
    }

    /// The scan this was packed from.
    #[cfg(test)]
    pub fn unpack(&self) -> Option<TextSearchResult> {
        self.unpack_checked(&|| false)
    }

    /// The whole scan a whole encoding holds; `None` for a seekable one.
    pub fn unpack_checked(&self, cancelled: &dyn Fn() -> bool) -> Option<TextSearchResult> {
        if self.seek.is_some() {
            return None;
        }
        // A whole encoding is one frame.
        let raw = self.inflate(*self.frames.first()?, cancelled)?;
        (raw.len() == self.raw_len).then_some(())?;
        decode(&raw, self.files, &self.stats, cancelled)
    }

    /// A seekable scan's pages.
    pub(super) fn index(&self) -> Option<&PageIndex> {
        self.seek.as_ref().map(|(_, index)| index)
    }

    pub(super) fn stats(&self) -> &TextSearchStats {
        &self.stats
    }

    /// Frame `at` of a seekable scan, inflated alone.
    pub(super) fn entry(&self, at: u32, cancelled: &dyn Fn() -> bool) -> Option<Entry> {
        self.seek.as_ref()?;
        let raw = self.inflate(*self.frames.get(at as usize)?, cancelled)?;
        let mut reader = Reader::new(&raw);
        let entry = reader.entry(cancelled)?;
        reader.finished().then_some(entry)
    }

    fn inflate(&self, frame: Frame, cancelled: &dyn Fn() -> bool) -> Option<Vec<u8>> {
        let deflated = self.bytes.get(frame.at..frame.at.checked_add(frame.len)?)?;
        let mut raw = Vec::with_capacity(frame.raw);
        let mut input = flate2::read::DeflateDecoder::new(deflated).take(frame.raw as u64);
        let mut block = vec![0; 65536.min(frame.raw.max(1))];
        loop {
            if cancelled() {
                return None;
            }
            let n = input.read(&mut block).ok()?;
            if n == 0 {
                break;
            }
            raw.extend_from_slice(&block[..n]);
        }
        (raw.len() == frame.raw).then_some(())?;
        #[cfg(test)]
        INFLATED.with(|inflated| inflated.set(inflated.get() + raw.len()));
        Some(raw)
    }

    /// Bytes this keeps in memory: the deflated frames and their index.
    pub fn weight(&self) -> usize {
        self.bytes.len()
            + self.frames.len() * std::mem::size_of::<Frame>()
            + self.seek.as_ref().map_or(0, |(files, index)| {
                files.len() * std::mem::size_of::<u32>() + index.weight()
            })
    }
}

/// The `files` files encoded in `raw`, with `stats`; `None` when `raw` is
/// not exactly that encoding.
fn decode(
    raw: &[u8],
    files: usize,
    stats: &TextSearchStats,
    cancelled: &dyn Fn() -> bool,
) -> Option<TextSearchResult> {
    let mut reader = Reader::new(raw);
    let files = (0..files)
        .map(|_| reader.file(cancelled))
        .collect::<Option<Vec<_>>>()?;
    reader.finished().then(|| TextSearchResult {
        files,
        stats: stats.clone(),
    })
}

#[cfg(test)]
thread_local! {
    /// Raw bytes this thread deflated, for tests that pin when a scan is packed.
    static DEFLATED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Raw bytes this thread inflated, for tests that pin what a page reads.
    static INFLATED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Raw bytes inflated on this thread so far.
#[cfg(test)]
pub fn inflated_on_this_thread() -> usize {
    INFLATED.with(std::cell::Cell::get)
}

/// Raw bytes deflated on this thread so far.
#[cfg(test)]
pub fn deflated_on_this_thread() -> usize {
    DEFLATED.with(std::cell::Cell::get)
}

/// Hits decoded into rows on this thread so far.
#[cfg(test)]
pub fn decoded_hits_on_this_thread() -> usize {
    read::decoded_hits_on_this_thread()
}

#[cfg(test)]
mod tests;
