//! In-process ripgrep search.
//!
//! Octocode is its own source of ripgrep: instead of shelling out to an `rg`
//! binary (and bundling one via `@vscode/ripgrep`), this module drives
//! ripgrep's own library crates directly —
//!   * `grep` (grep-searcher + grep-regex + grep-printer) for the search engine,
//!   * the `pcre2` feature (grep-pcre2) for `-P` lookaround/backreferences,
//!   * `ignore` for the gitignore-aware walk, `-g` override globs and `-t` types.
//!
//! It replicates every flag the old `RipgrepCommandBuilder` emitted and returns
//! the same `RipgrepParseResult` shape the `--json` parser produced, with native
//! byte/time stats populated by the in-process search path.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant, SystemTime};

use crate::error::{Error, Result, Status};
use grep::matcher::Matcher;
use grep::pcre2::RegexMatcherBuilder as Pcre2MatcherBuilder;
use grep::regex::RegexMatcherBuilder;
use grep::searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::overrides::OverrideBuilder;
use ignore::types::TypesBuilder;
use ignore::{WalkBuilder, WalkState};

use crate::search::classify;
use crate::search::ripgrep_parser::{assemble_file, strip_trailing_newline, FileEntry, RawMatch};
use crate::text::utf8_offsets::byte_to_char_offset_inner;
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

/// Cap on emitted spans per line in only-matching mode, so a pathological
/// minified line with a huge number of hits can't blow up the result. The true
/// submatch count is still reported in stats.
const MAX_ONLY_MATCHING_PER_LINE: u32 = 1000;

/// Cap on PCRE2's JIT stack (1 MiB). A user `-P` pattern with catastrophic
/// backtracking (`(a+)+$`-class) exhausts this cap and fails fast per file
/// instead of spinning against the JIT's default 32 KB stack growth. Residual
/// risk: `grep-pcre2` 0.1 exposes no `match_limit`/`depth_limit` knob, so a
/// backtracking blowup that stays within the JIT stack is still only bounded by
/// PCRE2's internal default match limit (not the wall clock). Applied to every
/// PCRE2 matcher we build (search + pattern validation).
pub(crate) const PCRE2_MAX_JIT_STACK_BYTES: usize = 1 << 20;

fn to_napi_err<E: std::fmt::Display>(e: E) -> Error {
    Error::new(Status::GenericFailure, e.to_string())
}

/// Largest char boundary `<= i` (clamped to `s.len()`).
fn floor_char_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Smallest char boundary `>= i` (clamped to `s.len()`).
fn ceil_char_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
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
}

fn match_work(mode: Mode, only_matching: bool) -> MatchWork {
    MatchWork {
        materialize_line: mode == Mode::Normal,
        enumerate_submatches: mode != Mode::CountLines,
        collect_spans: mode == Mode::Normal && only_matching,
    }
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
}

struct CollectResult {
    recs: Vec<FileRec>,
    files_searched: u32,
    bytes_searched: u64,
    elapsed: Duration,
    capped: bool,
    cap_reason: Option<String>,
    error_count: u32,
    first_error: Option<String>,
}

fn record_collection_error(count: &AtomicU32, first: &Mutex<Option<String>>, message: String) {
    count.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut detail) = first.lock() {
        if detail.is_none() {
            *detail = Some(message.chars().take(512).collect());
        }
    }
}

/// `grep_searcher::Sink` that accumulates matches/contexts for one file and,
/// using the matcher, derives the 0-based UTF-16 column of the first submatch.
struct CollectSink<'a, M: Matcher> {
    matcher: &'a M,
    entry: &'a mut FileEntry,
    submatches: u32,
    matched_lines: u32,
    work: MatchWork,
    /// Chars of context around each span in only-matching mode.
    match_window: usize,
    /// Accumulated only-matching spans for this file.
    om_matches: Vec<RipgrepMatch>,
    /// A retained span limit must never be reported as an exhaustive search.
    span_cap_reached: bool,
}

impl<M: Matcher> Sink for CollectSink<'_, M> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> std::io::Result<bool> {
        let line_number = mat.line_number().unwrap_or(0) as u32;
        let bytes = mat.bytes();
        let mut count: u32 = 0;

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
                    if count <= MAX_ONLY_MATCHING_PER_LINE {
                        let value = span_value(&line_text, m.start(), m.end(), window);
                        let column =
                            byte_to_char_offset_inner(&line_text, m.start().min(line_text.len()))
                                as u32;
                        om.push(RipgrepMatch {
                            line: line_number,
                            column,
                            value,
                            count: None,
                            kind: None,
                            score_hint: None,
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
                });
            }
        } else if self.work.enumerate_submatches {
            let mut first_byte_col = None;
            self.matcher
                .find_iter(bytes, |matched| {
                    count = count.saturating_add(1);
                    if first_byte_col.is_none() {
                        first_byte_col = Some(matched.start());
                    }
                    true
                })
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            if self.work.materialize_line {
                let line_cow = String::from_utf8_lossy(bytes);
                let line_text = strip_trailing_newline(line_cow.into_owned());
                let column = byte_to_char_offset_inner(
                    &line_text,
                    first_byte_col.unwrap_or(0).min(line_text.len()),
                ) as u32;
                self.entry.raw_matches.push(RawMatch {
                    line_text,
                    line_number,
                    column,
                });
            }
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

fn build_searcher(opts: &RipgrepSearchOptions, context_lines: u32) -> Searcher {
    let mut sb = SearcherBuilder::new();
    sb.line_number(true)
        .binary_detection(BinaryDetection::quit(b'\x00'));
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

fn checked_push(
    recs: &Mutex<Vec<FileRec>>,
    rec: FileRec,
    max_collected_files: Option<usize>,
    capped: &AtomicBool,
) -> bool {
    match recs.lock() {
        Ok(mut guard) => {
            if max_collected_files.is_some_and(|max| guard.len() >= max) {
                capped.store(true, Ordering::Relaxed);
                return false;
            }
            guard.push(rec);
            true
        }
        Err(_) => false,
    }
}

fn elapsed_human(elapsed: Duration) -> String {
    format!("{:.6}s", elapsed.as_secs_f64())
}

fn bytes_as_i64(bytes: u64) -> i64 {
    bytes.min(i64::MAX as u64) as i64
}

/// Run a parallel ignore walk + per-file search for a concrete matcher type.
fn collect<M: Matcher + Sync>(
    opts: &RipgrepSearchOptions,
    matcher: &M,
    mode: Mode,
    path_filter: Arc<dyn RipgrepPathFilter>,
) -> Result<CollectResult> {
    let started = Instant::now();
    let only_matching = opts.only_matching.unwrap_or(false);
    // only-matching emits bare spans; ripgrep's `-o` ignores `-C` context too.
    let context_lines = if mode == Mode::Normal && !only_matching {
        opts.context_lines.unwrap_or(0)
    } else {
        0
    };
    let match_window = opts.match_window.unwrap_or(0) as usize;
    let keep_unmatched = mode == Mode::FilesWithoutMatch;

    let recs = Arc::new(Mutex::new(Vec::<FileRec>::new()));
    let files_searched = Arc::new(AtomicU32::new(0));
    let bytes_searched = Arc::new(AtomicU64::new(0));
    let capped = Arc::new(AtomicBool::new(false));
    let span_capped = Arc::new(AtomicBool::new(false));
    let error_count = Arc::new(AtomicU32::new(0));
    let first_error = Arc::new(Mutex::new(None));
    let max_collected_files = opts
        .max_collected_files
        .map(|n| n as usize)
        .filter(|n| *n > 0);

    build_walk_builder(opts)?.build_parallel().run(|| {
        let path_filter = Arc::clone(&path_filter);
        let recs = Arc::clone(&recs);
        let files_searched = Arc::clone(&files_searched);
        let bytes_searched = Arc::clone(&bytes_searched);
        let capped = Arc::clone(&capped);
        let span_capped = Arc::clone(&span_capped);
        let error_count = Arc::clone(&error_count);
        let first_error = Arc::clone(&first_error);
        let mut searcher = build_searcher(opts, context_lines);

        Box::new(move |dent| {
            let dent = match dent {
                Ok(d) => d,
                Err(error) => {
                    record_collection_error(&error_count, &first_error, error.to_string());
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

            let mut entry = FileEntry::new();
            let (submatches, matched_lines, om_matches) = {
                let mut sink = CollectSink {
                    matcher,
                    entry: &mut entry,
                    submatches: 0,
                    matched_lines: 0,
                    work: match_work(mode, only_matching),
                    match_window,
                    om_matches: Vec::new(),
                    span_cap_reached: false,
                };
                // Keep successful files, but report incomplete coverage when
                // traversal or matching fails. A skipped file proves no absence.
                if let Err(error) = searcher.search_path(matcher, path, &mut sink) {
                    record_collection_error(
                        &error_count,
                        &first_error,
                        format!("{}: {error}", path.display()),
                    );
                    return WalkState::Continue;
                }
                if sink.span_cap_reached {
                    span_capped.store(true, Ordering::Relaxed);
                }
                (
                    sink.submatches,
                    sink.matched_lines,
                    std::mem::take(&mut sink.om_matches),
                )
            };
            files_searched.fetch_add(1, Ordering::Relaxed);
            if let Ok(metadata) = dent.metadata() {
                bytes_searched.fetch_add(metadata.len(), Ordering::Relaxed);
            }

            let has_match = matched_lines > 0;
            if has_match == keep_unmatched {
                // Normal/files-only/count modes keep matched files;
                // files-without-match keeps the rest.
                return WalkState::Continue;
            }

            let rec = FileRec {
                path: dent.path().to_string_lossy().into_owned(),
                entry,
                matched_lines,
                submatches,
                om_matches,
                sort_time: capture_sort_time(opts, &dent),
            };

            if checked_push(&recs, rec, max_collected_files, &capped) {
                WalkState::Continue
            } else {
                WalkState::Quit
            }
        })
    });

    let recs = {
        let mut guard = recs.lock().map_err(to_napi_err)?;
        std::mem::take(&mut *guard)
    };

    let mut cap_reasons = Vec::new();
    if capped.load(Ordering::Relaxed) {
        cap_reasons.push("maxCollectedFiles");
    }
    if span_capped.load(Ordering::Relaxed) {
        cap_reasons.push("maxOnlyMatchingPerLine");
    }
    let was_capped = !cap_reasons.is_empty();
    let first_error = first_error.lock().map_err(to_napi_err)?.take();
    Ok(CollectResult {
        recs,
        files_searched: files_searched.load(Ordering::Relaxed),
        bytes_searched: bytes_searched.load(Ordering::Relaxed),
        elapsed: started.elapsed(),
        capped: was_capped,
        cap_reason: was_capped.then(|| cap_reasons.join(", ")),
        error_count: error_count.load(Ordering::Relaxed),
        first_error,
    })
}

fn sort_recs(opts: &RipgrepSearchOptions, recs: &mut [FileRec]) {
    if preserves_traversal_order(opts) {
        return;
    }
    match opts.sort.as_deref() {
        Some("modified") | Some("accessed") | Some("created") => {
            recs.sort_by_key(|r| r.sort_time);
        }
        // Default and explicit "path": lexicographic by full path, matching
        // `rg --sort path`.
        _ => recs.sort_by(|a, b| a.path.cmp(&b.path)),
    }
    if opts.sort_reverse.unwrap_or(false) {
        recs.reverse();
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
        elapsed,
        capped,
        cap_reason,
        error_count,
        first_error,
    } = collected;
    sort_recs(opts, &mut recs);

    let context_lines = opts.context_lines.unwrap_or(0);
    let max_snippet = opts.max_snippet_chars.unwrap_or(DEFAULT_MAX_SNIPPET_CHARS) as usize;

    let files_matched = recs.len() as u32;
    let total_submatches: u32 = recs.iter().map(|r| r.submatches).sum();
    let total_matched_lines: u32 = recs.iter().map(|r| r.matched_lines).sum();

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
            // snippets — exactly what the old plain-text parser produced.
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
        files_searched: Some(files_searched),
        bytes_searched,
        search_time,
        capped: Some(capped),
        cap_reason,
        error_count: Some(error_count),
        first_error,
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
        let collected = collect(&opts, &matcher, mode, path_filter)?;
        Ok(build_result(&opts, mode, collected))
    } else {
        let mut b = RegexMatcherBuilder::new();
        b.case_insensitive(case_insensitive)
            .case_smart(smart_case)
            .word(whole_word)
            .multi_line(multiline)
            .dot_matches_new_line(dotall);
        let pattern = if fixed_string {
            regex::escape(&opts.pattern)
        } else {
            opts.pattern.clone()
        };
        let matcher = b.build(&pattern).map_err(to_napi_err)?;
        let collected = collect(&opts, &matcher, mode, path_filter)?;
        Ok(build_result(&opts, mode, collected))
    }
}

#[cfg(test)]
#[path = "ripgrep_search_tests.rs"]
mod tests;
