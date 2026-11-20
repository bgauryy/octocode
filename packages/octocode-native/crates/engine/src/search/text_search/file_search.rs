//! Searching one opened file: the searcher, the tapped read, the binary prefix re-search, and the
//! kept file's digest.

use crate::num::saturating_usize;
mod prefix;
mod read_digest;
#[cfg(any(test, feature = "test-hooks"))]
mod read_race;
use prefix::is_leading_binary;
pub(super) use prefix::read_prefix;
pub(super) use read_digest::ReadDigest;
#[cfg(any(test, feature = "test-hooks"))]
pub use read_race::{DURING_SEARCH_READ, ReadRace};

use super::file_terms::{CopiedRead, FILE_TERMS_MAX_BYTES};
use super::mode::MatchWork;
use super::sink::CollectSink;
use super::state::{Charge, Held};
use crate::search::text_parser::FileEntry;
use crate::types::{SearchContext, SearchedSource, TextSearchMatch, TextSearchOptions};
use grep_matcher::Matcher;
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder};
use sha2::{Digest as _, Sha256};
use std::path::Path;
use std::sync::atomic::AtomicBool;

/// Hard heap ceiling for a single `Searcher` pass. `grep-searcher` buffers a
/// whole line (or multiline block) before matching; without a cap a huge single
/// line can allocate unbounded. Files above the search's byte ceiling
/// (`collect::byte_ceiling`) are already skipped, so this only bounds a
/// within-ceiling pathological line; exceeding it fails that file (recorded as
/// an error), never the whole search.
pub(super) const SEARCH_HEAP_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// A file of at most this many bytes holds no more than that in the line
/// buffer, so its read is not admitted against the memory budget one by one.
const LINE_BUFFER_UNIT_BYTES: u64 = 1024 * 1024;

/// Whether the file's first [`LINE_BUFFER_UNIT_BYTES`] hold no line break and
/// no NUL: text with a line past the buffer's growth (a binary file quits at
/// its first NUL, which the line buffer finds without holding the file).
fn opens_with_a_long_line(file: &std::fs::File) -> std::io::Result<bool> {
    let head = read_prefix(file, LINE_BUFFER_UNIT_BYTES)?;
    std::io::Seek::seek(&mut &*file, std::io::SeekFrom::Start(0))?;
    Ok(!head.contains(&b'\n') && !head.contains(&0))
}

/// Largest pre-NUL prefix re-searched when a file is quit as binary. The
/// searcher drops the whole buffer that holds the first NUL, so matches before
/// it would otherwise be lost; beyond this offset the earlier buffers were
/// already searched and reported normally.
const MAX_BINARY_PREFIX_BYTES: u64 = 8 * 1024 * 1024;

pub(super) fn build_searcher(
    opts: &TextSearchOptions,
    context: SearchContext,
    binary: BinaryDetection,
) -> Searcher {
    let mut sb = SearcherBuilder::new();
    sb.line_number(true)
        .binary_detection(binary)
        // Bound the per-file line/block buffer so a pathological within-ceiling
        // single-line file cannot allocate unbounded.
        .heap_limit(Some(SEARCH_HEAP_LIMIT_BYTES))
        // Sniff a BOM so BOM-prefixed UTF-8/UTF-16 files are decoded rather than
        // mis-detected as binary on their first NUL. Default is on; set
        // explicitly so the behavior is not silently lost on a builder change.
        .bom_sniffing(true);
    if opts.multiline.unwrap_or(false) {
        sb.multi_line(true);
    }
    if opts.invert_match.unwrap_or(false) {
        sb.invert_match(true);
    }
    sb.before_context(context.before as usize);
    sb.after_context(context.after as usize);
    sb.build()
}

/// The searcher's read of one opened file. Every byte the search reads
/// passes through here once, so the file-level terms' bytes and the kept
/// file's digest come from that read instead of a second one.
struct SourceTap<'f> {
    file: &'f std::fs::File,
    /// The file's path, for the tests' writes made while it is read.
    #[cfg(any(test, feature = "test-hooks"))]
    path: &'f Path,
    /// Every byte read, up to [`FILE_TERMS_MAX_BYTES`], when file-level terms
    /// are checked against this same read.
    copy: Option<Vec<u8>>,
    /// The digest of the bytes read, for the file if it is kept.
    digest: &'f mut ReadDigest,
    /// A read returned no bytes: the file was read to its end.
    eof: bool,
    /// Ends the read once the caller cancelled, so a large file is not read
    /// to its end after the caller left. A search stopped by its own bound
    /// reads on: the matches kept before it are the file's.
    cancelled: Option<&'f (dyn Fn() -> bool + Sync)>,
}

/// The digest of a kept file, read whole from offset 0 after its search,
/// with the proof that the hashed bytes are the opened file whole: `None`
/// when its length differs from `meta`'s or a write moved its stamp since
/// ([`crate::text::unchanged_since`]). It binds a file
/// searched as the text its judge served, whose bytes only the judge read
/// ([`FileSearcher::searched_source`]); a file searched as its own bytes is
/// bound to the read that searched them.
pub(super) fn hashed_source(
    file: &std::fs::File,
    meta: &std::fs::Metadata,
) -> Option<SearchedSource> {
    use std::io::{Read, Seek, SeekFrom};
    thread_local! {
        /// One read buffer per search worker, not one per kept file.
        static HASH_BUFFER: std::cell::RefCell<Vec<u8>> =
            std::cell::RefCell::new(vec![0u8; 64 * 1024]);
    }
    let mut hasher = Sha256::new();
    let mut len = 0u64;
    let mut lines = crate::text::LineCount::default();
    let mut utf16 = false;
    let mut handle = file;
    handle.seek(SeekFrom::Start(0)).ok()?;
    HASH_BUFFER.with_borrow_mut(|buf| {
        loop {
            match handle.read(buf) {
                Ok(0) => return Some(()),
                Ok(read) => {
                    if len == 0 {
                        utf16 = crate::text::has_utf16_bom(&buf[..read]);
                    }
                    hasher.update(&buf[..read]);
                    lines.feed(&buf[..read]);
                    len += read as u64;
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
    })?;
    (len == meta.len() && crate::text::unchanged_since(file, meta)).then(|| SearchedSource {
        size: len,
        modified: meta.modified().ok(),
        digest: hasher.finalize().into(),
        lines: (!utf16).then(|| crate::num::saturating_u32(lines.lines())),
    })
}

impl<'f> SourceTap<'f> {
    fn new(
        (file, path): (&'f std::fs::File, &'f Path),
        copy: bool,
        cancelled: Option<&'f (dyn Fn() -> bool + Sync)>,
        digest: &'f mut ReadDigest,
    ) -> Self {
        digest.restart();
        #[cfg(not(any(test, feature = "test-hooks")))]
        let _ = path;
        Self {
            file,
            #[cfg(any(test, feature = "test-hooks"))]
            path,
            copy: copy.then(Vec::new),
            digest,
            eof: false,
            cancelled,
        }
    }
}

/// Why a read of a file ended: the caller cancelled. It is no
/// `Interrupted` error, which the searcher's decoder retries, so the read
/// would never end.
#[derive(Debug)]
struct StoppedRead;

impl std::fmt::Display for StoppedRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the search was stopped")
    }
}

impl std::error::Error for StoppedRead {}

pub(super) fn stopped_read() -> std::io::Error {
    std::io::Error::other(StoppedRead)
}

/// Whether `error` is the end of a read the caller cancelled.
pub(super) fn is_stopped_read(error: &std::io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.downcast_ref::<StoppedRead>().is_some())
}

impl std::io::Read for SourceTap<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cancelled.is_some_and(|gone| gone()) {
            return Err(stopped_read());
        }
        let mut handle = self.file;
        let read = handle.read(buf)?;
        if read == 0 {
            if !buf.is_empty() {
                self.eof = true;
                self.digest.ended();
            }
            return Ok(0);
        }
        #[cfg(any(test, feature = "test-hooks"))]
        read_race::during_read(
            self.path,
            std::io::Seek::stream_position(&mut self.file).unwrap_or(0),
        );
        let bytes = &buf[..read];
        self.digest.feed(bytes);
        if let Some(copy) = &mut self.copy {
            let room = saturating_usize(FILE_TERMS_MAX_BYTES) - copy.len();
            copy.extend_from_slice(&bytes[..read.min(room)]);
        }
        Ok(read)
    }
}

/// Result of searching one file.
pub(super) struct FileOutcome {
    /// What the kept rows hold of the search's bound: released if the
    /// outcome is dropped, kept with the file's record.
    pub(super) charge: Charge,
    pub(super) entry: FileEntry,
    pub(super) submatches: u32,
    pub(super) matched_lines: u32,
    pub(super) om_matches: Vec<TextSearchMatch>,
    pub(super) binary: bool,
    /// The first NUL came before any text: nothing text-searchable was lost.
    pub(super) opaque: bool,
    /// The searched bytes, when file-level terms are checked.
    pub(super) copied: Option<CopiedRead>,
    /// A returned line was not UTF-8 (see [`CollectSink::not_utf8`]).
    pub(super) not_utf8: bool,
    /// A matched file starts with a UTF-8 byte-order mark the searcher
    /// dropped: line 1's columns and byte offsets count the text after it.
    pub(super) utf8_bom: bool,
    /// Per searched term (see [`CollectSink::term_counts`]).
    pub(super) term_counts: Vec<u32>,
    /// The values are of the text the search's judge served, which only the
    /// judge read of the file ([`FileSearcher::searched_source`]).
    pub(super) served: bool,
    /// Masked spans of the text the file was searched as (0: its own bytes).
    pub(super) masked: u32,
    /// Matched lines a test-code scope left out.
    pub(super) test_lines_excluded: u32,
    /// The file's test-only spans, when a hit read them.
    pub(super) test_spans: Option<std::sync::Arc<Vec<[u32; 2]>>>,
}

impl<M: Matcher> From<CollectSink<'_, M>> for FileOutcome {
    fn from(sink: CollectSink<'_, M>) -> Self {
        Self {
            charge: sink.charge,
            entry: sink.entry,
            submatches: sink.submatches,
            matched_lines: sink.matched_lines,
            om_matches: sink.om_matches,
            binary: sink.binary_offset.is_some(),
            opaque: sink.binary_offset == Some(0),
            copied: None,
            not_utf8: sink.not_utf8,
            utf8_bom: false,
            term_counts: sink.term_counts,
            served: false,
            masked: 0,
            test_lines_excluded: sink.test_lines_excluded,
            test_spans: sink.test_spans,
        }
    }
}

/// What stops work on a file: the search's stop flag, and the caller's
/// cancellation where it stops files mid-way.
#[derive(Clone, Copy)]
struct Halt<'a> {
    stop: &'a AtomicBool,
    cancelled: Option<&'a (dyn Fn() -> bool + Sync)>,
}

impl Halt<'_> {
    fn is_set(&self) -> bool {
        self.stop.load(std::sync::atomic::Ordering::Relaxed)
            || self.cancelled.is_some_and(|cancelled| cancelled())
    }
}

/// The text a judge served, with the bytes it holds admitted until dropped.
pub(super) struct JudgedText {
    pub(super) served: crate::types::ServedText,
    _held: crate::memory::Ticket<'static>,
}

/// Bytes a search runs over whole, with the ranges of them a judge wrote
/// ([`crate::types::ServedText::written`]; empty for a file's own bytes).
#[derive(Clone, Copy)]
pub(super) struct Searched<'a> {
    pub(super) bytes: &'a [u8],
    pub(super) written: &'a [std::ops::Range<usize>],
}

impl<'a> Searched<'a> {
    /// A file's own bytes: nothing of them was written.
    pub(super) fn own(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            written: &[],
        }
    }
}

impl<'a> From<&'a crate::types::ServedText> for Searched<'a> {
    fn from(served: &'a crate::types::ServedText) -> Self {
        Self {
            bytes: &served.text,
            written: &served.written,
        }
    }
}

mod held_whole;
use held_whole::outgrew_the_line_buffer;

/// Per-worker searchers: the walk searcher quits at the first NUL; the prefix
/// searcher (built on first use) re-searches the NUL-free bytes before it.
pub(super) struct FileSearcher<'a, M: Matcher> {
    pub(super) opts: &'a TextSearchOptions,
    pub(super) matcher: &'a M,
    pub(super) searcher: Searcher,
    pub(super) prefix_searcher: Option<Searcher>,
    pub(super) context: SearchContext,
    pub(super) work: MatchWork,
    pub(super) stop: &'a AtomicBool,
    /// The search's bound on what its kept evidence holds.
    pub(super) held: &'a std::sync::Arc<Held>,
    /// Whether the caller cancelled the search, where a cancelled search
    /// stops files mid-way (a stream's does not: its files finish).
    pub(super) cancelled: Option<&'a (dyn Fn() -> bool + Sync)>,
    /// Keep the read bytes for the file-level terms check.
    pub(super) copy: bool,
    /// One matcher per searched term, counted per term by a count view
    /// (empty: no per-term counts).
    pub(super) terms: &'a [M],
    /// Which files the search's judge need not judge.
    pub(super) gate: &'a super::judge_gate::JudgeGate,
    /// What the search's read of the current file took of its bytes.
    pub(super) read: ReadDigest,
}

impl<M: Matcher> FileSearcher<'_, M> {
    /// Search an opened file in one read through a [`SourceTap`]. When it
    /// is quit as binary, the searcher has dropped the whole buffer holding
    /// the NUL, so the NUL-free prefix is searched again from byte 0 and its
    /// matches replace the first pass. A file its judge replaced bytes of
    /// is searched as the text the judge serves ([`crate::types::SourceJudge`]).
    /// When the search's gate allows it, the file's own bytes are searched
    /// first: a file they prove holds no match in the text its judge serves
    /// is not judged ([`super::judge_gate`]), and that search is the answer
    /// too when the judge replaces nothing.
    pub(super) fn search(
        &mut self,
        file: &std::fs::File,
        path: &Path,
    ) -> std::io::Result<FileOutcome> {
        self.read.clear();
        let own = if self.opts.judge.is_some() && self.gate.gates() {
            let own = self.search_read(file, path)?;
            if self
                .gate
                .proves_unmatched(file, own.matched_lines, own.binary)?
            {
                return Ok(own);
            }
            Some(own)
        } else {
            None
        };
        let mut outcome = match self.judged(file, path)? {
            Some(served) => {
                drop(own);
                let work = self.work_for(file)?;
                let mut outcome = self.search_bytes((&served.served).into(), path, work)?;
                outcome.served = true;
                outcome.masked = served.served.masked_spans();
                outcome
            }
            None => match own {
                Some(own) => own,
                None => self.search_read(file, path)?,
            },
        };
        // Only a matched file's values and positions are shown.
        if outcome.matched_lines > 0 {
            let prefix = read_prefix(file, 3)?;
            outcome.utf8_bom =
                !crate::text::has_utf16_bom(&prefix) && crate::text::bom_len(&prefix) > 0;
        }
        Ok(outcome)
    }

    /// The digest a kept `file` (opened with `meta`) carries: of the bytes
    /// its values were made from. A file searched as its own bytes is bound
    /// to the read that searched them, finished from where it stopped. A file
    /// searched as the text its judge served (`served`) was read by the judge
    /// alone: it is bound to a whole read made after the judge, which proves
    /// the file unmoved since it was opened ([`hashed_source`]), and, when the
    /// search read the file before judging it, to that read too: the two
    /// must be the same bytes. `None` when nothing binds the values (a write
    /// moved the file's stamp since it was opened, so the digest is of no
    /// one version, or its rest could not be read before the caller left).
    pub(super) fn searched_source(
        &mut self,
        file: &std::fs::File,
        meta: &std::fs::Metadata,
        served: bool,
    ) -> Option<SearchedSource> {
        let read = self.read.finish(file, self.cancelled);
        if served {
            let whole = hashed_source(file, meta)?;
            return match read {
                Some(read) if read.digest != whole.digest => None,
                _ => Some(whole),
            };
        }
        let read = read?;
        if !crate::text::unchanged_since(file, meta) {
            return None;
        }
        Some(SearchedSource {
            size: read.size,
            modified: meta.modified().ok(),
            digest: read.digest,
            lines: read.lines,
        })
    }

    /// What this search's read of `file` took of it, from its first byte to
    /// its end, by count and SHA-256: the read finished from where it stopped,
    /// as [`Self::searched_source`] binds it, for proving those bytes the
    /// start of a version the file grew to ([`crate::text::Settling::settled_read`]).
    /// `None` when the search hashed no read of its own bytes; after it, no
    /// digest binds the file's values.
    pub(super) fn taken(&mut self, file: &std::fs::File) -> Option<(u64, [u8; 32])> {
        let read = self.read.finish(file, self.cancelled)?;
        Some((read.size, read.digest))
    }

    /// [`Self::search`]'s outcome of `text`, held: the start of one version
    /// of the file at `path` ([`crate::text::Proved::Prefix`]), or the text
    /// its judge serves of those bytes (`served`, [`crate::types::JudgeHeld`]);
    /// `utf16` when those bytes are BOM-marked UTF-16 ([`Self::work_for`]).
    pub(super) fn search_proved(
        &mut self,
        text: Searched<'_>,
        path: &Path,
        (utf16, served): (bool, bool),
    ) -> std::io::Result<FileOutcome> {
        let mut work = self.work;
        work.deferred_line &= !utf16;
        let mut outcome = self.search_bytes(text, path, work)?;
        outcome.served = served;
        Ok(outcome)
    }

    /// Whether this search was stopped, so work on a file ends.
    pub(super) fn halted(&self) -> bool {
        self.halt().is_set()
    }

    fn halt(&self) -> Halt<'_> {
        Halt {
            stop: self.stop,
            cancelled: self.cancelled,
        }
    }

    /// The text the search's judge serves of `file` in place of its bytes
    /// (`None`: no judge, or it replaced nothing), with the bytes it holds
    /// admitted ([`crate::memory::WHOLE_FILES`]) until it is dropped. The
    /// judge is handed the file at its start, wherever an earlier read left
    /// it, and the file is read from its start after.
    pub(super) fn judged(
        &self,
        file: &std::fs::File,
        path: &Path,
    ) -> std::io::Result<Option<JudgedText>> {
        let Some(judge) = &self.opts.judge else {
            return Ok(None);
        };
        // The judge holds the file's raw line, its decoded copy and the text
        // it serves at once (measured at about 2.4 times the file).
        let len = usize::try_from(file.metadata()?.len()).unwrap_or(usize::MAX);
        let mut ticket = self.admit(len.saturating_mul(3))?;
        std::io::Seek::seek(&mut &*file, std::io::SeekFrom::Start(0))?;
        let halt = self.halt();
        let judged = (judge.serve)(path, file, &|| halt.is_set())?;
        std::io::Seek::seek(&mut &*file, std::io::SeekFrom::Start(0))?;
        Ok(judged.map(|served| {
            ticket.shrink_to(served.text.len());
            JudgedText {
                served,
                _held: ticket,
            }
        }))
    }

    /// `bytes` admitted for work that holds them whole, once the others'
    /// have been released; an interrupted error when the search is stopped
    /// while it waits.
    pub(super) fn admit(&self, bytes: usize) -> std::io::Result<crate::memory::Ticket<'static>> {
        crate::memory::WHOLE_FILES
            .acquire(bytes, &|| self.halted())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::Interrupted, "the search was stopped")
            })
    }

    /// The search's work for `file`: a UTF-16 file keeps its line values,
    /// which no deferred re-read of its bytes restates.
    fn work_for(&self, file: &std::fs::File) -> std::io::Result<MatchWork> {
        let mut work = self.work;
        if work.deferred_line {
            let prefix = read_prefix(file, 3)?;
            if crate::text::has_utf16_bom(&prefix) {
                work.deferred_line = false;
            }
            std::io::Seek::seek(&mut &*file, std::io::SeekFrom::Start(0))?;
        }
        Ok(work)
    }

    /// [`Self::search`]'s read of the file.
    fn search_read(&mut self, file: &std::fs::File, path: &Path) -> std::io::Result<FileOutcome> {
        // A file past one unit may grow the line buffer to its bound, so its
        // bytes are admitted until the read ends (a longer line then holds
        // the file whole, admitted separately once this one is released).
        let len = file.metadata()?.len();
        let mut buffered = if len > LINE_BUFFER_UNIT_BYTES {
            let bound = len.min(SEARCH_HEAP_LIMIT_BYTES as u64);
            Some(self.admit(usize::try_from(bound).unwrap_or(usize::MAX))?)
        } else {
            None
        };
        // A file whose first megabyte holds no line break has a line past the
        // buffer's growth: it is held whole at once, not after a chain of
        // doublings that leaves each freed block with the allocator.
        if len > LINE_BUFFER_UNIT_BYTES && opens_with_a_long_line(file)? {
            drop(buffered.take());
            let work = self.work_for(file)?;
            return self.search_read_whole(file, path, work);
        }
        let work = self.work_for(file)?;
        let mut tap = SourceTap::new((file, path), self.copy, self.cancelled, &mut self.read);
        let mut sink = CollectSink::new(self.matcher, work, (self.stop, self.held), self.terms);
        sink.cancelled = self.cancelled;
        sink.test_code = self.opts.test_code.as_ref().map(|scope| (scope, path));
        sink.line_scope = self.opts.line_scope.as_ref().map(|scope| (scope, path));
        sink.source = Some(path);
        sink.inspect_deferred = self
            .opts
            .inspect_deferred
            .as_ref()
            .map(|inspect| (inspect, path));
        // `search_file` reads through this same reader path: memory maps are
        // never used, and multiline reads the whole file under the heap limit.
        if let Err(error) = self
            .searcher
            .search_reader(self.matcher, &mut tap, &mut sink)
        {
            if !outgrew_the_line_buffer(&error) {
                return Err(error);
            }
            drop(buffered.take());
            return self.search_read_whole(file, path, work);
        }
        let offset = sink
            .binary_offset
            .filter(|offset| *offset != 0 && *offset <= MAX_BINARY_PREFIX_BYTES);
        let Some(offset) = offset else {
            let mut outcome = FileOutcome::from(sink);
            outcome.copied = tap.copy.map(|bytes| CopiedRead {
                bytes,
                eof: tap.eof,
                written: Vec::new(),
            });
            return Ok(outcome);
        };
        drop(buffered);
        let copied = tap.copy.map(|bytes| CopiedRead {
            bytes,
            eof: tap.eof,
            written: Vec::new(),
        });
        // The rows come from this second read of the bytes before the NUL:
        // the file's digest is of it, read on from where it ends.
        let prefix = read_prefix(file, offset)?;
        self.read.restart();
        self.read.feed(&prefix);
        if (prefix.len() as u64) < offset {
            self.read.ended();
        }
        let (opts, context) = (self.opts, self.context);
        let prefix_searcher = self
            .prefix_searcher
            .get_or_insert_with(|| build_searcher(opts, context, BinaryDetection::none()));
        let mut prefix_sink =
            CollectSink::new(self.matcher, self.work, (self.stop, self.held), self.terms);
        prefix_sink.cancelled = self.cancelled;
        prefix_sink.test_code = self.opts.test_code.as_ref().map(|scope| (scope, path));
        prefix_sink.line_scope = self.opts.line_scope.as_ref().map(|scope| (scope, path));
        prefix_sink.source = Some(path);
        prefix_sink.inspect_deferred = self
            .opts
            .inspect_deferred
            .as_ref()
            .map(|inspect| (inspect, path));
        prefix_searcher.search_slice(self.matcher, &prefix, &mut prefix_sink)?;
        let mut outcome = FileOutcome::from(prefix_sink);
        outcome.binary = true;
        outcome.opaque = is_leading_binary(&prefix, outcome.matched_lines > 0);
        outcome.copied = copied;
        Ok(outcome)
    }

    /// [`Self::search`] over a file's bytes read once for several matchers,
    /// through the same line buffer, so a NUL ends it where it ends a read of
    /// the file. Only for a search with no per-file scope ([`super::listable`]).
    pub(super) fn search_held(&mut self, text: Searched<'_>) -> std::io::Result<FileOutcome> {
        let Searched { bytes, written } = text;
        let mut sink =
            CollectSink::new(self.matcher, self.work, (self.stop, self.held), self.terms);
        sink.cancelled = self.cancelled;
        sink.written = written;
        // An inverted search keeps the lines the pattern does not match: over
        // the text as served, a line whose only match is a placeholder's
        // spelling would count as matching. It reads the text with what the
        // judge wrote made filler, and its lines from the served text.
        let inverted = self.opts.invert_match.unwrap_or(false) && !written.is_empty();
        let filled = inverted.then(|| super::written::fill(bytes, written));
        if inverted {
            sink.inverted_over = Some(bytes);
        }
        self.searcher
            .search_reader(self.matcher, filled.as_deref().unwrap_or(bytes), &mut sink)?;
        let offset = sink
            .binary_offset
            .filter(|offset| *offset != 0 && *offset <= MAX_BINARY_PREFIX_BYTES);
        let Some(offset) = offset else {
            return Ok(FileOutcome::from(sink));
        };
        let prefix =
            &bytes[..usize::try_from(offset).map_or(bytes.len(), |at| at.min(bytes.len()))];
        let (opts, context) = (self.opts, self.context);
        let prefix_searcher = self
            .prefix_searcher
            .get_or_insert_with(|| build_searcher(opts, context, BinaryDetection::none()));
        let mut prefix_sink =
            CollectSink::new(self.matcher, self.work, (self.stop, self.held), self.terms);
        prefix_sink.cancelled = self.cancelled;
        prefix_sink.written = written;
        let filled_prefix = filled
            .as_deref()
            .map(|filled| &filled[..prefix.len().min(filled.len())]);
        if filled_prefix.is_some() {
            prefix_sink.inverted_over = Some(prefix);
        }
        prefix_searcher.search_slice(
            self.matcher,
            filled_prefix.unwrap_or(prefix),
            &mut prefix_sink,
        )?;
        let mut outcome = FileOutcome::from(prefix_sink);
        outcome.binary = true;
        outcome.opaque = is_leading_binary(prefix, outcome.matched_lines > 0);
        Ok(outcome)
    }
}
