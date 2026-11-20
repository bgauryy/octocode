//! Alternate matchers over the files a walk already admitted: an optional
//! probe that needs no listing or admission of its own, and reads each file
//! once however many matchers it runs.

use super::collect::byte_ceiling;
use super::file_search::Searched;
use super::file_search::{FileSearcher, SEARCH_HEAP_LIMIT_BYTES, build_searcher};
use super::judge_gate::JudgeGate;
use super::mode::{Mode, match_work};
use super::{rust_matcher_builder, rust_spelling, to_engine_err};
use crate::error::{Error, ErrorKind, Result};
use crate::types::{PatternSyntax, SearchContext, SearchOutput, TextSearchOptions};
use grep_regex::RegexMatcher;
use grep_searcher::BinaryDetection;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The largest read buffer a worker keeps for its next file.
const REUSED_BUFFER_BYTES: usize = 1024 * 1024;

#[cfg(test)]
thread_local! {
    static READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Passes this thread's workers made over a file's bytes.
#[cfg(test)]
pub(super) fn reads() -> usize {
    READS.with(std::cell::Cell::get)
}

fn note_read() {
    #[cfg(test)]
    READS.with(|reads| reads.set(reads.get() + 1));
}

/// Whether [`count_listed`] answers `opts` as its walk would: a Rust or literal
/// pattern (PCRE2 runs on its own worker driver), with no scope that reads more
/// of a file than its matches (`testCode`, a line scope, `fileHas`, `fileLacks`).
#[must_use]
pub fn listable(opts: &TextSearchOptions) -> bool {
    opts.syntax != PatternSyntax::Pcre2
        && opts.output == SearchOutput::FilesWithMatch
        && opts.test_code.is_none()
        && opts.line_scope.is_none()
        && opts.file_contains_all.is_none()
        && opts.file_contains_none.is_none()
}

/// How many of `files` hold a match of each of `alternates`, as many counts as
/// alternates, by the rules of the walk [`listable`] options describe: `files`
/// stand for what its globs, types, depth and path filter admitted, and a
/// file that cannot be opened or read counts for none. Each file is opened and
/// read once when it fits a searcher's heap; a larger one is streamed once per
/// matcher rather than held. Stops at the next file once `cancelled()` is true,
/// keeping what it counted.
///
/// # Errors
/// An alternate that is not [`listable`] or whose pattern does not compile.
pub fn count_listed(
    alternates: &[&TextSearchOptions],
    files: &[PathBuf],
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<Vec<usize>> {
    if !alternates.iter().all(|opts| listable(opts)) {
        return Err(Error::with_kind(
            ErrorKind::InvalidInput,
            "these search options need a walk",
        ));
    }
    let matchers = alternates
        .iter()
        .map(|opts| {
            rust_matcher_builder(opts)
                .build(&rust_spelling(opts, &opts.pattern))
                .map_err(to_engine_err)
        })
        .collect::<Result<Vec<_>>>()?;
    let gates = alternates
        .iter()
        .map(|opts| JudgeGate::of(opts, &rust_spelling(opts, &opts.pattern)))
        .collect::<Vec<_>>();
    let found = alternates
        .iter()
        .map(|_| AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let (next, stop) = (AtomicUsize::new(0), AtomicBool::new(false));
    // Counting keeps no row, so nothing is held against the bound.
    let held = std::sync::Arc::new(super::state::Held::new(None));
    let work = || {
        let mut searchers = alternates
            .iter()
            .zip(&matchers)
            .zip(&gates)
            .map(|((opts, matcher), gate)| FileSearcher {
                opts,
                matcher,
                searcher: build_searcher(
                    opts,
                    SearchContext::default(),
                    BinaryDetection::quit(b'\x00'),
                ),
                prefix_searcher: None,
                context: SearchContext::default(),
                // Only whether a line matched: no span or value is kept.
                work: match_work(Mode::CountLines, false),
                stop: &stop,
                held: &held,
                cancelled: Some(cancelled),
                copy: false,
                terms: &[],
                gate,
                // Counting binds no digest to a file.
                read: super::file_search::ReadDigest::new(false),
            })
            .collect::<Vec<_>>();
        let mut held = Vec::new();
        while let Some(path) = files.get(next.fetch_add(1, Ordering::Relaxed)) {
            if stop.load(Ordering::Relaxed) {
                return;
            }
            if cancelled() {
                stop.store(true, Ordering::Relaxed);
                return;
            }
            count_in(path, &mut searchers, &mut held, &found);
            // The read buffer is reused by the files after it only while it
            // is no larger than one read of the search's streaming unit.
            if held.capacity() > REUSED_BUFFER_BYTES {
                held = Vec::new();
            }
        }
    };
    let workers = crate::cpu::host().share().min(files.len());
    std::thread::scope(|scope| {
        for _ in 1..workers {
            scope.spawn(work);
        }
        work();
    });
    Ok(found
        .iter()
        .map(|count| count.load(Ordering::Relaxed))
        .collect())
}

/// Count `path` for each searcher that finds a match in its text.
fn count_in(
    path: &Path,
    searchers: &mut [FileSearcher<'_, RegexMatcher>],
    held: &mut Vec<u8>,
    found: &[AtomicUsize],
) {
    let Ok((file, meta)) = crate::text::open_regular(path) else {
        return;
    };
    let heap = SEARCH_HEAP_LIMIT_BYTES as u64;
    let shared = searchers.len() > 1 && meta.len() <= heap;
    // What the file holds in memory is admitted until it is counted: its
    // bytes, and the text its judge serves of them.
    let _admitted = if shared {
        let copies = if searchers.iter().any(|s| s.opts.judge.is_some()) {
            2
        } else {
            1
        };
        let bytes = usize::try_from(meta.len()).unwrap_or(usize::MAX);
        match searchers
            .first()
            .map(|searcher| searcher.admit(bytes.saturating_mul(copies)))
        {
            Some(Ok(ticket)) => Some(ticket),
            _ => return,
        }
    } else {
        None
    };
    if shared {
        held.clear();
        note_read();
        if (&file).take(heap).read_to_end(held).is_err() {
            return;
        }
    }
    // The text the search's judge serves is what every matcher sees: judged
    // once, when the first matcher whose gate does not prove the file
    // unmatched needs it.
    let mut judged = Judged::Pending;
    for (searcher, found) in searchers.iter_mut().zip(found.iter()) {
        if meta.len() > byte_ceiling(searcher.opts) {
            continue;
        }
        let outcome = if shared {
            held_outcome(searcher, (&file, path), held, &mut judged)
        } else {
            note_read();
            (&file)
                .seek(SeekFrom::Start(0))
                .and_then(|_| searcher.search(&file, path))
        };
        // Binary from its leading bytes is outside a text search, whatever
        // its bytes matched.
        match outcome {
            Ok(outcome) if outcome.matched_lines > 0 && !outcome.opaque => {
                found.fetch_add(1, Ordering::Relaxed);
            }
            Ok(_) => {}
            // A judge that failed leaves the file unsearched by every matcher.
            Err(_) if shared && matches!(judged, Judged::Pending) => return,
            Err(_) => {}
        }
    }
}

/// What the search's judge made of one held file.
enum Judged {
    /// Not judged yet: every matcher so far was proved unmatched by its gate.
    Pending,
    /// Judged, and the judge replaced nothing: the held bytes are its text.
    Unchanged,
    /// Judged: the text the judge serves in place of the held bytes.
    Replaced(super::file_search::JudgedText),
}

/// `searcher`'s outcome over `held`, the bytes of `file` read once: the
/// search of those bytes when its gate proves the text the judge serves
/// holds no match, else the search of that text, judged into `judged` once.
fn held_outcome(
    searcher: &mut FileSearcher<'_, RegexMatcher>,
    (file, path): (&std::fs::File, &Path),
    held: &[u8],
    judged: &mut Judged,
) -> std::io::Result<super::file_search::FileOutcome> {
    if matches!(judged, Judged::Pending) {
        if searcher.opts.judge.is_some() && searcher.gate.gates() {
            let own = searcher.search_held(Searched::own(held))?;
            if searcher
                .gate
                .proves_unmatched(file, own.matched_lines, own.binary)?
            {
                return Ok(own);
            }
        }
        *judged = match searcher.judged(file, path)? {
            Some(served) => Judged::Replaced(served),
            None => Judged::Unchanged,
        };
    }
    match judged {
        Judged::Replaced(judged) => searcher.search_held((&judged.served).into()),
        Judged::Pending | Judged::Unchanged => searcher.search_held(Searched::own(held)),
    }
}
