//! The one worker-local tree-sitter parser and every deadline-bounded parse.

use std::cell::RefCell;
use std::ops::ControlFlow;
use std::time::{Duration, Instant};
use tree_sitter::{Language, ParseOptions, Parser, Tree};

/// The deadline of a whole-file fact extraction: none it can reach. One file
/// cannot be paged, so it is read to completion (input is bounded by
/// `MAX_PARSE_SIZE`); a scan pages between files with its pacer, and its
/// cursor and checkpoints resume the rest. A wall-clock cut would drop a
/// file's facts depending on machine load.
pub(crate) fn whole_file_deadline() -> Instant {
    Instant::now() + Duration::from_secs(u64::from(u32::MAX))
}

thread_local! {
    /// The one worker-local tree-sitter parser. Repository scans reuse its
    /// allocations without retaining syntax trees or sharing mutable parser
    /// state; every tree-sitter parse in the engine goes through
    /// [`parse_with_deadline`].
    static PARSER: RefCell<Parser> = RefCell::new(Parser::new());
}

/// Why [`parse_with_deadline`] produced no tree.
#[derive(Debug)]
pub(crate) enum ParseFailure {
    /// The grammar was rejected by the parser (ABI mismatch).
    Language(String),
    /// The deadline passed before or during the parse.
    Interrupted,
}

/// Parse `content` with `language` on the worker-local parser, cancelling at
/// `deadline`. The parser is reset before and after every call (success,
/// interruption, or error), so the next independent file never inherits
/// cancellation or incremental parse state.
pub(crate) fn parse_with_deadline(
    content: &str,
    language: &Language,
    deadline: Instant,
) -> Result<Tree, ParseFailure> {
    if Instant::now() >= deadline {
        return Err(ParseFailure::Interrupted);
    }
    PARSER.with_borrow_mut(|parser| {
        parser.reset();
        parser
            .set_language(language)
            .map_err(|err| ParseFailure::Language(err.to_string()))?;
        let tree = parse_until(parser, content, "", deadline);
        parser.reset();
        tree.filter(|_| Instant::now() < deadline)
            .ok_or(ParseFailure::Interrupted)
    })
}

/// One parse of `content` followed by `tail` (source the parser reads as if
/// the content ended with it) that the progress callback stops at `deadline`.
fn parse_until(
    parser: &mut tree_sitter::Parser,
    content: &str,
    tail: &str,
    deadline: Instant,
) -> Option<Tree> {
    let bytes = content.as_bytes();
    let mut read = |offset: usize, _| {
        if offset < bytes.len() {
            &bytes[offset..]
        } else {
            tail.as_bytes().get(offset - bytes.len()..).unwrap_or(b"")
        }
    };
    let mut progress = |_: &tree_sitter::ParseState| {
        if Instant::now() >= deadline {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    parser.parse_with_options(
        &mut read,
        None,
        Some(ParseOptions::new().progress_callback(&mut progress)),
    )
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
    if Instant::now() >= deadline {
        return None;
    }
    PARSER.with_borrow_mut(|parser| {
        parser.reset();
        parser.set_language(language).ok()?;
        parser.set_included_ranges(ranges).ok()?;
        let tree = parse_until(parser, content, tail, deadline);
        let _ = parser.set_included_ranges(&[]);
        parser.reset();
        tree.filter(|_| Instant::now() < deadline)
    })
}

/// [`parse_with_deadline`] for callers that treat every failure alike.
pub(crate) fn parse_before(content: &str, language: &Language, deadline: Instant) -> Option<Tree> {
    parse_with_deadline(content, language, deadline).ok()
}
