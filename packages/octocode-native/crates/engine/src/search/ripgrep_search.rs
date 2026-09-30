//! In-process ripgrep search.
//!
//! Octocode is its own source of ripgrep: instead of shelling out to an `rg`
//! binary (and bundling one via `@vscode/ripgrep`), this module drives
//! ripgrep's own library crates directly —
//!   * `grep` (grep-searcher + grep-regex + grep-printer) for the search engine,
//!   * the `pcre2` feature (grep-pcre2) for `-P` lookaround/backreferences,
//!   * `ignore` for the gitignore-aware walk, `-g` override globs and `-t` types.
//!
//! It returns the same `RipgrepParseResult` shape as the `--json` parser, with
//! native byte/time stats populated by the in-process search path.

use std::collections::HashMap;
use std::path::Path;
#[cfg(feature = "pcre2")]
use std::sync::atomic::AtomicUsize;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
};
use std::time::{Duration, Instant, SystemTime};

use super::relevance;
use crate::error::{Error, Result, Status};
use grep_matcher::Matcher;
#[cfg(feature = "pcre2")]
use grep_pcre2::RegexMatcherBuilder as Pcre2MatcherBuilder;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::overrides::OverrideBuilder;
use ignore::types::TypesBuilder;
use ignore::{WalkBuilder, WalkState};

use crate::search::classify;
use crate::search::ripgrep_parser::{FileEntry, RawMatch, assemble_file, strip_trailing_newline};
use crate::text::utf8_offsets::{
    byte_to_char_offset_inner, ceil_char_boundary, floor_char_boundary,
};
use crate::types::{
    RipgrepFile, RipgrepMatch, RipgrepParseResult, RipgrepSearchOptions, RipgrepStats,
};

pub trait RipgrepPathFilter: Send + Sync {
    fn allows(&self, path: &Path, is_dir: bool) -> bool;
}
struct AllowAll;
impl RipgrepPathFilter for AllowAll {
    fn allows(&self, _: &Path, _: bool) -> bool {
        true
    }
}

const DEFAULT_MAX_SNIPPET_CHARS: u32 = 500;

/// Default per-file byte ceiling for the search path. A file larger than this is
/// skipped before it is opened/searched, so a pathological multi-GB (often
/// single-line, minified/generated) file cannot force a giant line buffer plus a
/// lossy-UTF8 copy. Callers may override via [`RipgrepSearchOptions::max_file_bytes`].
/// Skipped files are surfaced as a `maxFileSize` cap reason, never silently
/// dropped. Mirrors the classification guard (`classify.rs`), but larger so real
/// source files (including sizeable generated ones) are still searched.
pub(crate) const DEFAULT_MAX_SEARCH_FILE_BYTES: u64 = 20 * 1024 * 1024;

/// Default per-file ceiling for line-oriented (non-multiline) search. Line
/// mode streams the file through a line buffer already bounded by
/// [`SEARCH_HEAP_LIMIT_BYTES`], so large logs, dumps, and generated files stay
/// searchable; only multiline search, which buffers the whole file, keeps the
/// tighter [`DEFAULT_MAX_SEARCH_FILE_BYTES`].
pub(crate) const DEFAULT_MAX_LINE_SEARCH_FILE_BYTES: u64 = 512 * 1024 * 1024;

/// Hard heap ceiling for a single `Searcher` pass. `grep-searcher` buffers a
/// whole line (or multiline block) before matching; without a cap a huge single
/// line can allocate unbounded. Files above [`DEFAULT_MAX_SEARCH_FILE_BYTES`]
/// are already skipped, so this only bounds a within-ceiling pathological line;
/// exceeding it fails that file (recorded as an error), never the whole search.
const SEARCH_HEAP_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// Cap on emitted spans per line in only-matching mode, so a pathological
/// minified line with a huge number of hits can't blow up the result. The true
/// submatch count is still reported in stats.
const MAX_ONLY_MATCHING_PER_LINE: u32 = 1000;

#[cfg(feature = "pcre2")]
/// Cap on PCRE2's JIT stack (1 MiB). A user `-P` pattern with catastrophic
/// backtracking (`(a+)+$`-class) exhausts this cap and fails fast per file
/// instead of spinning against the JIT's default 32 KB stack growth. Residual
/// risk: `grep-pcre2` 0.1 exposes no `match_limit`/`depth_limit` knob, so a
/// backtracking blowup that stays within the JIT stack is still only bounded by
/// PCRE2's internal default match limit (not the wall clock). Applied to every
/// PCRE2 matcher we build (search + pattern validation).
pub(crate) const PCRE2_MAX_JIT_STACK_BYTES: usize = 1 << 20;

#[cfg(feature = "pcre2")]
/// Wall-clock ceiling for a whole PCRE2 (`-P`) search. PCRE2's JIT-stack cap
/// bounds a single catastrophic backtrack's *memory*, but nothing bounds its
/// *time*: a pathological `-P` pattern can spin for a long time inside a single
/// `find_at`/`search_path` call that cannot be interrupted from the sink. We
/// bound PCRE2 searches two ways, both keyed off this deadline:
///   1. Cooperatively — the collect walk and the match sink poll the deadline
///      between files and between matched lines, so an accumulation of moderately
///      expensive matches stops promptly while keeping partial results.
///   2. Hard — the whole PCRE2 search runs on a worker thread that writes each
///      finished file into shared [`CollectState`]. After
///      `PCRE2_SEARCH_DEADLINE + PCRE2_DEADLINE_GRACE` the driver raises the
///      shared stop flag and returns the files finished so far, flagged
///      `pcre2Deadline`. A thread cannot be killed: a worker inside one
///      uninterruptible match keeps its slot until that match returns, then
///      sees the stop flag and exits without searching further files.
///
/// Only PCRE2 needs this — the default Rust-regex engine is linear and cannot
/// catastrophically backtrack.
pub(crate) const PCRE2_SEARCH_DEADLINE: Duration = Duration::from_secs(5);

#[cfg(feature = "pcre2")]
/// Extra time the driver waits past the cooperative deadline before it stops
/// waiting for a stuck PCRE2 worker (see [`PCRE2_SEARCH_DEADLINE`]).
pub(crate) const PCRE2_DEADLINE_GRACE: Duration = Duration::from_secs(2);

#[cfg(feature = "pcre2")]
/// How often the PCRE2 driver wakes to poll caller cancellation and the hard
/// deadline while it waits for the worker.
const PCRE2_DRIVER_POLL: Duration = Duration::from_millis(25);

#[cfg(feature = "pcre2")]
/// Maximum number of PCRE2 (`-P`) search worker threads alive at once,
/// including workers still finishing an uninterruptible match after the driver
/// stopped waiting. Once saturated, a new `-P` search is rejected rather than
/// spawning another thread (each may hold a 1 MiB JIT stack — see
/// [`PCRE2_MAX_JIT_STACK_BYTES`]).
const MAX_ACTIVE_PCRE2_WORKERS: usize = 8;

#[cfg(feature = "pcre2")]
/// Wall-clock limits for one PCRE2 search. Production uses
/// [`PCRE2_SEARCH_DEADLINE`] and [`PCRE2_DEADLINE_GRACE`]; tests shrink them.
#[derive(Clone, Copy)]
struct Pcre2Limits {
    deadline: Duration,
    grace: Duration,
}

#[cfg(feature = "pcre2")]
const PCRE2_LIMITS: Pcre2Limits = Pcre2Limits {
    deadline: PCRE2_SEARCH_DEADLINE,
    grace: PCRE2_DEADLINE_GRACE,
};

/// Largest pre-NUL prefix re-searched when a file is quit as binary. The
/// searcher drops the whole buffer that holds the first NUL, so matches before
/// it would otherwise be lost; beyond this offset the earlier buffers were
/// already searched and reported normally.
const MAX_BINARY_PREFIX_BYTES: u64 = 8 * 1024 * 1024;

#[cfg(feature = "pcre2")]
/// Live PCRE2 worker count (including abandoned-but-still-running workers).
static ACTIVE_PCRE2_WORKERS: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "pcre2")]
/// Reserve a worker slot if the live count is below `max`. Returns false when
/// saturated (leaving the counter unchanged). Lock-free and pure so the bound is
/// directly unit-testable without driving a real catastrophic regex.
fn try_acquire_worker_slot(counter: &AtomicUsize, max: usize) -> bool {
    let mut current = counter.load(Ordering::Relaxed);
    loop {
        if current >= max {
            return false;
        }
        match counter.compare_exchange_weak(
            current,
            current + 1,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => return true,
            Err(observed) => current = observed,
        }
    }
}

#[cfg(feature = "pcre2")]
fn release_worker_slot(counter: &AtomicUsize) {
    counter.fetch_sub(1, Ordering::AcqRel);
}

#[cfg(feature = "pcre2")]
/// Releases the global PCRE2 worker slot when the worker thread exits — whether
/// it completed normally or finished after the driver stopped waiting.
struct Pcre2WorkerSlot;

#[cfg(feature = "pcre2")]
impl Drop for Pcre2WorkerSlot {
    fn drop(&mut self) {
        release_worker_slot(&ACTIVE_PCRE2_WORKERS);
    }
}

fn to_napi_err<E: std::fmt::Display>(e: E) -> Error {
    Error::new(Status::GenericFailure, e.to_string())
}

/// Map a byte offset in the raw line `bytes` to the matching byte offset in
/// `String::from_utf8_lossy(bytes)`. Each invalid byte becomes a 3-byte U+FFFD,
/// so raw offsets must not be applied to the lossy text directly.
fn lossy_offset(bytes: &[u8], offset: usize) -> usize {
    let offset = offset.min(bytes.len());
    match std::str::from_utf8(bytes) {
        Ok(_) => offset,
        Err(_) => String::from_utf8_lossy(&bytes[..offset]).len(),
    }
}

/// Slice the matched span `[start, end)` (byte offsets) out of `line`,
/// optionally widened by `window` characters on each side. Always returns a
/// valid UTF-8 substring; trimmed sides are marked with `…`.
fn span_value(line: &str, start: usize, end: usize, window: usize) -> String {
    let start = floor_char_boundary(line, start);
    let end = ceil_char_boundary(line, end).max(start);
    if window == 0 {
        return line[start..end].to_owned();
    }
    // Step back `window` chars from `start`.
    let mut left = start;
    for _ in 0..window {
        if left == 0 {
            break;
        }
        left = floor_char_boundary(line, left - 1);
    }
    // Step forward `window` chars from `end`.
    let mut right = end;
    for _ in 0..window {
        if right >= line.len() {
            break;
        }
        right = ceil_char_boundary(line, right + 1);
    }
    let mut out = String::new();
    if left > 0 {
        out.push('…');
    }
    out.push_str(&line[left..right]);
    if right < line.len() {
        out.push('…');
    }
    out
}

/// Output mode. The CLI builder applied these with a fixed precedence
/// (filesOnly → filesWithoutMatch → countMatches → countLines → normal); we
/// mirror that precedence so conflicting flags resolve identically.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    FilesOnly,
    FilesWithoutMatch,
    CountMatches,
    CountLines,
    Normal,
}

#[derive(Clone, Copy)]
struct MatchWork {
    materialize_line: bool,
    enumerate_submatches: bool,
    collect_spans: bool,
    /// Sum [`relevance::line_weight`] over matched lines.
    weigh_lines: bool,
}

fn match_work(mode: Mode, only_matching: bool) -> MatchWork {
    MatchWork {
        materialize_line: mode == Mode::Normal,
        enumerate_submatches: mode != Mode::CountLines,
        collect_spans: mode == Mode::Normal && only_matching,
        weigh_lines: false,
    }
}

/// Views that report per-file matches; path-list views carry no density.
fn lists_match_density(mode: Mode) -> bool {
    !matches!(mode, Mode::FilesOnly | Mode::FilesWithoutMatch)
}

fn ranks_by_relevance(opts: &RipgrepSearchOptions) -> bool {
    opts.sort.as_deref() == Some("relevance")
}

/// A search for one bare identifier (`spawn_blocking`, literal or regex):
/// the question is where that name lives, so a declaring file leads.
fn identifier_search(opts: &RipgrepSearchOptions) -> bool {
    let pattern = opts.pattern.as_bytes();
    !opts.invert_match.unwrap_or(false)
        && pattern
            .first()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$'))
        && pattern
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
}

fn resolve_mode(opts: &RipgrepSearchOptions) -> Mode {
    if opts.files_only.unwrap_or(false) {
        Mode::FilesOnly
    } else if opts.files_without_match.unwrap_or(false) {
        Mode::FilesWithoutMatch
    } else if opts.count_matches_per_file.unwrap_or(false) {
        Mode::CountMatches
    } else if opts.count_lines_per_file.unwrap_or(false) {
        Mode::CountLines
    } else {
        Mode::Normal
    }
}

/// Per-file collected state from a single `Searcher` pass.
struct FileRec {
    path: String,
    entry: FileEntry,
    /// Number of matching (or, with `-v`, non-matching) lines reported.
    matched_lines: u32,
    /// Number of individual matches (submatches) across all matched lines.
    submatches: u32,
    /// only-matching spans (empty unless `only_matching` is set).
    om_matches: Vec<RipgrepMatch>,
    /// Metadata timestamp captured for non-path sort keys.
    sort_time: Option<SystemTime>,
    /// `relevance` only: summed [`relevance::line_weight`] of matched lines.
    line_weight: u32,
    /// `relevance` only: a test, generated, or vendored path below the root.
    demoted: bool,
    /// `relevance` identifier search only: a matched line declares the name
    /// (see [`identifier_search`]).
    declares: bool,
}

/// Everything one search accumulated. Totals (`files_matched`, `submatches`,
/// `matched_lines`) count every kept file, including files the collection cap
/// later drops from `recs`.
struct CollectResult {
    recs: Vec<FileRec>,
    files_searched: u64,
    bytes_searched: u64,
    files_matched: u64,
    submatches: u64,
    matched_lines: u64,
    elapsed: Duration,
    /// A per-line only-matching span cap was hit (`maxOnlyMatchingPerLine`).
    span_capped: bool,
    /// The PCRE2 wall-clock deadline was hit (`pcre2Deadline`).
    timed_out: bool,
    /// At least one file exceeded the byte ceiling and was skipped (`maxFileSize`).
    size_skipped: bool,
    /// At least one file was quit as binary (`binaryQuit`); coverage is partial.
    binary_quit: bool,
    /// Paths of binary-quit files (bounded) and their total count.
    binary_files: Vec<String>,
    binary_file_count: u32,
    skipped_binary_count: u32,
    /// The caller cancelled the search before the walk finished (`cancelled`).
    cancelled: bool,
    error_count: u32,
    first_error: Option<String>,
}

/// Binary-quit file paths a search keeps to name in its warning.
const MAX_REPORTED_BINARY_FILES: usize = 5;

/// Shared accumulation state for one search. Walk workers write finished files
/// and counters here; the PCRE2 driver can read a consistent snapshot of the
/// files finished so far while a worker is still stuck in a match, and raise
/// `stop` so every worker quits at its next check.
struct CollectState {
    started: Instant,
    recs: Mutex<Vec<FileRec>>,
    files_searched: AtomicU64,
    bytes_searched: AtomicU64,
    files_matched: AtomicU64,
    submatches: AtomicU64,
    matched_lines: AtomicU64,
    span_capped: AtomicBool,
    timed_out: AtomicBool,
    size_skipped: AtomicBool,
    binary_quit: AtomicBool,
    binary_files: Mutex<Vec<String>>,
    binary_file_count: AtomicU32,
    skipped_binary_count: AtomicU32,
    cancelled: AtomicBool,
    /// Set when the walk must end now (deadline, cancellation, driver timeout).
    stop: AtomicBool,
    error_count: AtomicU32,
    first_error: Mutex<Option<String>>,
}

impl CollectState {
    fn new() -> Self {
        Self {
            started: Instant::now(),
            recs: Mutex::new(Vec::new()),
            files_searched: AtomicU64::new(0),
            bytes_searched: AtomicU64::new(0),
            files_matched: AtomicU64::new(0),
            submatches: AtomicU64::new(0),
            matched_lines: AtomicU64::new(0),
            span_capped: AtomicBool::new(false),
            timed_out: AtomicBool::new(false),
            size_skipped: AtomicBool::new(false),
            binary_quit: AtomicBool::new(false),
            binary_files: Mutex::new(Vec::new()),
            binary_file_count: AtomicU32::new(0),
            skipped_binary_count: AtomicU32::new(0),
            cancelled: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            error_count: AtomicU32::new(0),
            first_error: Mutex::new(None),
        }
    }

    /// Remember a file searched only up to its first NUL byte. The kept
    /// paths are the smallest in path order, so the list does not depend on
    /// which walk worker finished first.
    fn record_binary(&self, path: &Path) {
        self.binary_quit.store(true, Ordering::Relaxed);
        let _ = self
            .binary_file_count
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(1))
            });
        if let Ok(mut files) = self.binary_files.lock() {
            files.push(path.to_string_lossy().into_owned());
            files.sort();
            files.truncate(MAX_REPORTED_BINARY_FILES);
        }
    }

    fn record_error(&self, message: String) {
        // Saturate: a count stuck at u32::MAX still reports incomplete coverage.
        let _ = self
            .error_count
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(1))
            });
        if let Ok(mut detail) = self.first_error.lock()
            && detail.is_none()
        {
            *detail = Some(message.chars().take(512).collect());
        }
    }

    /// Count one kept file in the search-wide totals. Totals are `u64`, so they
    /// cannot overflow from per-file `u32` counts; output saturates to `u32`.
    fn record_kept(&self, submatches: u32, matched_lines: u32) {
        self.files_matched.fetch_add(1, Ordering::Relaxed);
        self.submatches
            .fetch_add(u64::from(submatches), Ordering::Relaxed);
        self.matched_lines
            .fetch_add(u64::from(matched_lines), Ordering::Relaxed);
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Take the files finished so far plus every counter. Files a worker
    /// finishes after the snapshot are not included.
    fn snapshot(&self) -> CollectResult {
        let recs = self
            .recs
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default();
        let first_error = self
            .first_error
            .lock()
            .ok()
            .and_then(|mut detail| detail.take());
        CollectResult {
            recs,
            files_searched: self.files_searched.load(Ordering::Relaxed),
            bytes_searched: self.bytes_searched.load(Ordering::Relaxed),
            files_matched: self.files_matched.load(Ordering::Relaxed),
            submatches: self.submatches.load(Ordering::Relaxed),
            matched_lines: self.matched_lines.load(Ordering::Relaxed),
            elapsed: self.started.elapsed(),
            span_capped: self.span_capped.load(Ordering::Relaxed),
            timed_out: self.timed_out.load(Ordering::Relaxed),
            size_skipped: self.size_skipped.load(Ordering::Relaxed),
            binary_quit: self.binary_quit.load(Ordering::Relaxed),
            binary_files: self
                .binary_files
                .lock()
                .map(|files| files.clone())
                .unwrap_or_default(),
            binary_file_count: self.binary_file_count.load(Ordering::Relaxed),
            skipped_binary_count: self.skipped_binary_count.load(Ordering::Relaxed),
            cancelled: self.cancelled.load(Ordering::Relaxed),
            error_count: self.error_count.load(Ordering::Relaxed),
            first_error,
        }
    }
}

/// Saturating `u64` → `u32` for the public (napi-compatible) `u32` stats.
fn saturate_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// `grep_searcher::Sink` that accumulates matches/contexts for one file and,
/// using the matcher, derives the 0-based UTF-16 column of the first submatch.
struct CollectSink<'a, M: Matcher> {
    matcher: &'a M,
    entry: FileEntry,
    submatches: u32,
    matched_lines: u32,
    work: MatchWork,
    /// Chars of context around each span in only-matching mode.
    match_window: usize,
    /// Accumulated only-matching spans for this file.
    om_matches: Vec<RipgrepMatch>,
    /// A retained span limit must never be reported as an exhaustive search.
    span_cap_reached: bool,
    /// Wall-clock ceiling for this file's search (see [`PCRE2_SEARCH_DEADLINE`]).
    /// `None` disables the deadline (linear engines don't need it).
    deadline: Option<Instant>,
    /// Search-wide stop flag, polled between matched lines.
    stop: &'a AtomicBool,
    /// Set when `deadline` was hit and the search was stopped early.
    deadline_hit: bool,
    /// Absolute offset of the first NUL byte when the file was quit as binary.
    /// The searcher stops there, so bytes after it are unsearched.
    binary_offset: Option<u64>,
    /// Summed [`relevance::line_weight`] (only with `work.weigh_lines`).
    line_weight: u32,
    /// A matched line declares the matched name (only with `work.weigh_lines`).
    declares: bool,
}

impl<'a, M: Matcher> CollectSink<'a, M> {
    fn new(
        matcher: &'a M,
        work: MatchWork,
        match_window: usize,
        deadline: Option<Instant>,
        stop: &'a AtomicBool,
    ) -> Self {
        Self {
            matcher,
            entry: FileEntry::new(),
            submatches: 0,
            matched_lines: 0,
            work,
            match_window,
            om_matches: Vec::new(),
            span_cap_reached: false,
            deadline,
            stop,
            deadline_hit: false,
            binary_offset: None,
            line_weight: 0,
            declares: false,
        }
    }
}

impl<M: Matcher> Sink for CollectSink<'_, M> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> std::io::Result<bool> {
        // Stop searching this file *before* doing more per-line work. Returning
        // Ok(false) ends the search cleanly and keeps the matches collected so far.
        if self.stop.load(Ordering::Relaxed) {
            return Ok(false);
        }
        if let Some(deadline) = self.deadline
            && Instant::now() >= deadline
        {
            self.deadline_hit = true;
            return Ok(false);
        }
        let line_number = mat.line_number().unwrap_or(0) as u32;
        let bytes = mat.bytes();
        let mut count: u32 = 0;
        let mut first_start = None;

        if self.work.collect_spans {
            let line_cow = String::from_utf8_lossy(bytes);
            let line_text = strip_trailing_newline(line_cow.into_owned());
            // Emit one span per submatch with its own UTF-16 column, rather than
            // one whole-line match. find_iter yields non-overlapping matches L→R.
            let matcher = self.matcher;
            let window = self.match_window;
            let om = &mut self.om_matches;
            matcher
                .find_iter(bytes, |m| {
                    count = count.saturating_add(1);
                    first_start.get_or_insert(m.start());
                    if count <= MAX_ONLY_MATCHING_PER_LINE {
                        let (start, end) =
                            (lossy_offset(bytes, m.start()), lossy_offset(bytes, m.end()));
                        let value = span_value(&line_text, start, end, window);
                        let column =
                            byte_to_char_offset_inner(&line_text, start.min(line_text.len()))
                                as u32;
                        om.push(RipgrepMatch {
                            line: line_number,
                            column,
                            value,
                            count: None,
                            kind: None,
                            score_hint: None,
                            rank: None,
                            original_chars: None,
                        });
                    } else {
                        self.span_cap_reached = true;
                    }
                    true
                })
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            // A matched line with no enumerable submatch (e.g. zero-width or
            // multiline block) still yields one span: the whole line.
            if count == 0 {
                count = 1;
                self.om_matches.push(RipgrepMatch {
                    line: line_number,
                    column: 0,
                    value: line_text,
                    count: None,
                    kind: None,
                    score_hint: None,
                    rank: None,
                    original_chars: None,
                });
            }
        } else if self.work.enumerate_submatches {
            self.matcher
                .find_iter(bytes, |matched| {
                    count = count.saturating_add(1);
                    first_start.get_or_insert(matched.start());
                    true
                })
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            if self.work.materialize_line {
                let line_cow = String::from_utf8_lossy(bytes);
                let line_text = strip_trailing_newline(line_cow.into_owned());
                let column = byte_to_char_offset_inner(
                    &line_text,
                    lossy_offset(bytes, first_start.unwrap_or(0)).min(line_text.len()),
                ) as u32;
                self.entry.raw_matches.push(RawMatch {
                    line_text,
                    line_number,
                    column,
                });
            }
        }

        if self.work.weigh_lines {
            // Count-lines never enumerates submatches: find the first one here.
            let first = match first_start {
                Some(start) => start,
                None => self
                    .matcher
                    .find(bytes)
                    .map_err(|error| std::io::Error::other(error.to_string()))?
                    .map_or(0, |matched| matched.start()),
            };
            let weight = relevance::line_weight(bytes, first);
            self.declares |= weight == relevance::DECLARATION_WEIGHT;
            self.line_weight = self.line_weight.saturating_add(weight);
        }
        self.submatches = self.submatches.saturating_add(count.max(1));
        self.matched_lines = self.matched_lines.saturating_add(1);
        Ok(true)
    }

    fn context(&mut self, _searcher: &Searcher, ctx: &SinkContext<'_>) -> std::io::Result<bool> {
        let line_number = ctx.line_number().unwrap_or(0) as u32;
        let line_cow = String::from_utf8_lossy(ctx.bytes());
        let line_text = strip_trailing_newline(line_cow.into_owned());
        self.entry.contexts.insert(line_number, line_text);
        Ok(true)
    }

    fn binary_data(
        &mut self,
        _searcher: &Searcher,
        binary_byte_offset: u64,
    ) -> std::io::Result<bool> {
        // A NUL byte was detected; with `BinaryDetection::quit` the searcher
        // stops here. Record the offset so the pre-NUL prefix can be searched
        // and the file reported as partially covered (`binaryQuit`).
        self.binary_offset = Some(binary_byte_offset);
        Ok(true)
    }
}

/// Build the gitignore-aware walker with `-g` overrides, `-t` types, hidden and
/// no-ignore handling. Results are sorted after parallel traversal to reproduce
/// `--sort`/`--sortr` deterministically.
fn build_walk_builder(opts: &RipgrepSearchOptions) -> Result<WalkBuilder> {
    let mut wb = WalkBuilder::new(&opts.path);
    let no_ignore = opts.no_ignore.unwrap_or(false);
    wb.ignore(!no_ignore)
        .git_ignore(!no_ignore)
        .git_global(!no_ignore)
        .git_exclude(!no_ignore)
        .parents(!no_ignore)
        // hidden(true) means "skip hidden"; rg searches them only with --hidden.
        .hidden(!opts.hidden.unwrap_or(false))
        .follow_links(false);

    // ignore::WalkBuilder counts the root as depth 0 and its direct children as
    // depth 1. The public tool contract counts files in the root as maxDepth 0.
    if let Some(max_depth) = opts.max_depth {
        wb.max_depth(Some(max_depth as usize + 1));
    }

    if let Some(lang) = opts.lang_type.as_deref().filter(|l| !l.is_empty()) {
        let mut tb = TypesBuilder::new();
        tb.add_defaults();
        tb.select(lang);
        wb.types(tb.build().map_err(to_napi_err)?);
    }

    let has_globs = opts.include.as_ref().is_some_and(|v| !v.is_empty())
        || opts.exclude.as_ref().is_some_and(|v| !v.is_empty())
        || opts.exclude_dir.as_ref().is_some_and(|v| !v.is_empty());
    if has_globs {
        let mut ob = OverrideBuilder::new(&opts.path);
        if let Some(include) = &opts.include {
            for glob in include {
                ob.add(glob).map_err(to_napi_err)?;
            }
        }
        if let Some(exclude) = &opts.exclude {
            for glob in exclude {
                ob.add(&format!("!{glob}")).map_err(to_napi_err)?;
            }
        }
        if let Some(exclude_dir) = &opts.exclude_dir {
            for dir in exclude_dir {
                // `excludeDir: ["sub/"]` must behave like `["sub"]`, not build `!sub//`.
                let dir = dir.trim_end_matches('/');
                if dir.is_empty() {
                    continue;
                }
                ob.add(&format!("!{dir}/")).map_err(to_napi_err)?;
            }
        }
        wb.overrides(ob.build().map_err(to_napi_err)?);
    }

    Ok(wb)
}

fn capture_sort_time(opts: &RipgrepSearchOptions, entry: &ignore::DirEntry) -> Option<SystemTime> {
    match opts.sort.as_deref() {
        Some("modified") => entry.metadata().ok()?.modified().ok(),
        Some("accessed") => entry.metadata().ok()?.accessed().ok(),
        Some("created") => entry.metadata().ok()?.created().ok(),
        _ => None,
    }
}

fn build_searcher(
    opts: &RipgrepSearchOptions,
    context_lines: u32,
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
    if context_lines > 0 {
        sb.before_context(context_lines as usize);
        sb.after_context(context_lines as usize);
    }
    sb.build()
}

/// Open a walk entry for searching without following a symlink, and confirm
/// the opened handle is a regular file. The walk reported a regular file, but
/// the path can be replaced (by a symlink, FIFO, or directory) before the
/// open; checking the handle closes that window. Unix opens non-blocking so a
/// FIFO swapped in cannot hang the open. Returns the file and its length.
fn open_regular(path: &Path) -> std::io::Result<(std::fs::File, u64)> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT: open a symlink itself, not its target.
        options.custom_flags(0x0020_0000);
    }
    let file = options.open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(std::io::Error::other(
            "not a regular file when opened (the path changed after the walk)",
        ));
    }
    Ok((file, meta.len()))
}

/// Read the first `len` bytes of an already-open file.
/// Bytes before a first NUL that read as text: valid UTF-8 without control
/// bytes other than whitespace and ESC. Binary headers (PNG, ELF, Mach-O,
/// archives) fail this within their first bytes.
fn is_text_prefix(prefix: &[u8]) -> bool {
    std::str::from_utf8(prefix).is_ok_and(|text| {
        !text.bytes().any(|byte| {
            byte == 0x7f || (byte < 0x20 && !matches!(byte, b'\t' | b'\n' | b'\r' | 0x0c | 0x1b))
        })
    })
}

fn read_prefix(file: &std::fs::File, len: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut handle = file;
    handle.seek(SeekFrom::Start(0))?;
    let mut prefix = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    handle.take(len).read_to_end(&mut prefix)?;
    Ok(prefix)
}

/// Per-worker collection buffer. Workers push into their own `Vec` and merge
/// into the shared state on drop, after `build_parallel().run()` finishes,
/// instead of taking a global mutex per matched file. With `eager` set (the
/// PCRE2 path) every file is merged at once so the driver's snapshot sees it.
///
/// Retention is bounded without depending on walk order: under a total
/// [`compare_recs`] order, the global first `limit` records are the first
/// `limit` of the union of each worker's first `limit`, so every buffer is
/// pruned to `limit` whenever it reaches twice that. Traversal order keeps the
/// first `limit` records pushed by its single worker.
struct WorkerRecs<'a> {
    local: Vec<FileRec>,
    state: &'a CollectState,
    opts: &'a RipgrepSearchOptions,
    mode: Mode,
    limit: Option<usize>,
    eager: bool,
}

impl WorkerRecs<'_> {
    fn push(&mut self, rec: FileRec) {
        self.state.record_kept(rec.submatches, rec.matched_lines);
        if self.eager {
            if let Ok(mut shared) = self.state.recs.lock() {
                retain_into(self.opts, self.mode, self.limit, &mut shared, rec);
            }
        } else {
            retain_into(self.opts, self.mode, self.limit, &mut self.local, rec);
        }
    }
}

impl Drop for WorkerRecs<'_> {
    fn drop(&mut self) {
        if self.local.is_empty() {
            return;
        }
        if let Ok(mut shared) = self.state.recs.lock() {
            for rec in self.local.drain(..) {
                retain_into(self.opts, self.mode, self.limit, &mut shared, rec);
            }
        }
    }
}

/// Push `rec` into `buf` while keeping at most `2 * limit` records, pruning to
/// the first `limit` under the final result order.
fn retain_into(
    opts: &RipgrepSearchOptions,
    mode: Mode,
    limit: Option<usize>,
    buf: &mut Vec<FileRec>,
    rec: FileRec,
) {
    let Some(limit) = limit else {
        buf.push(rec);
        return;
    };
    if preserves_traversal_order(opts) {
        if buf.len() < limit {
            buf.push(rec);
        }
        return;
    }
    buf.push(rec);
    if buf.len() >= limit.saturating_mul(2) {
        buf.sort_by(|a, b| compare_recs(opts, mode, a, b));
        buf.truncate(limit);
    }
}

fn collection_limit(opts: &RipgrepSearchOptions) -> Option<usize> {
    opts.max_collected_files
        .map(|n| n as usize)
        .filter(|n| *n > 0)
}

fn elapsed_human(elapsed: Duration) -> String {
    format!("{:.6}s", elapsed.as_secs_f64())
}

fn bytes_as_i64(bytes: u64) -> i64 {
    bytes.min(i64::MAX as u64) as i64
}

/// Result of searching one file.
struct FileOutcome {
    entry: FileEntry,
    submatches: u32,
    matched_lines: u32,
    om_matches: Vec<RipgrepMatch>,
    span_cap_reached: bool,
    deadline_hit: bool,
    binary: bool,
    /// The first NUL came before any text: nothing text-searchable was lost.
    opaque: bool,
    line_weight: u32,
    declares: bool,
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
            declares: sink.declares,
        }
    }
}

/// Per-worker searchers: the walk searcher quits at the first NUL; the prefix
/// searcher (built on first use) re-searches the NUL-free bytes before it.
struct FileSearcher<'a, M: Matcher> {
    opts: &'a RipgrepSearchOptions,
    matcher: &'a M,
    searcher: Searcher,
    prefix_searcher: Option<Searcher>,
    context_lines: u32,
    work: MatchWork,
    match_window: usize,
    deadline: Option<Instant>,
    stop: &'a AtomicBool,
}

impl<M: Matcher> FileSearcher<'_, M> {
    /// Search an opened file. When it is quit as binary, the searcher has
    /// dropped the whole buffer holding the NUL, so the NUL-free prefix is
    /// searched again from byte 0 and its matches replace the first pass.
    fn search(&mut self, file: &std::fs::File) -> std::io::Result<FileOutcome> {
        let mut sink = CollectSink::new(
            self.matcher,
            self.work,
            self.match_window,
            self.deadline,
            self.stop,
        );
        self.searcher.search_file(self.matcher, file, &mut sink)?;
        let Some(offset) = sink.binary_offset else {
            return Ok(sink.into());
        };
        if offset == 0 || offset > MAX_BINARY_PREFIX_BYTES {
            return Ok(sink.into());
        }
        let prefix = read_prefix(file, offset)?;
        let (opts, context_lines) = (self.opts, self.context_lines);
        let prefix_searcher = self
            .prefix_searcher
            .get_or_insert_with(|| build_searcher(opts, context_lines, BinaryDetection::none()));
        let mut prefix_sink = CollectSink::new(
            self.matcher,
            self.work,
            self.match_window,
            self.deadline,
            self.stop,
        );
        prefix_searcher.search_slice(self.matcher, &prefix, &mut prefix_sink)?;
        let mut outcome = FileOutcome::from(prefix_sink);
        outcome.binary = true;
        outcome.opaque = !is_text_prefix(&prefix);
        Ok(outcome)
    }
}

/// How a walk is bounded and where finished files go.
#[derive(Clone, Copy)]
struct WalkControl<'a> {
    /// Wall-clock ceiling (PCRE2 only); `None` for linear engines.
    deadline: Option<Instant>,
    /// Caller cancellation, polled before every walk entry.
    cancelled: &'a (dyn Fn() -> bool + Sync),
    /// Merge each finished file into the shared state immediately.
    eager: bool,
}

/// Run a parallel ignore walk + per-file search for a concrete matcher type,
/// writing into `state`. The walk stops at the next entry once `cancelled()`
/// returns true, `state.stop` is raised, or `deadline` passes.
fn collect<M: Matcher + Sync>(
    opts: &RipgrepSearchOptions,
    matcher: &M,
    mode: Mode,
    path_filter: Arc<dyn RipgrepPathFilter>,
    state: &CollectState,
    control: WalkControl<'_>,
) -> Result<()> {
    let WalkControl {
        deadline,
        cancelled,
        eager,
    } = control;
    let only_matching = opts.only_matching.unwrap_or(false);
    // only-matching emits bare spans; ripgrep's `-o` ignores `-C` context too.
    let context_lines = if mode == Mode::Normal && !only_matching {
        opts.context_lines.unwrap_or(0)
    } else {
        0
    };
    let match_window = opts.match_window.unwrap_or(0) as usize;
    let keep_unmatched = mode == Mode::FilesWithoutMatch;
    let limit = collection_limit(opts);

    let max_file_bytes =
        opts.max_file_bytes
            .map(u64::from)
            .unwrap_or(if opts.multiline.unwrap_or(false) {
                DEFAULT_MAX_SEARCH_FILE_BYTES
            } else {
                DEFAULT_MAX_LINE_SEARCH_FILE_BYTES
            });

    // Traversal ordering is only stable if a single worker drains the walk in a
    // fixed order; the default parallel walk merges worker buffers arbitrarily.
    let mut walk_builder = build_walk_builder(opts)?;
    if preserves_traversal_order(opts) {
        walk_builder.threads(1);
    } else if let Some(threads) = opts.walk_threads {
        walk_builder.threads(threads as usize);
    }

    let identifier = identifier_search(opts);
    walk_builder.build_parallel().run(|| {
        let path_filter = Arc::clone(&path_filter);
        let mut worker_recs = WorkerRecs {
            local: Vec::new(),
            state,
            opts,
            mode,
            limit,
            eager,
        };
        let mut files = FileSearcher {
            opts,
            matcher,
            searcher: build_searcher(opts, context_lines, BinaryDetection::quit(b'\x00')),
            prefix_searcher: None,
            context_lines,
            // `mode`/`only_matching` are invariant for the whole search.
            work: MatchWork {
                weigh_lines: ranks_by_relevance(opts)
                    && lists_match_density(mode)
                    && !opts.invert_match.unwrap_or(false),
                ..match_work(mode, only_matching)
            },
            match_window,
            deadline,
            stop: &state.stop,
        };

        Box::new(move |dent| {
            // Between entries: end the walk on stop, cancellation, or the
            // deadline, keeping the files already finished.
            if state.stopped() {
                return WalkState::Quit;
            }
            if cancelled() {
                state.cancelled.store(true, Ordering::Relaxed);
                state.stop.store(true, Ordering::Relaxed);
                return WalkState::Quit;
            }
            if let Some(deadline) = deadline
                && Instant::now() >= deadline
            {
                state.timed_out.store(true, Ordering::Relaxed);
                state.stop.store(true, Ordering::Relaxed);
                return WalkState::Quit;
            }
            let dent = match dent {
                Ok(d) => d,
                Err(error) => {
                    state.record_error(error.to_string());
                    return WalkState::Continue;
                }
            };
            let is_dir = dent.file_type().is_some_and(|kind| kind.is_dir());
            if !path_filter.allows(dent.path(), is_dir) {
                return if is_dir {
                    WalkState::Skip
                } else {
                    WalkState::Continue
                };
            }
            if !dent.file_type().is_some_and(|t| t.is_file()) {
                return WalkState::Continue;
            }
            let path: &Path = dent.path();

            // Skip files above the byte ceiling before opening them, so a
            // pathological multi-GB file cannot force a giant buffer + lossy
            // copy. Surfaced as `maxFileSize`; not counted in `files_searched`.
            let over_ceiling = |len: u64| {
                if len > max_file_bytes {
                    state.size_skipped.store(true, Ordering::Relaxed);
                    true
                } else {
                    false
                }
            };
            if dent
                .metadata()
                .ok()
                .is_some_and(|meta| over_ceiling(meta.len()))
            {
                return WalkState::Continue;
            }
            // Keep successful files, but report incomplete coverage when
            // opening or matching fails. A skipped file proves no absence.
            let (file, file_len) = match open_regular(path) {
                Ok(opened) => opened,
                Err(error) => {
                    state.record_error(format!("{}: {error}", path.display()));
                    return WalkState::Continue;
                }
            };
            if over_ceiling(file_len) {
                return WalkState::Continue;
            }
            let outcome = match files.search(&file) {
                Ok(outcome) => outcome,
                Err(error) => {
                    state.record_error(format!("{}: {error}", path.display()));
                    return WalkState::Continue;
                }
            };
            drop(file);
            if outcome.span_cap_reached {
                state.span_capped.store(true, Ordering::Relaxed);
            }
            if outcome.deadline_hit {
                state.timed_out.store(true, Ordering::Relaxed);
            }
            state.files_searched.fetch_add(1, Ordering::Relaxed);
            state.bytes_searched.fetch_add(file_len, Ordering::Relaxed);
            if outcome.opaque {
                state.skipped_binary_count.fetch_add(1, Ordering::Relaxed);
            } else if outcome.binary {
                // Text after the NUL was not searched: coverage is partial.
                state.record_binary(path);
            }

            let has_match = outcome.matched_lines > 0;
            // A binary file quit before any match is *unknown*, not "without
            // match": rg --files-without-match never lists it either.
            if outcome.binary && !has_match && keep_unmatched {
                return WalkState::Continue;
            }
            if has_match == keep_unmatched {
                // Normal/files-only/count modes keep matched files;
                // files-without-match keeps the rest.
                return WalkState::Continue;
            }

            let demoted = ranks_by_relevance(opts)
                && relevance::is_demoted_path(
                    &path
                        .strip_prefix(&opts.path)
                        .unwrap_or(path)
                        .to_string_lossy(),
                );
            worker_recs.push(FileRec {
                path: dent.path().to_string_lossy().into_owned(),
                entry: outcome.entry,
                matched_lines: outcome.matched_lines,
                submatches: outcome.submatches,
                om_matches: outcome.om_matches,
                sort_time: capture_sort_time(opts, &dent),
                line_weight: outcome.line_weight,
                demoted,
                declares: outcome.declares && identifier,
            });
            WalkState::Continue
        })
    });
    Ok(())
}

/// Sort `recs` into the final result order and truncate to the collection cap.
/// Returns whether any records were dropped by the cap.
fn sort_and_cap(opts: &RipgrepSearchOptions, mode: Mode, recs: &mut Vec<FileRec>) -> bool {
    sort_recs(opts, mode, recs);
    if let Some(max) = collection_limit(opts)
        && recs.len() > max
    {
        recs.truncate(max);
        return true;
    }
    false
}

fn sort_recs(opts: &RipgrepSearchOptions, mode: Mode, recs: &mut [FileRec]) {
    if preserves_traversal_order(opts) {
        return;
    }
    recs.sort_by(|a, b| compare_recs(opts, mode, a, b));
}

/// The per-file weight `matchCount` ordering ranks by: the unit the view
/// reports as each file's match count (matched lines for line views, spans or
/// submatches otherwise).
fn rank_weight(opts: &RipgrepSearchOptions, mode: Mode, rec: &FileRec) -> u32 {
    match mode {
        Mode::CountLines => rec.matched_lines,
        Mode::Normal if !opts.only_matching.unwrap_or(false) => rec.matched_lines,
        _ => rec.submatches,
    }
}

/// Total order over collected files, including `sort_reverse`. Every key ends
/// with the path so a cap always retains the same records.
///
/// * `modified` / `accessed` / `created`: ascending timestamp.
/// * `matchCount`: descending [`rank_weight`] (the most-matched files survive
///   the collection cap).
/// * `relevance`: an [`identifier_search`] first ranks source files whose hit
///   declares the name (the definition answers "where is X"), then descending
///   [`rank_weight`], then source paths before test, generated, and vendored
///   paths, then descending summed line weight (declaration > code >
///   comment/string), see [`relevance`]. Path-list views have no per-file
///   density: source paths first, then path.
/// * default and `path`: lexicographic by full path, matching `rg --sort path`.
fn compare_recs(
    opts: &RipgrepSearchOptions,
    mode: Mode,
    a: &FileRec,
    b: &FileRec,
) -> std::cmp::Ordering {
    let order = match opts.sort.as_deref() {
        Some("modified" | "accessed" | "created") => a
            .sort_time
            .cmp(&b.sort_time)
            .then_with(|| a.path.cmp(&b.path)),
        Some("matchCount") => rank_weight(opts, mode, b)
            .cmp(&rank_weight(opts, mode, a))
            .then_with(|| a.path.cmp(&b.path)),
        Some("relevance") if lists_match_density(mode) => (b.declares && !b.demoted)
            .cmp(&(a.declares && !a.demoted))
            .then_with(|| rank_weight(opts, mode, b).cmp(&rank_weight(opts, mode, a)))
            .then_with(|| a.demoted.cmp(&b.demoted))
            .then_with(|| b.line_weight.cmp(&a.line_weight))
            .then_with(|| a.path.cmp(&b.path)),
        Some("relevance") => a.demoted.cmp(&b.demoted).then_with(|| a.path.cmp(&b.path)),
        _ => a.path.cmp(&b.path),
    };
    if opts.sort_reverse.unwrap_or(false) {
        order.reverse()
    } else {
        order
    }
}

fn preserves_traversal_order(opts: &RipgrepSearchOptions) -> bool {
    opts.sort.as_deref() == Some("traversal")
}

fn collapse_unique_matches(matches: Vec<RipgrepMatch>, include_counts: bool) -> Vec<RipgrepMatch> {
    let mut seen: HashMap<String, usize> = HashMap::with_capacity(matches.len());
    let mut unique: Vec<RipgrepMatch> = Vec::with_capacity(matches.len());

    for mut matched in matches {
        if let Some(index) = seen.get(matched.value.as_str()).copied() {
            if include_counts {
                let next = unique[index].count.unwrap_or(1).saturating_add(1);
                unique[index].count = Some(next);
            }
            continue;
        }

        if include_counts {
            matched.count = Some(1);
        }
        seen.insert(matched.value.clone(), unique.len());
        unique.push(matched);
    }

    if include_counts {
        unique.sort_by_key(|matched| std::cmp::Reverse(matched.count.unwrap_or(1)));
    }

    unique
}

fn build_result(
    opts: &RipgrepSearchOptions,
    mode: Mode,
    collected: CollectResult,
) -> RipgrepParseResult {
    let CollectResult {
        mut recs,
        files_searched,
        bytes_searched,
        files_matched,
        submatches,
        matched_lines,
        elapsed,
        span_capped,
        timed_out,
        size_skipped,
        binary_quit,
        binary_files,
        binary_file_count,
        skipped_binary_count,
        cancelled,
        error_count,
        first_error,
    } = collected;
    // Workers retain a bounded, order-independent candidate set; the final
    // sort + truncate keeps the first `max_collected_files` in result order.
    let retained_truncated = sort_and_cap(opts, mode, &mut recs);
    let cap_truncated = retained_truncated || files_matched > recs.len() as u64;

    // Assemble cap reasons in a fixed order so single-cause results carry a
    // stable, exact reason string.
    let mut cap_reasons: Vec<&str> = Vec::new();
    if cap_truncated {
        cap_reasons.push("maxCollectedFiles");
    }
    if span_capped {
        cap_reasons.push("maxOnlyMatchingPerLine");
    }
    if timed_out {
        cap_reasons.push("pcre2Deadline");
    }
    if size_skipped {
        cap_reasons.push("maxFileSize");
    }
    if cancelled {
        cap_reasons.push("cancelled");
    }
    if binary_quit {
        cap_reasons.push("binaryQuit");
    }
    let capped = !cap_reasons.is_empty();
    let cap_reason = capped.then(|| cap_reasons.join(", "));

    let context_lines = opts.context_lines.unwrap_or(0);
    let max_snippet = opts.max_snippet_chars.unwrap_or(DEFAULT_MAX_SNIPPET_CHARS) as usize;

    // Totals cover every kept file, including files the cap dropped from the
    // returned list (see `maxCollectedFiles`).
    let files_matched = saturate_u32(files_matched);
    let total_submatches = saturate_u32(submatches);
    let total_matched_lines = saturate_u32(matched_lines);

    let only_matching = opts.only_matching.unwrap_or(false);
    let unique = opts.unique.unwrap_or(false) || opts.count_unique.unwrap_or(false);
    let count_unique = opts.count_unique.unwrap_or(false);
    let bytes_searched = Some(bytes_as_i64(bytes_searched));
    let search_time = Some(elapsed_human(elapsed));
    let mut files: Vec<RipgrepFile> = recs
        .into_iter()
        .map(|r| match mode {
            Mode::Normal if only_matching => {
                let matches = if unique {
                    collapse_unique_matches(r.om_matches, count_unique)
                } else {
                    r.om_matches
                };
                RipgrepFile {
                    path: r.path,
                    match_count: matches.len() as u32,
                    matches,
                }
            }
            Mode::Normal => assemble_file(r.path, &r.entry, context_lines, max_snippet),
            // files-only / files-without-match: path list, matchCount 1, no
            // snippets.
            Mode::FilesOnly | Mode::FilesWithoutMatch => RipgrepFile {
                path: r.path,
                match_count: 1,
                matches: Vec::new(),
            },
            Mode::CountLines => RipgrepFile {
                path: r.path,
                match_count: r.matched_lines,
                matches: Vec::new(),
            },
            Mode::CountMatches => RipgrepFile {
                path: r.path,
                match_count: r.submatches,
                matches: Vec::new(),
            },
        })
        .collect();

    // Optional AST classification: only meaningful for Normal mode (the other
    // modes carry no per-line snippets to anchor a parse position).
    if opts.classify_matches.unwrap_or(false) && matches!(mode, Mode::Normal) {
        classify::classify_ripgrep_files(&mut files, classify::DEFAULT_CLASSIFY_FILE_CAP);
    }

    // Every view describes the same collection. Only count-lines changes the
    // unit of match_count; completeness and resource evidence must stay shared.
    let stats = RipgrepStats {
        match_count: Some(if matches!(mode, Mode::CountLines) {
            total_matched_lines
        } else {
            total_submatches
        }),
        matched_lines: Some(total_matched_lines),
        files_matched: Some(files_matched),
        files_searched: Some(saturate_u32(files_searched)),
        bytes_searched,
        search_time,
        capped: Some(capped),
        cap_reason,
        error_count: Some(error_count),
        first_error,
        binary_files: (!binary_files.is_empty()).then_some(binary_files),
        binary_file_count: binary_quit.then_some(binary_file_count),
        skipped_binary_count: (skipped_binary_count > 0).then_some(skipped_binary_count),
    };

    RipgrepParseResult { files, stats }
}

/// Build the appropriate matcher (default Rust regex, or PCRE2 for `-P`) and run
/// the search. `fixed_string` is honored by escaping the pattern for the regex
/// engine; the CLI gave `-F` precedence over `-P`, so PCRE2 only applies when
/// `fixed_string` is not set.
pub(crate) fn search(opts: RipgrepSearchOptions) -> Result<RipgrepParseResult> {
    search_filtered(opts, Arc::new(AllowAll))
}

pub(crate) fn search_filtered(
    opts: RipgrepSearchOptions,
    path_filter: Arc<dyn RipgrepPathFilter>,
) -> Result<RipgrepParseResult> {
    search_cancellable(opts, path_filter, &|| false)
}

/// [`search_filtered`] that stops walking at the next entry once `cancelled`
/// returns true. The partial result carries `capReason` `cancelled`.
pub(crate) fn search_cancellable(
    opts: RipgrepSearchOptions,
    path_filter: Arc<dyn RipgrepPathFilter>,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Result<RipgrepParseResult> {
    #[cfg(feature = "pcre2")]
    {
        search_with_limits(opts, path_filter, cancelled, PCRE2_LIMITS)
    }
    #[cfg(not(feature = "pcre2"))]
    {
        search_with_limits(opts, path_filter, cancelled)
    }
}

fn search_with_limits(
    opts: RipgrepSearchOptions,
    path_filter: Arc<dyn RipgrepPathFilter>,
    cancelled: &(dyn Fn() -> bool + Sync),
    #[cfg(feature = "pcre2")] limits: Pcre2Limits,
) -> Result<RipgrepParseResult> {
    let mode = resolve_mode(&opts);

    if (opts.unique.unwrap_or(false) || opts.count_unique.unwrap_or(false))
        && !opts.only_matching.unwrap_or(false)
    {
        return Err(Error::new(
            Status::InvalidArg,
            "unique/countUnique require onlyMatching:true",
        ));
    }

    let case_sensitive = opts.case_sensitive.unwrap_or(false);
    let case_insensitive = !case_sensitive && opts.case_insensitive.unwrap_or(false);
    // Default (neither -s nor -i): smart-case, matching the builder's `-S`.
    let smart_case = !case_sensitive && !opts.case_insensitive.unwrap_or(false);
    let whole_word = opts.whole_word.unwrap_or(false);
    let multiline = opts.multiline.unwrap_or(false);
    let dotall = multiline && opts.multiline_dotall.unwrap_or(false);
    let fixed_string = opts.fixed_string.unwrap_or(false);
    let perl_regex = !fixed_string && opts.perl_regex.unwrap_or(false);

    // ── PCRE2 path (only compiled with the `pcre2` feature) ──────────────────────────────
    #[cfg(feature = "pcre2")]
    if perl_regex {
        let mut b = Pcre2MatcherBuilder::new();
        b.caseless(case_insensitive)
            .case_smart(smart_case)
            .word(whole_word)
            .multi_line(multiline)
            .dotall(dotall)
            .crlf(true)
            .utf(true)
            .ucp(true)
            .jit_if_available(true)
            .max_jit_stack_size(Some(PCRE2_MAX_JIT_STACK_BYTES));
        let matcher = b.build(&opts.pattern).map_err(to_napi_err)?;
        return search_pcre2(opts, mode, matcher, path_filter, cancelled, limits);
    }

    // ── Error when PCRE2 is not compiled in ──────────────────────────────────────────
    #[cfg(not(feature = "pcre2"))]
    if perl_regex {
        return Err(Error::new(
            Status::GenericFailure,
            "PCRE2 (`perl_regex` / `-P`) search is not available in this build; \
             use regex:\"rust\" (the default engine) instead.",
        ));
    }

    // ── Default Rust-regex path (always compiled) ───────────────────────────────────
    let mut b = RegexMatcherBuilder::new();
    b.case_insensitive(case_insensitive)
        .case_smart(smart_case)
        .word(whole_word)
        .multi_line(multiline)
        .dot_matches_new_line(dotall);
    // ripgrep parity + fast path: in line-oriented (non-multiline) search the
    // searcher feeds one terminator-stripped line at a time. Declaring the line
    // terminator lets grep-regex extract inner literals and cheaply skip lines
    // that cannot match (its primary perf lever) and forbids a pattern from
    // matching across `\n` (which the line-oriented sink could never surface
    // anyway). Multiline matchers span lines and must NOT set it — matching
    // ripgrep's `hiargs` matcher construction exactly.
    if !multiline {
        b.line_terminator(Some(b'\n'));
    }
    let pattern = if fixed_string {
        regex::escape(&opts.pattern)
    } else {
        opts.pattern.clone()
    };
    let matcher = b.build(&pattern).map_err(to_napi_err)?;
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

#[cfg(feature = "pcre2")]
/// Run a PCRE2 search on a worker thread bounded by `limits` (see
/// [`PCRE2_SEARCH_DEADLINE`]). The driver polls `cancelled` and the hard
/// deadline; on either it raises the shared stop flag and returns the files the
/// worker finished so far, flagged `cancelled` or `pcre2Deadline`.
fn search_pcre2(
    opts: RipgrepSearchOptions,
    mode: Mode,
    matcher: grep_pcre2::RegexMatcher,
    path_filter: Arc<dyn RipgrepPathFilter>,
    cancelled: &(dyn Fn() -> bool + Sync),
    limits: Pcre2Limits,
) -> Result<RipgrepParseResult> {
    // Bound concurrently live PCRE2 worker threads, including workers still
    // finishing an uninterruptible match after their driver returned.
    if !try_acquire_worker_slot(&ACTIVE_PCRE2_WORKERS, MAX_ACTIVE_PCRE2_WORKERS) {
        return Err(Error::new(
            Status::GenericFailure,
            "Too many concurrent PCRE2 (-P) searches are in flight (some past their wall-clock deadline are still finishing a match); retry shortly, or use regex:\"literal\"/the default engine.",
        ));
    }
    let started = Instant::now();
    let deadline = started + limits.deadline;
    let hard_deadline = deadline + limits.grace;
    let state = Arc::new(CollectState::new());
    let (tx, rx) = std::sync::mpsc::channel();
    let opts_worker = opts.clone();
    let state_worker = Arc::clone(&state);
    let spawned = std::thread::Builder::new()
        .name("pcre2-search".into())
        .spawn(move || {
            // Release the slot when this thread exits.
            let _slot = Pcre2WorkerSlot;
            let _ = tx.send(collect(
                &opts_worker,
                &matcher,
                mode,
                path_filter,
                &state_worker,
                WalkControl {
                    deadline: Some(deadline),
                    cancelled: &|| false,
                    eager: true,
                },
            ));
        });
    if spawned.is_err() {
        // Spawn failed: no worker will ever run to release the reserved slot.
        release_worker_slot(&ACTIVE_PCRE2_WORKERS);
    }
    spawned.map_err(to_napi_err)?;
    loop {
        if cancelled() {
            state.cancelled.store(true, Ordering::Relaxed);
            state.stop.store(true, Ordering::Relaxed);
            return Ok(build_result(&opts, mode, state.snapshot()));
        }
        let now = Instant::now();
        if now >= hard_deadline {
            state.timed_out.store(true, Ordering::Relaxed);
            state.stop.store(true, Ordering::Relaxed);
            return Ok(build_result(&opts, mode, state.snapshot()));
        }
        match rx.recv_timeout(PCRE2_DRIVER_POLL.min(hard_deadline - now)) {
            Ok(collected) => {
                collected?;
                return Ok(build_result(&opts, mode, state.snapshot()));
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err(Error::new(
                    Status::GenericFailure,
                    "PCRE2 search worker terminated unexpectedly",
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "ripgrep_search_tests.rs"]
mod tests;
