//! Alternate matchers over the files a walk already admitted: an optional
//! probe that needs no listing or admission of its own, and reads each file
//! once however many matchers it runs.

use super::collect::byte_ceiling;
use super::file_search::{FileSearcher, SEARCH_HEAP_LIMIT_BYTES, build_searcher};
use super::mode::{Mode, match_work};
use super::{rust_matcher_builder, rust_spelling, to_engine_err};
use crate::error::{Error, ErrorKind, Result};
use crate::types::{PatternSyntax, SearchContext, SearchOutput, TextSearchOptions};
use grep_regex::RegexMatcher;
use grep_searcher::BinaryDetection;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
/// pattern (PCRE2 needs its deadline driver), with no scope that reads more
/// of a file than its matches (`testCode`, `fileHas`, `fileLacks`).
#[must_use]
pub fn listable(opts: &TextSearchOptions) -> bool {
    opts.syntax != PatternSyntax::Pcre2
        && opts.output == SearchOutput::FilesWithMatch
        && opts.test_code.is_none()
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
    let found = alternates
        .iter()
        .map(|_| AtomicUsize::new(0))
        .collect::<Vec<_>>();
    let (next, stop) = (AtomicUsize::new(0), AtomicBool::new(false));
    let work = || {
        let mut searchers = alternates
            .iter()
            .zip(&matchers)
            .map(|(opts, matcher)| FileSearcher {
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
                deadline: None,
                stop: &stop,
                keep_head: false,
                copy: false,
                words: None,
                terms: &[],
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
    if shared {
        held.clear();
        note_read();
        if (&file).take(heap).read_to_end(held).is_err() {
            return;
        }
    }
    for (searcher, found) in searchers.iter_mut().zip(found) {
        if meta.len() > byte_ceiling(searcher.opts) {
            continue;
        }
        let outcome = if shared {
            searcher.search_held(held)
        } else {
            note_read();
            (&file)
                .seek(SeekFrom::Start(0))
                .and_then(|_| searcher.search(&file, path))
        };
        // Binary from its leading bytes is outside a text search, whatever
        // its bytes matched.
        if outcome.is_ok_and(|outcome| outcome.matched_lines > 0 && !outcome.opaque) {
            found.fetch_add(1, Ordering::Relaxed);
        }
    }
}
