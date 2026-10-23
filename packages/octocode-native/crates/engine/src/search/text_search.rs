//! Octocode's in-process text search: a gitignore-aware parallel walk feeding
//! a line-oriented regex searcher, with no external binary. Built on
//!   * `grep-searcher` + `grep-regex` for line-oriented matching,
//!   * `grep-pcre2` for lookaround and backreferences,
//!   * `ignore` for the gitignore-aware walk, include globs and file types.
//!
//! It returns a `TextSearchResult` with byte and time stats from the walk.

use crate::error::{Error, Result};
use crate::types::{CaseRule, PatternSyntax, TextSearchOptions, TextSearchResult};
use collect::{WalkControl, collect};
use grep_regex::RegexMatcherBuilder;
use mode::resolve_mode;
use pcre2::{PCRE2_LIMITS, Pcre2Limits, search_pcre2};
use result::build_result;
use state::CollectState;
use std::path::Path;
use std::sync::Arc;

mod collect;
mod file_search;
mod file_terms;
mod mode;
mod pcre2;
mod result;
mod sink;
mod state;
mod visit;
mod walk_rule;

pub(crate) use pcre2::{
    ACTIVE_PCRE2_WORKERS, MAX_ACTIVE_PCRE2_WORKERS, PCRE2_DEADLINE_GRACE, PCRE2_SEARCH_DEADLINE,
    Pcre2WorkerSlot, pcre2_builder, release_worker_slot, try_acquire_worker_slot,
};

pub trait TextSearchPathFilter: Send + Sync {
    fn allows(&self, path: &Path, is_dir: bool) -> bool;
    /// [`Self::allows`] for a walked entry with the type its directory
    /// listing gave (`None` when the listing had none): a filter that can
    /// decide from the type without another `lstat` overrides it.
    fn allows_entry(&self, path: &Path, file_type: Option<std::fs::FileType>) -> bool {
        self.allows(path, file_type.is_some_and(|kind| kind.is_dir()))
    }
    /// A dot entry the walk left out because hidden entries are off (a
    /// skipped directory is not descended into), so the caller can count
    /// it. Called before [`Self::allows`], which never sees the entry.
    fn skipped_hidden(&self, _path: &Path, _is_dir: bool) {}
    /// An entry an ignore rule (`.gitignore`, `.ignore`, the git exclude
    /// file or the global gitignore) left out because ignored entries are
    /// off (a skipped directory is not descended into), so the caller can
    /// count it. Called before [`Self::allows`], which never sees the entry.
    fn skipped_ignored(&self, _path: &Path, _is_dir: bool) {}
    /// A symlink [`Self::allows`] admitted that the walk did not follow (it
    /// searches regular files only), so the caller can count it.
    fn skipped_symlink(&self, _path: &Path) {}
}

const DEFAULT_MAX_SNIPPET_CHARS: u32 = 500;

/// Default per-file byte ceiling for the search path. A file larger than this is
/// skipped before it is opened/searched, so a pathological multi-GB (often
/// single-line, minified/generated) file cannot force a giant line buffer plus a
/// lossy-UTF8 copy. Callers may override via [`TextSearchOptions::max_file_bytes`].
/// Skipped files are surfaced as a `maxFileSize` cap reason, never silently
/// dropped.
pub(crate) const DEFAULT_MAX_SEARCH_FILE_BYTES: u64 = 20 * 1024 * 1024;

/// Default per-file ceiling for line-oriented (non-multiline) search. Line
/// mode streams the file through a line buffer already bounded by
/// `file_search::SEARCH_HEAP_LIMIT_BYTES`, so large logs, dumps, and generated files stay
/// searchable; only multiline search, which buffers the whole file, keeps the
/// tighter [`DEFAULT_MAX_SEARCH_FILE_BYTES`].
pub(crate) const DEFAULT_MAX_LINE_SEARCH_FILE_BYTES: u64 = 512 * 1024 * 1024;

fn to_engine_err<E: std::fmt::Display>(e: E) -> Error {
    Error::new(e.to_string())
}

/// Build the appropriate matcher (default Rust regex, or PCRE2 for `-P`) and run
/// the search, stopping at the next entry once `cancelled` returns true (the
/// partial result carries [`CapReason::Cancelled`](crate::types::CapReason::Cancelled)). A literal pattern is
/// escaped for the regex engine.
pub fn search_cancellable(
    opts: TextSearchOptions,
    path_filter: Arc<dyn TextSearchPathFilter>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<TextSearchResult> {
    search_with_limits(opts, path_filter, cancelled, PCRE2_LIMITS)
}

fn search_with_limits(
    opts: TextSearchOptions,
    path_filter: Arc<dyn TextSearchPathFilter>,
    cancelled: &(dyn Fn() -> bool + Sync),
    limits: Pcre2Limits,
) -> Result<TextSearchResult> {
    let mode = resolve_mode(&opts);

    let case_insensitive = opts.case == CaseRule::Insensitive;
    let smart_case = opts.case == CaseRule::Smart;
    let whole_word = opts.whole_word.unwrap_or(false);
    let multiline = opts.multiline.unwrap_or(false);
    let dotall = multiline && opts.multiline_dotall.unwrap_or(false);

    // ── PCRE2 path ──────────────────────────────────────────────────────────────
    if opts.syntax == PatternSyntax::Pcre2 {
        let mut b = pcre2_builder();
        b.caseless(case_insensitive)
            .case_smart(smart_case)
            .word(whole_word)
            .multi_line(multiline)
            .dotall(dotall)
            .crlf(true);
        let matcher = b.build(&opts.pattern).map_err(to_engine_err)?;
        return search_pcre2(opts, mode, matcher, path_filter, cancelled, limits);
    }

    // ── Default Rust-regex path ─────────────────────────────────────────────────
    let mut b = RegexMatcherBuilder::new();
    b.case_insensitive(case_insensitive)
        .case_smart(smart_case)
        .word(whole_word)
        // `^`/`$` always anchor at line boundaries; spanning lines is the
        // searcher's choice (multiline mode).
        .multi_line(true)
        .dot_matches_new_line(dotall);
    // Fast path: in line-oriented (non-multiline) search the
    // searcher feeds one terminator-stripped line at a time. Declaring the line
    // terminator lets grep-regex extract inner literals and cheaply skip lines
    // that cannot match (its primary perf lever) and forbids a pattern from
    // matching across `\n` (which the line-oriented sink could never surface
    // anyway). Multiline matchers span lines and must NOT set it.
    if !multiline {
        // `$` also matches before `\r\n`, as on the PCRE2
        // path: a Windows-edited file answers an end-anchored pattern. Set
        // before the `\n` terminator, which the searcher feeds lines by.
        b.crlf(true);
        b.line_terminator(Some(b'\n'));
    }
    let pattern = if opts.syntax == PatternSyntax::Literal {
        regex::escape(&opts.pattern)
    } else {
        opts.pattern.clone()
    };
    let matcher = b.build(&pattern).map_err(to_engine_err)?;
    // The Rust regex engine is linear-time and cannot catastrophically
    // backtrack, so it needs no wall-clock deadline.
    let state = CollectState::new();
    collect(
        &opts,
        &matcher,
        mode,
        path_filter,
        &state,
        WalkControl {
            deadline: None,
            cancelled,
            eager: false,
        },
    )?;
    Ok(build_result(&opts, mode, state.snapshot()))
}

#[cfg(test)]
mod tests;
