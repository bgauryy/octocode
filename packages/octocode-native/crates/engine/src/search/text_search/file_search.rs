//! Searching one opened file: the searcher, the tapped read, the binary prefix re-search, and the
//! kept file's digest.

use super::file_terms::{CopiedRead, FILE_TERMS_MAX_BYTES};
use super::mode::MatchWork;
use super::sink::CollectSink;
use crate::search::relevance;
use crate::search::text_parser::FileEntry;
use crate::types::{SearchContext, SearchedSource, TextSearchMatch, TextSearchOptions};
use aho_corasick::AhoCorasick;
use grep_matcher::Matcher;
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder};
use sha2::{Digest as _, Sha256};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// Hard heap ceiling for a single `Searcher` pass. `grep-searcher` buffers a
/// whole line (or multiline block) before matching; without a cap a huge single
/// line can allocate unbounded. Files above [`DEFAULT_MAX_SEARCH_FILE_BYTES`](super::DEFAULT_MAX_SEARCH_FILE_BYTES)
/// are already skipped, so this only bounds a within-ceiling pathological line;
/// exceeding it fails that file (recorded as an error), never the whole search.
const SEARCH_HEAP_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// Largest pre-NUL prefix re-searched when a file is quit as binary. The
/// searcher drops the whole buffer that holds the first NUL, so matches before
/// it would otherwise be lost; beyond this offset the earlier buffers were
/// already searched and reported normally.
const MAX_BINARY_PREFIX_BYTES: u64 = 8 * 1024 * 1024;

/// Leading bytes a format header occupies. A first NUL inside them, before
/// any line break, ends a binary signature (font, database, archive magic),
/// not text cut short.
const LEADING_BINARY_BYTES: usize = 1024;

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

/// Whether a file quit at its first NUL is binary from its leading bytes,
/// and so outside a text search: the bytes before the NUL are binary by the
/// one sniff ([`crate::text::sniff`]), or they are a short single-line header
/// that matched nothing. A match, a complete text line, or a long text run
/// before the NUL is searchable text the cut leaves unread.
fn is_leading_binary(prefix: &[u8], matched: bool) -> bool {
    crate::text::sniff(prefix) == crate::text::BinaryVerdict::Binary
        || (!matched && prefix.len() < LEADING_BINARY_BYTES && !prefix.contains(&b'\n'))
}

/// Read the first `len` bytes of an already-open file.
pub(super) fn read_prefix(file: &std::fs::File, len: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut handle = file;
    handle.seek(SeekFrom::Start(0))?;
    let mut prefix = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    handle.take(len).read_to_end(&mut prefix)?;
    Ok(prefix)
}

/// Leading bytes kept from the searched read for the generated-header check.
const HEAD_BYTES: usize = relevance::GENERATED_HEADER_BYTES as usize;

/// The searcher's read of one opened file. Every byte the search reads
/// passes through here once, so the generated-header prefix and the
/// file-level terms' bytes come from that read instead of a second one.
struct SourceTap<'f> {
    file: &'f std::fs::File,
    /// The first [`HEAD_BYTES`] bytes, when `keep_head` is set.
    head: [u8; HEAD_BYTES],
    head_len: usize,
    keep_head: bool,
    /// Every byte read, up to [`FILE_TERMS_MAX_BYTES`], when file-level terms
    /// are checked against this same read.
    copy: Option<Vec<u8>>,
    /// A read returned no bytes: the file was read to its end.
    eof: bool,
}

/// The digest of a kept file, read whole from offset 0 after its search
/// (only kept files are hashed: most files a walk searches are dropped),
/// with the proof that the hashed bytes are the opened file whole: `None`
/// when its length or modification time moved since `meta`.
pub(super) fn hashed_source(
    file: &std::fs::File,
    meta: &std::fs::Metadata,
) -> Option<SearchedSource> {
    use std::io::{Read, Seek, SeekFrom};
    let mut hasher = Sha256::new();
    let mut len = 0u64;
    let mut buf = [0u8; 64 * 1024];
    let mut handle = file;
    handle.seek(SeekFrom::Start(0)).ok()?;
    loop {
        match handle.read(&mut buf) {
            Ok(0) => break,
            Ok(read) => {
                hasher.update(&buf[..read]);
                len += read as u64;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    let after = file.metadata().ok()?;
    let modified = meta.modified().ok();
    (len == meta.len() && after.len() == len && after.modified().ok() == modified).then(|| {
        SearchedSource {
            size: len,
            modified,
            digest: hasher.finalize().into(),
        }
    })
}

impl<'f> SourceTap<'f> {
    pub(super) fn new(file: &'f std::fs::File, keep_head: bool, copy: bool) -> Self {
        Self {
            file,
            head: [0; HEAD_BYTES],
            head_len: 0,
            keep_head,
            copy: copy.then(Vec::new),
            eof: false,
        }
    }

    /// The leading bytes, when the read reached [`HEAD_BYTES`] or the end.
    fn head(&self) -> Option<&[u8]> {
        (self.keep_head && (self.head_len == HEAD_BYTES || self.eof))
            .then(|| &self.head[..self.head_len])
    }
}

impl std::io::Read for SourceTap<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut handle = self.file;
        let read = handle.read(buf)?;
        if read == 0 {
            if !buf.is_empty() {
                self.eof = true;
            }
            return Ok(0);
        }
        let bytes = &buf[..read];
        if self.keep_head && self.head_len < HEAD_BYTES {
            let take = (HEAD_BYTES - self.head_len).min(read);
            self.head[self.head_len..self.head_len + take].copy_from_slice(&bytes[..take]);
            self.head_len += take;
        }
        if let Some(copy) = &mut self.copy {
            let room = usize::try_from(FILE_TERMS_MAX_BYTES).unwrap_or(usize::MAX) - copy.len();
            copy.extend_from_slice(&bytes[..read.min(room)]);
        }
        Ok(read)
    }
}

/// Result of searching one file.
pub(super) struct FileOutcome {
    pub(super) entry: FileEntry,
    pub(super) submatches: u32,
    pub(super) matched_lines: u32,
    pub(super) om_matches: Vec<TextSearchMatch>,
    pub(super) span_cap_reached: bool,
    pub(super) deadline_hit: bool,
    pub(super) binary: bool,
    /// The first NUL came before any text: nothing text-searchable was lost.
    pub(super) opaque: bool,
    pub(super) line_weight: u32,
    pub(super) declares: bool,
    pub(super) covered: u32,
    /// `relevance` only: the leading bytes declare a generated file; `None`
    /// when the read did not reach them.
    pub(super) generated_header: Option<bool>,
    /// The searched bytes, when file-level terms are checked.
    pub(super) copied: Option<CopiedRead>,
}

impl<M: Matcher> From<CollectSink<'_, M>> for FileOutcome {
    fn from(sink: CollectSink<'_, M>) -> Self {
        Self {
            entry: sink.entry,
            submatches: sink.submatches,
            matched_lines: sink.matched_lines,
            om_matches: sink.om_matches,
            span_cap_reached: sink.span_cap_reached,
            deadline_hit: sink.deadline_hit,
            binary: sink.binary_offset.is_some(),
            opaque: sink.binary_offset == Some(0),
            line_weight: sink.line_weight,
            covered: sink.covered.count_ones(),
            declares: sink.declares,
            generated_header: None,
            copied: None,
        }
    }
}

/// Per-worker searchers: the walk searcher quits at the first NUL; the prefix
/// searcher (built on first use) re-searches the NUL-free bytes before it.
pub(super) struct FileSearcher<'a, M: Matcher> {
    pub(super) opts: &'a TextSearchOptions,
    pub(super) matcher: &'a M,
    pub(super) searcher: Searcher,
    pub(super) prefix_searcher: Option<Searcher>,
    pub(super) context: SearchContext,
    pub(super) work: MatchWork,
    pub(super) deadline: Option<Instant>,
    pub(super) stop: &'a AtomicBool,
    /// Keep the leading bytes for the generated-header check.
    pub(super) keep_head: bool,
    /// Keep the read bytes for the file-level terms check.
    pub(super) copy: bool,
    pub(super) words: Option<&'a AhoCorasick>,
}

impl<M: Matcher> FileSearcher<'_, M> {
    /// Search an opened file in one read through a [`SourceTap`]. When it
    /// is quit as binary, the searcher has dropped the whole buffer holding
    /// the NUL, so the NUL-free prefix is searched again from byte 0 and its
    /// matches replace the first pass.
    pub(super) fn search(&mut self, file: &std::fs::File) -> std::io::Result<FileOutcome> {
        let mut work = self.work;
        if work.deferred_snippet.is_some() {
            let prefix = read_prefix(file, 3)?;
            if crate::text::has_utf16_bom(&prefix) {
                work.deferred_snippet = None;
            }
            std::io::Seek::seek(&mut &*file, std::io::SeekFrom::Start(0))?;
        }
        let mut tap = SourceTap::new(file, self.keep_head, self.copy);
        let mut sink = CollectSink::new(self.matcher, work, self.deadline, self.stop, self.words);
        // `search_file` reads through this same reader path: memory maps are
        // never used, and multiline reads the whole file under the heap limit.
        self.searcher
            .search_reader(self.matcher, &mut tap, &mut sink)?;
        // Only a matched file's header decides its rank.
        let header = |matched: bool| {
            matched
                .then(|| tap.head().map(relevance::has_generated_header))
                .flatten()
        };
        let offset = sink
            .binary_offset
            .filter(|offset| *offset != 0 && *offset <= MAX_BINARY_PREFIX_BYTES);
        let Some(offset) = offset else {
            let mut outcome = FileOutcome::from(sink);
            outcome.generated_header = header(outcome.matched_lines > 0);
            outcome.copied = tap.copy.map(|bytes| CopiedRead {
                bytes,
                eof: tap.eof,
            });
            return Ok(outcome);
        };
        let prefix = read_prefix(file, offset)?;
        let (opts, context) = (self.opts, self.context);
        let prefix_searcher = self
            .prefix_searcher
            .get_or_insert_with(|| build_searcher(opts, context, BinaryDetection::none()));
        let mut prefix_sink = CollectSink::new(
            self.matcher,
            self.work,
            self.deadline,
            self.stop,
            self.words,
        );
        prefix_searcher.search_slice(self.matcher, &prefix, &mut prefix_sink)?;
        let mut outcome = FileOutcome::from(prefix_sink);
        outcome.binary = true;
        outcome.opaque = is_leading_binary(&prefix, outcome.matched_lines > 0);
        outcome.generated_header = header(outcome.matched_lines > 0);
        outcome.copied = tap.copy.map(|bytes| CopiedRead {
            bytes,
            eof: tap.eof,
        });
        Ok(outcome)
    }
}
