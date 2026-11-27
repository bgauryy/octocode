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
//!   and list pages keep this shape: a classification filters rows before
//!   its cut, and a list page shows no rows.
//! - **Seekable** ([`FrameWriter`]): one frame per page entry of the
//!   page's streamed cut, in page order, with an index of where each frame
//!   sits and which entries make each page ([`PageIndex`]). Consecutive
//!   frames share a deflate stream of about [`STREAM_RAW_BYTES`] raw bytes
//!   (a large frame takes one alone). A continuation inflates only the
//!   streams of its own page's frames and decodes only those frames.
//!
//! ## Page-entry frame
//!
//! One frame holds one file entry of one page, raw bytes in the packed
//! encoding ([`write`](mod@write): LEB128 numbers, length-prefixed values):
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
//! The encoding itself is written by [`write`](mod@write) and read back by [`read`].

mod read;
mod write;

use octocode_engine::types::{TextSearchFile, TextSearchMatch, TextSearchResult, TextSearchStats};
use read::Reader;
use std::io::{Read, Write};
pub(super) use write::Identity;
use write::Writer;

/// Which page entries a seekable scan's pages are made of, and the totals
/// a page reports, so a continuation cuts nothing again. Only the default
/// (streamed) layout is seekable; a list view is kept whole.
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
pub(super) struct Encoded {
    raw: Vec<u8>,
    files: usize,
    stats: TextSearchStats,
    /// A seekable scan's frames and pages; `None` for a whole one.
    seek: Option<(Vec<FrameEnd>, PageIndex)>,
}

/// Raw bytes one deflate stream takes in before the next frame starts a
/// new one; a page inflates whole streams.
const STREAM_RAW_BYTES: usize = 64 * 1024;

/// Writes a seekable scan's frames (`Encoded::frames`).
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
    pub(super) fn encode_checked(
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
    pub(super) fn sourced_files(
        &self,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Vec<TextSearchFile>> {
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
    /// Consecutive frames share a deflate stream until it has taken in
    /// [`STREAM_RAW_BYTES`]: a stream's setup and block tables cost the same
    /// for a one-row frame as for a large one, so a scan of one-row pages
    /// starts as many streams as its bytes fill, not one per frame.
    pub(super) fn deflate_checked(
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
        let mut out = Deflating::new(ends.len());
        let mut start = 0;
        for end in ends {
            out.frame(self.raw.get(start..end)?, max_packed, cancelled)?;
            start = end;
        }
        let (bytes, streams, frames) = out.finish(max_packed)?;
        #[cfg(test)]
        DEFLATED.with(|deflated| deflated.set(deflated.get() + self.raw.len()));
        Some(Packed {
            bytes,
            streams,
            frames,
            raw_len: self.raw.len(),
            files: self.files,
            stats: self.stats,
            seek,
        })
    }
}

/// Frames being deflated into shared streams ([`Encoded::deflate_checked`]).
struct Deflating {
    /// The closed streams, one after another.
    bytes: Vec<u8>,
    streams: Vec<Stream>,
    frames: Vec<Frame>,
    /// The stream taking frames in, if any.
    open: Option<OpenStream>,
}

/// A deflate stream still taking frames in.
struct OpenStream {
    encoder: flate2::write::DeflateEncoder<Vec<u8>>,
    /// Where it starts in the deflated bytes.
    at: usize,
    /// Raw bytes it has taken in.
    raw: usize,
}

impl Deflating {
    fn new(frames: usize) -> Self {
        Self {
            bytes: Vec::new(),
            streams: Vec::new(),
            frames: Vec::with_capacity(frames),
            open: None,
        }
    }

    /// Deflate the next frame's `raw` bytes, into the open stream unless it
    /// has taken in [`STREAM_RAW_BYTES`] already.
    fn frame(&mut self, raw: &[u8], max_packed: usize, cancelled: &dyn Fn() -> bool) -> Option<()> {
        if self
            .open
            .as_ref()
            .is_some_and(|open| open.raw >= STREAM_RAW_BYTES)
        {
            self.close(max_packed)?;
        }
        let stream = u32::try_from(self.streams.len()).ok()?;
        let open = match &mut self.open {
            Some(open) => open,
            empty @ None => {
                #[cfg(test)]
                STREAMS.with(|streams| streams.set(streams.get() + 1));
                let at = self.bytes.len();
                empty.insert(OpenStream {
                    encoder: flate2::write::DeflateEncoder::new(
                        std::mem::take(&mut self.bytes),
                        flate2::Compression::fast(),
                    ),
                    at,
                    raw: 0,
                })
            }
        };
        self.frames.push(Frame {
            stream,
            start: open.raw,
            raw: raw.len(),
        });
        for block in raw.chunks(STREAM_RAW_BYTES) {
            if cancelled() {
                return None;
            }
            open.encoder.write_all(block).ok()?;
            // The deflated bytes so far only grow.
            (open.encoder.get_ref().len() <= max_packed).then_some(())?;
        }
        open.raw += raw.len();
        Some(())
    }

    /// Finish the open stream, if any.
    fn close(&mut self, max_packed: usize) -> Option<()> {
        let Some(open) = self.open.take() else {
            return Some(());
        };
        self.bytes = open.encoder.finish().ok()?;
        (self.bytes.len() <= max_packed).then_some(())?;
        self.streams.push(Stream {
            at: open.at,
            len: self.bytes.len() - open.at,
            raw: open.raw,
        });
        Some(())
    }

    /// The deflated bytes, their streams and the frames they hold.
    fn finish(mut self, max_packed: usize) -> Option<(Vec<u8>, Vec<Stream>, Vec<Frame>)> {
        self.close(max_packed)?;
        Some((self.bytes, self.streams, self.frames))
    }
}

/// Where one deflate stream sits in [`Packed::bytes`], and its raw length.
#[derive(Clone, Copy, Debug)]
struct Stream {
    at: usize,
    len: usize,
    raw: usize,
}

/// Where one frame's raw bytes sit: its stream, and its span in that
/// stream's raw bytes.
#[derive(Clone, Copy, Debug)]
struct Frame {
    stream: u32,
    start: usize,
    raw: usize,
}

/// A scan deflated by [`Encoded::deflate_checked`].
pub(super) struct Packed {
    /// The deflated streams, one after another.
    bytes: Vec<u8>,
    streams: Vec<Stream>,
    /// Each frame's place in `streams`.
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
    pub(super) fn pack(scan: &TextSearchResult, max_raw: usize, max_packed: usize) -> Option<Self> {
        Encoded::encode_checked(scan, max_raw, &|| false)?.deflate_checked(max_packed, &|| false)
    }

    /// The scan this was packed from.
    #[cfg(test)]
    pub(super) fn unpack(&self) -> Option<TextSearchResult> {
        self.unpack_checked(&|| false)
    }

    /// The whole scan a whole encoding holds; `None` for a seekable one.
    pub(super) fn unpack_checked(&self, cancelled: &dyn Fn() -> bool) -> Option<TextSearchResult> {
        if self.seek.is_some() {
            return None;
        }
        // A whole encoding is one frame in one stream.
        let raw = self.inflate(*self.streams.first()?, cancelled)?;
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

    /// Frame `at` of a seekable scan, its stream inflated alone.
    #[cfg(test)]
    pub(super) fn entry(&self, at: u32, cancelled: &dyn Fn() -> bool) -> Option<Entry> {
        self.entries(at..at + 1, cancelled)?.pop()
    }

    /// Frames `range` of a seekable scan (one page's), each stream they sit
    /// in inflated once.
    pub(super) fn entries(
        &self,
        range: std::ops::Range<u32>,
        cancelled: &dyn Fn() -> bool,
    ) -> Option<Vec<Entry>> {
        self.seek.as_ref()?;
        let mut inflated: Option<(u32, Vec<u8>)> = None;
        let mut entries = Vec::with_capacity(range.len());
        for at in range {
            let frame = *self.frames.get(at as usize)?;
            if inflated
                .as_ref()
                .is_none_or(|(stream, _)| *stream != frame.stream)
            {
                let stream = *self.streams.get(frame.stream as usize)?;
                inflated = Some((frame.stream, self.inflate(stream, cancelled)?));
            }
            let (_, raw) = inflated.as_ref()?;
            let raw = raw.get(frame.start..frame.start.checked_add(frame.raw)?)?;
            let mut reader = Reader::new(raw);
            let entry = reader.entry(cancelled)?;
            reader.finished().then_some(())?;
            entries.push(entry);
        }
        Some(entries)
    }

    fn inflate(&self, stream: Stream, cancelled: &dyn Fn() -> bool) -> Option<Vec<u8>> {
        let deflated = self
            .bytes
            .get(stream.at..stream.at.checked_add(stream.len)?)?;
        let mut raw = Vec::with_capacity(stream.raw);
        let mut input = flate2::read::DeflateDecoder::new(deflated).take(stream.raw as u64);
        let mut block = vec![0; 65536.min(stream.raw.max(1))];
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
        (raw.len() == stream.raw).then_some(())?;
        #[cfg(test)]
        INFLATED.with(|inflated| inflated.set(inflated.get() + raw.len()));
        Some(raw)
    }

    /// Bytes this keeps in memory: the deflated streams and their index.
    pub(super) fn weight(&self) -> usize {
        self.bytes.len()
            + self.streams.len() * std::mem::size_of::<Stream>()
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
    /// Deflate streams this thread started, for tests that pin what one costs.
    static STREAMS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Raw bytes this thread deflated, for tests that pin when a scan is packed.
    static DEFLATED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Raw bytes this thread inflated, for tests that pin what a page reads.
    static INFLATED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Raw bytes inflated on this thread so far.
#[cfg(test)]
pub(super) fn inflated_on_this_thread() -> usize {
    INFLATED.with(std::cell::Cell::get)
}

/// Deflate streams started on this thread so far.
#[cfg(test)]
pub(super) fn streams_on_this_thread() -> usize {
    STREAMS.with(std::cell::Cell::get)
}

/// Raw bytes deflated on this thread so far.
#[cfg(test)]
pub(super) fn deflated_on_this_thread() -> usize {
    DEFLATED.with(std::cell::Cell::get)
}

/// Hits decoded into rows on this thread so far.
#[cfg(test)]
pub(super) fn decoded_hits_on_this_thread() -> usize {
    read::decoded_hits_on_this_thread()
}

#[cfg(test)]
mod tests;
