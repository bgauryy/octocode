//! The one worker-local tree-sitter parser and every parse in the engine.
//!
//! A parse ends when the work it serves is over: its deadline passes, its
//! caller stops waiting (a deep-stack job's cancel flag), or the caller's own
//! stop signal is raised. It also ends when its work stops being linear in its
//! source: on some malformed input, tree-sitter's error recovery re-reads the
//! source again and again (an unclosed regex class repeated on one line), and
//! time grows faster than the input. That parse fails as [`ParseFailure::Unbounded`], which each
//! caller discloses as a file the grammar could not read.

use std::cell::{Cell, RefCell};
use std::ops::ControlFlow;
use std::time::{Duration, Instant};
use tree_sitter::{Language, ParseOptions, Parser, Tree};

/// The deadline of a whole-file fact extraction: none it can reach. One file
/// cannot be paged, so it is read to completion (bounded by the linear work
/// bound below); a scan pages between files with its pacer, and its cursor
/// and checkpoints resume the rest. A wall-clock cut would drop a file's facts
/// depending on machine load.
pub(crate) fn whole_file_deadline() -> Instant {
    Instant::now() + Duration::from_secs(u64::from(u32::MAX))
}

thread_local! {
    /// The one worker-local tree-sitter parser. Repository scans reuse its
    /// allocations without retaining syntax trees or sharing mutable parser
    /// state; every tree-sitter parse in the engine goes through [`run`].
    static PARSER: RefCell<Parser> = RefCell::new(Parser::new());
}

/// Bytes the parser is handed per read, cut at a char boundary: source the
/// lexer reads again after recovery rewinds shows up as more reads.
const READ_CHUNK: usize = 4096;

/// The linear work bound: progress reports (tree-sitter makes one about every
/// hundred parser operations) per KiB of source, and reads per [`READ_CHUNK`].
/// Measured 2026-11-05 by `skills-dev/octocode-dev/scripts/compact_sweep.py`
/// over every error-free file of `packages/octocode-benchmark/local/repos`
/// (279,000 files, 13 grammars): at most 0.057 reports per byte (58.4 per
/// KiB, Rust) and 46 reads per chunk (Python); an unclosed regex class
/// repeated on one 64 KiB line takes thousands of reads per chunk. The bound
/// is that maximum times [`HEADROOM`].
const REPORTS_PER_KIB: usize = 59 * HEADROOM;
const READS_PER_CHUNK: usize = 46 * HEADROOM;
const HEADROOM: usize = 16;

/// Why a parse produced no tree.
#[derive(Debug)]
pub(crate) enum ParseFailure {
    /// The grammar was rejected by the parser (ABI mismatch).
    Language(String),
    /// The deadline passed, or the work it serves stopped, before or during
    /// the parse.
    Interrupted,
    /// Its work passed the linear bound: the grammar cannot read this source
    /// in time proportional to it.
    Unbounded,
}

/// The work a parse did.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Work {
    pub(crate) reports: usize,
    pub(crate) reads: usize,
}

impl Work {
    /// The read chunks a source of `len` bytes spans.
    pub(crate) fn chunks(len: usize) -> usize {
        len / READ_CHUNK + 1
    }

    fn exceeds(self, len: usize) -> bool {
        self.reports > REPORTS_PER_KIB * (len / 1024 + 1)
            || self.reads > READS_PER_CHUNK * Self::chunks(len)
    }
}

/// One parse of `content` followed by `tail` (source the parser reads as if
/// the content ended with it) on the worker-local parser, inside `ranges`
/// when given. The parser is reset, with no included ranges, before and after
/// every call, so the next independent file never inherits cancellation,
/// ranges or incremental parse state.
fn run(
    content: &str,
    tail: &str,
    language: &Language,
    ranges: Option<&[tree_sitter::Range]>,
    stop: &dyn Fn() -> bool,
    bounded: bool,
) -> (Result<Tree, ParseFailure>, Work) {
    let work = Cell::new(Work::default());
    let stopped = || stop() || crate::deep_stack::job_cancelled();
    if stopped() {
        return (Err(ParseFailure::Interrupted), work.get());
    }
    let parsed = PARSER.with_borrow_mut(|parser| {
        parser.reset();
        let _ = parser.set_included_ranges(&[]);
        parser
            .set_language(language)
            .map_err(|err| ParseFailure::Language(err.to_string()))?;
        if let Some(ranges) = ranges {
            parser
                .set_included_ranges(ranges)
                .map_err(|err| ParseFailure::Language(err.to_string()))?;
        }
        let len = content.len() + tail.len();
        let failure: Cell<Option<End>> = Cell::new(None);
        let mut read = |offset: usize, _| {
            work.set(Work {
                reads: work.get().reads + 1,
                ..work.get()
            });
            let (text, at) = if offset < content.len() {
                (content, offset)
            } else {
                (tail, offset - content.len())
            };
            chunk(text, at)
        };
        let mut progress = |_: &tree_sitter::ParseState| {
            let done = Work {
                reports: work.get().reports + 1,
                ..work.get()
            };
            work.set(done);
            if stopped() {
                failure.set(Some(End::Interrupted));
            } else if bounded && done.exceeds(len) {
                failure.set(Some(End::Unbounded));
            }
            if failure.get().is_some() {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        };
        let tree = parser.parse_with_options(
            &mut read,
            None,
            Some(ParseOptions::new().progress_callback(&mut progress)),
        );
        let _ = parser.set_included_ranges(&[]);
        parser.reset();
        match (tree, failure.get()) {
            (_, Some(End::Unbounded)) => Err(ParseFailure::Unbounded),
            (Some(tree), None) => Ok(tree),
            (_, Some(End::Interrupted)) | (None, None) => Err(ParseFailure::Interrupted),
        }
    });
    (parsed, work.get())
}

#[derive(Clone, Copy)]
enum End {
    Interrupted,
    Unbounded,
}

/// Up to [`READ_CHUNK`] bytes of `text` from `at`, ending on a char boundary.
fn chunk(text: &str, at: usize) -> &[u8] {
    let Some(rest) = text.as_bytes().get(at..) else {
        return b"";
    };
    let mut end = READ_CHUNK.min(rest.len());
    while end < rest.len() && !text.is_char_boundary(at + end) {
        end += 1;
    }
    &rest[..end]
}

/// Parse `content` with `language`, ending at `deadline`.
pub(crate) fn parse_with_deadline(
    content: &str,
    language: &Language,
    deadline: Instant,
) -> Result<Tree, ParseFailure> {
    run(
        content,
        "",
        language,
        None,
        &|| Instant::now() >= deadline,
        true,
    )
    .0
}

/// Parse `content` with `language` until `stop` says the work it serves is
/// over (its request was cancelled or ran out of time): there is no clock of
/// its own, so what it returns never depends on machine load.
pub(crate) fn parse_until_stopped(
    content: &str,
    language: &Language,
    stop: &dyn Fn() -> bool,
) -> Result<Tree, ParseFailure> {
    run(content, "", language, None, stop, true).0
}

/// Parse only `ranges` of `content` (tree-sitter included ranges): node
/// positions stay in `content` coordinates, so one `LineIndex` serves both
/// trees. Used to read Rust item-level macro bodies as items.
pub(crate) fn parse_ranges_before(
    content: &str,
    language: &Language,
    ranges: &[tree_sitter::Range],
    deadline: Instant,
) -> Option<Tree> {
    parse_ranges_ending(content, "", language, ranges, deadline)
}

/// [`parse_ranges_before`] of `content` followed by `tail`: the parser reads
/// `tail` right after `content` without it being part of the caller's text,
/// so a range may cover it. A `#define` body is only a complete statement
/// with its `;`.
pub(crate) fn parse_ranges_ending(
    content: &str,
    tail: &str,
    language: &Language,
    ranges: &[tree_sitter::Range],
    deadline: Instant,
) -> Option<Tree> {
    let stop = || Instant::now() >= deadline;
    run(content, tail, language, Some(ranges), &stop, true)
        .0
        .ok()
}

/// [`parse_with_deadline`] for callers that treat every failure alike.
pub(crate) fn parse_before(content: &str, language: &Language, deadline: Instant) -> Option<Tree> {
    parse_with_deadline(content, language, deadline).ok()
}

/// A parse with no work bound, and the work it did: what the bound is
/// measured from (`minify::verify::parse_work`).
pub(crate) fn measure(content: &str, language: &Language) -> (Result<Tree, ParseFailure>, Work) {
    run(content, "", language, None, &|| false, false)
}

#[cfg(test)]
mod tests;
