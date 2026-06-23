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
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant, SystemTime};

use grep::matcher::Matcher;
use grep::pcre2::RegexMatcherBuilder as Pcre2MatcherBuilder;
use grep::regex::RegexMatcherBuilder;
use grep::searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkContext, SinkMatch};
use ignore::overrides::OverrideBuilder;
use ignore::types::TypesBuilder;
use ignore::{WalkBuilder, WalkState};
use napi::{Error, Result, Status};

use crate::classify;
use crate::ripgrep_parser::{assemble_file, strip_trailing_newline, FileEntry, RawMatch};
use crate::types::{
    RipgrepFile, RipgrepMatch, RipgrepParseResult, RipgrepSearchOptions, RipgrepStats,
};
use crate::utf8_offsets::byte_to_char_offset_inner;

const DEFAULT_MAX_SNIPPET_CHARS: u32 = 500;

/// Cap on emitted spans per line in only-matching mode, so a pathological
/// minified line with a huge number of hits can't blow up the result. The true
/// submatch count is still reported in stats.
const MAX_ONLY_MATCHING_PER_LINE: u32 = 1000;

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
}

/// `grep_searcher::Sink` that accumulates matches/contexts for one file and,
/// using the matcher, derives the 0-based UTF-16 column of the first submatch.
struct CollectSink<'a, M: Matcher> {
    matcher: &'a M,
    entry: &'a mut FileEntry,
    submatches: u32,
    matched_lines: u32,
    /// When set, collect one span per submatch (rg -o) instead of the line.
    only_matching: bool,
    /// Chars of context around each span in only-matching mode.
    match_window: usize,
    /// Accumulated only-matching spans for this file.
    om_matches: Vec<RipgrepMatch>,
}

impl<M: Matcher> Sink for CollectSink<'_, M> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> std::io::Result<bool> {
        let line_number = mat.line_number().unwrap_or(0) as u32;
        let bytes = mat.bytes();
        let line_cow = String::from_utf8_lossy(bytes);
        let line_text = strip_trailing_newline(line_cow.into_owned());

        // Count submatches on this line for --count-matches. A matched line has
        // at least one match even if find_iter is conservative.
        let mut count: u32 = 0;

        if self.only_matching {
            // Emit one span per submatch with its own UTF-16 column, rather than
            // one whole-line match. find_iter yields non-overlapping matches L→R.
            let matcher = self.matcher;
            let window = self.match_window;
            let om = &mut self.om_matches;
            let _ = matcher.find_iter(bytes, |m| {
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
                }
                true
            });
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
        } else {
            // rg reports the submatch start as a BYTE offset within the line;
            // convert to a 0-based UTF-16 char column so multibyte lines line up
            // with JS string indices (mirrors the --json parser's convention).
            let byte_col = self
                .matcher
                .find(bytes)
                .ok()
                .flatten()
                .map(|m| m.start())
                .unwrap_or(0);
            let column = byte_to_char_offset_inner(&line_text, byte_col) as u32;
            let _ = self.matcher.find_iter(bytes, |_m| {
                count = count.saturating_add(1);
                true
            });
            self.entry.raw_matches.push(RawMatch {
                line_text,
                line_number,
                column,
            });
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

fn checked_push(recs: &Mutex<Vec<FileRec>>, rec: FileRec) -> bool {
    match recs.lock() {
        Ok(mut guard) => {
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

    build_walk_builder(opts)?.build_parallel().run(|| {
        let recs = Arc::clone(&recs);
        let files_searched = Arc::clone(&files_searched);
        let bytes_searched = Arc::clone(&bytes_searched);
        let mut searcher = build_searcher(opts, context_lines);

        Box::new(move |dent| {
            let dent = match dent {
                Ok(d) => d,
                Err(_) => return WalkState::Continue,
            };
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
                    only_matching,
                    match_window,
                    om_matches: Vec::new(),
                };
                // Per-file IO errors (permission denied, mid-file invalid UTF-8
                // under PCRE2 utf mode, etc.) just skip that file — rg behaves the
                // same.
                if searcher.search_path(matcher, path, &mut sink).is_err() {
                    return WalkState::Continue;
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

            if checked_push(&recs, rec) {
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

    Ok(CollectResult {
        recs,
        files_searched: files_searched.load(Ordering::Relaxed),
        bytes_searched: bytes_searched.load(Ordering::Relaxed),
        elapsed: started.elapsed(),
    })
}

fn sort_recs(opts: &RipgrepSearchOptions, recs: &mut [FileRec]) {
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

    let stats = match mode {
        Mode::Normal => RipgrepStats {
            match_count: Some(total_submatches),
            matched_lines: Some(total_matched_lines),
            files_matched: Some(files_matched),
            files_searched: Some(files_searched),
            bytes_searched,
            search_time,
        },
        Mode::CountLines => RipgrepStats {
            match_count: Some(total_matched_lines),
            matched_lines: Some(total_matched_lines),
            files_matched: Some(files_matched),
            files_searched: Some(files_searched),
            bytes_searched,
            search_time,
        },
        Mode::CountMatches => RipgrepStats {
            match_count: Some(total_submatches),
            matched_lines: Some(total_matched_lines),
            files_matched: Some(files_matched),
            files_searched: Some(files_searched),
            bytes_searched,
            search_time,
        },
        Mode::FilesOnly | Mode::FilesWithoutMatch => RipgrepStats {
            match_count: Some(total_submatches),
            matched_lines: Some(total_matched_lines),
            files_matched: Some(files_matched),
            files_searched: Some(files_searched),
            bytes_searched,
            search_time,
        },
    };

    RipgrepParseResult { files, stats }
}

/// Build the appropriate matcher (default Rust regex, or PCRE2 for `-P`) and run
/// the search. `fixed_string` is honored by escaping the pattern for the regex
/// engine; the CLI gave `-F` precedence over `-P`, so PCRE2 only applies when
/// `fixed_string` is not set.
pub(crate) fn search(opts: RipgrepSearchOptions) -> Result<RipgrepParseResult> {
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
            .jit_if_available(true);
        let matcher = b.build(&opts.pattern).map_err(to_napi_err)?;
        let collected = collect(&opts, &matcher, mode)?;
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
        let collected = collect(&opts, &matcher, mode)?;
        Ok(build_result(&opts, mode, collected))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Unique temp dir per test (no Date/rand needed): pid + atomic counter.
    struct TmpDir(PathBuf);
    impl TmpDir {
        fn new() -> Self {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            let dir =
                std::env::temp_dir().join(format!("octo-rg-test-{}-{id}", std::process::id()));
            fs::create_dir_all(&dir).expect("create temp dir");
            TmpDir(dir)
        }
        fn write(&self, rel: &str, content: &str) {
            let p = self.0.join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).expect("create parent");
            }
            fs::write(p, content).expect("write file");
        }
        fn path(&self) -> String {
            self.0.to_string_lossy().into_owned()
        }
    }
    impl Drop for TmpDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn opts(path: String, pattern: &str) -> RipgrepSearchOptions {
        RipgrepSearchOptions {
            path,
            pattern: pattern.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn finds_matches_with_line_and_column() {
        let t = TmpDir::new();
        t.write("a.txt", "hello world\nno match here\nhello again\n");
        let r = search(opts(t.path(), "hello")).expect("search ok");
        assert_eq!(r.files.len(), 1);
        let f = &r.files[0];
        assert_eq!(f.match_count, 2);
        assert_eq!(f.matches[0].line, 1);
        assert_eq!(f.matches[0].column, 0);
        assert_eq!(f.matches[0].value, "hello world");
        assert_eq!(f.matches[1].line, 3);
        assert_eq!(r.stats.match_count, Some(2));
        assert_eq!(r.stats.files_matched, Some(1));
        assert!(r.stats.bytes_searched.unwrap_or_default() > 0);
        assert!(r
            .stats
            .search_time
            .as_deref()
            .is_some_and(|s| s.ends_with('s')));
    }

    #[test]
    fn column_is_utf16_offset_on_multibyte_line() {
        let t = TmpDir::new();
        // 'b' is at byte 8 but UTF-16 index 7 ('é' is 2 bytes).
        t.write("u.txt", "café = bar\n");
        let r = search(opts(t.path(), "bar")).expect("ok");
        assert_eq!(r.files[0].matches[0].column, 7);
    }

    #[test]
    fn smart_case_default_is_case_insensitive_for_lowercase() {
        let t = TmpDir::new();
        t.write("a.txt", "Hello\nhello\n");
        let r = search(opts(t.path(), "hello")).expect("ok");
        assert_eq!(r.files[0].match_count, 2);
    }

    #[test]
    fn case_sensitive_flag_is_exact() {
        let t = TmpDir::new();
        t.write("a.txt", "Hello\nhello\n");
        let mut o = opts(t.path(), "hello");
        o.case_sensitive = Some(true);
        let r = search(o).expect("ok");
        assert_eq!(r.files[0].match_count, 1);
    }

    #[test]
    fn fixed_string_treats_pattern_literally() {
        let t = TmpDir::new();
        t.write("a.txt", "a.b\naxb\n");
        let mut o = opts(t.path(), "a.b");
        o.fixed_string = Some(true);
        let r = search(o).expect("ok");
        // Literal "a.b" matches only line 1, not "axb".
        assert_eq!(r.files[0].match_count, 1);
        assert_eq!(r.files[0].matches[0].value, "a.b");
    }

    #[test]
    fn perl_regex_supports_lookahead() {
        let t = TmpDir::new();
        t.write("a.txt", "foobar\nfoobaz\n");
        let mut o = opts(t.path(), "foo(?=bar)");
        o.perl_regex = Some(true);
        let r = search(o).expect("ok");
        assert_eq!(r.files[0].match_count, 1);
        assert_eq!(r.files[0].matches[0].line, 1);
    }

    #[test]
    fn files_only_lists_paths_without_snippets() {
        let t = TmpDir::new();
        t.write("a.txt", "needle\n");
        t.write("b.txt", "haystack\n");
        let mut o = opts(t.path(), "needle");
        o.files_only = Some(true);
        let r = search(o).expect("ok");
        assert_eq!(r.files.len(), 1);
        assert_eq!(r.files[0].match_count, 1);
        assert!(r.files[0].matches.is_empty());
        assert!(r.files[0].path.ends_with("a.txt"));
        assert_eq!(r.stats.files_matched, Some(1));
        assert!(r.stats.bytes_searched.unwrap_or_default() > 0);
    }

    #[test]
    fn files_without_match_inverts_file_set() {
        let t = TmpDir::new();
        t.write("a.txt", "needle\n");
        t.write("b.txt", "haystack\n");
        let mut o = opts(t.path(), "needle");
        o.files_without_match = Some(true);
        let r = search(o).expect("ok");
        assert_eq!(r.files.len(), 1);
        assert!(r.files[0].path.ends_with("b.txt"));
    }

    #[test]
    fn count_matches_counts_submatches_per_file() {
        let t = TmpDir::new();
        t.write("a.txt", "x x x\nx\n");
        let mut o = opts(t.path(), "x");
        o.count_matches_per_file = Some(true);
        let r = search(o).expect("ok");
        assert_eq!(r.files[0].match_count, 4);
        assert_eq!(r.stats.match_count, Some(4));
    }

    #[test]
    fn count_lines_counts_matched_lines_per_file() {
        let t = TmpDir::new();
        t.write("a.txt", "x x x\nx\nnope\n");
        let mut o = opts(t.path(), "x");
        o.count_lines_per_file = Some(true);
        let r = search(o).expect("ok");
        assert_eq!(r.files[0].match_count, 2);
    }

    #[test]
    fn context_lines_are_assembled_into_snippet() {
        let t = TmpDir::new();
        t.write("a.txt", "before\nmatch\nafter\n");
        let mut o = opts(t.path(), "match");
        o.context_lines = Some(1);
        let r = search(o).expect("ok");
        let v = &r.files[0].matches[0].value;
        assert!(
            v.contains("before") && v.contains("match") && v.contains("after"),
            "{v}"
        );
    }

    #[test]
    fn lang_type_filters_by_extension() {
        let t = TmpDir::new();
        t.write("a.ts", "target\n");
        t.write("b.py", "target\n");
        let mut o = opts(t.path(), "target");
        o.lang_type = Some("ts".to_owned());
        let r = search(o).expect("ok");
        assert_eq!(r.files.len(), 1);
        assert!(r.files[0].path.ends_with("a.ts"));
    }

    #[test]
    fn include_glob_restricts_files() {
        let t = TmpDir::new();
        t.write("a.ts", "target\n");
        t.write("b.js", "target\n");
        let mut o = opts(t.path(), "target");
        o.include = Some(vec!["*.ts".to_owned()]);
        let r = search(o).expect("ok");
        assert_eq!(r.files.len(), 1);
        assert!(r.files[0].path.ends_with("a.ts"));
    }

    #[test]
    fn exclude_dir_prunes_directory() {
        let t = TmpDir::new();
        t.write("keep/a.txt", "target\n");
        t.write("skip/b.txt", "target\n");
        let mut o = opts(t.path(), "target");
        o.exclude_dir = Some(vec!["skip".to_owned()]);
        let r = search(o).expect("ok");
        assert_eq!(r.files.len(), 1);
        assert!(r.files[0].path.contains("keep"));
    }

    #[test]
    fn results_are_sorted_by_path() {
        let t = TmpDir::new();
        t.write("z.txt", "m\n");
        t.write("a.txt", "m\n");
        t.write("m.txt", "m\n");
        let r = search(opts(t.path(), "m")).expect("ok");
        let paths: Vec<&str> = r.files.iter().map(|f| f.path.as_str()).collect();
        let mut sorted = paths.clone();
        sorted.sort_unstable();
        assert_eq!(paths, sorted);
    }

    #[test]
    fn sort_reverse_flips_order() {
        let t = TmpDir::new();
        t.write("a.txt", "m\n");
        t.write("z.txt", "m\n");
        let mut o = opts(t.path(), "m");
        o.sort_reverse = Some(true);
        let r = search(o).expect("ok");
        assert!(r.files[0].path.ends_with("z.txt"));
        assert!(r.files[1].path.ends_with("a.txt"));
    }

    #[test]
    fn respects_gitignore_by_default_and_no_ignore_overrides() {
        let t = TmpDir::new();
        // The `ignore` crate (like rg) only applies .gitignore inside a git repo
        // (require_git defaults true); a bare `.git` dir marks the temp tree as one.
        fs::create_dir_all(t.0.join(".git")).expect("create .git");
        t.write(".gitignore", "ignored.txt\n");
        t.write("ignored.txt", "target\n");
        t.write("kept.txt", "target\n");

        let r = search(opts(t.path(), "target")).expect("ok");
        assert_eq!(r.files.len(), 1, "gitignore'd file should be skipped");
        assert!(r.files[0].path.ends_with("kept.txt"));

        let mut o = opts(t.path(), "target");
        o.no_ignore = Some(true);
        let r2 = search(o).expect("ok");
        assert_eq!(r2.files.len(), 2, "--no-ignore searches the ignored file");
    }

    #[test]
    fn whole_word_requires_word_boundary() {
        let t = TmpDir::new();
        t.write("a.txt", "foo\nfoobar\n");
        let mut o = opts(t.path(), "foo");
        o.whole_word = Some(true);
        let r = search(o).expect("ok");
        assert_eq!(r.files[0].match_count, 1);
        assert_eq!(r.files[0].matches[0].line, 1);
    }

    #[test]
    fn empty_result_when_no_match() {
        let t = TmpDir::new();
        t.write("a.txt", "nothing here\n");
        let r = search(opts(t.path(), "absent")).expect("ok");
        assert!(r.files.is_empty());
    }

    // ── only-matching (rg -o) ───────────────────────────────────────────────

    #[test]
    fn only_matching_emits_one_match_per_submatch() {
        let t = TmpDir::new();
        t.write("a.txt", "ab ab ab\n");
        let mut o = opts(t.path(), "ab");
        o.only_matching = Some(true);
        let r = search(o).expect("ok");
        let f = &r.files[0];
        assert_eq!(f.match_count, 3);
        assert_eq!(f.matches.len(), 3);
        assert!(f.matches.iter().all(|m| m.value == "ab"));
        assert!(f.matches.iter().all(|m| m.line == 1));
    }

    #[test]
    fn only_matching_value_is_the_span_not_the_line() {
        let t = TmpDir::new();
        t.write("a.txt", "prefix_NEEDLE_suffix\n");
        let mut o = opts(t.path(), "NEEDLE");
        o.only_matching = Some(true);
        let r = search(o).expect("ok");
        assert_eq!(r.files[0].matches.len(), 1);
        assert_eq!(r.files[0].matches[0].value, "NEEDLE");
        // column is the 0-based UTF-16 offset of the span start.
        assert_eq!(r.files[0].matches[0].column, 7);
    }

    #[test]
    fn only_matching_enumerates_every_hit_on_one_minified_line() {
        let t = TmpDir::new();
        // The motivating case: a minified one-liner with many host tokens that
        // line-mode search can only *count*, never enumerate.
        t.write(
            "bundle.js",
            "a=\"x.cursor.sh\";b=\"y.cursor.sh\";c=\"z.cursor.sh\";\n",
        );
        let mut o = opts(t.path(), r"\w+\.cursor\.sh");
        o.only_matching = Some(true);
        let r = search(o).expect("ok");
        let vals: Vec<&str> = r.files[0]
            .matches
            .iter()
            .map(|m| m.value.as_str())
            .collect();
        assert_eq!(vals, vec!["x.cursor.sh", "y.cursor.sh", "z.cursor.sh"]);
    }

    #[test]
    fn only_matching_window_widens_span_with_surrounding_context() {
        let t = TmpDir::new();
        t.write("a.txt", "leftcontext_HIT_rightcontext\n");
        let mut o = opts(t.path(), "HIT");
        o.only_matching = Some(true);
        o.match_window = Some(4);
        let r = search(o).expect("ok");
        let v = &r.files[0].matches[0].value;
        assert!(v.contains("HIT"), "{v}");
        assert!(v.contains("ext_") && v.contains("_rig"), "{v}");
        // window trims both sides, so ellipsis markers are present.
        assert!(v.starts_with('…') && v.ends_with('…'), "{v}");
    }

    #[test]
    fn only_matching_window_is_char_boundary_safe_on_multibyte() {
        let t = TmpDir::new();
        // Multibyte chars on both sides of the hit: window slicing must never
        // panic by cutting a codepoint in half.
        t.write("u.txt", "café→HIT←déjà\n");
        let mut o = opts(t.path(), "HIT");
        o.only_matching = Some(true);
        o.match_window = Some(2);
        let r = search(o).expect("ok");
        assert!(r.files[0].matches[0].value.contains("HIT"));
    }

    #[test]
    fn only_matching_unique_keeps_distinct_values_in_first_occurrence_order() {
        let t = TmpDir::new();
        t.write("a.txt", "ab ab cd ab cd ef\n");
        let mut o = opts(t.path(), r"\w+");
        o.only_matching = Some(true);
        o.unique = Some(true);
        let r = search(o).expect("ok");
        let vals: Vec<&str> = r.files[0]
            .matches
            .iter()
            .map(|m| m.value.as_str())
            .collect();
        assert_eq!(vals, vec!["ab", "cd", "ef"]);
        assert!(r.files[0].matches.iter().all(|m| m.count.is_none()));
    }

    #[test]
    fn only_matching_count_unique_attaches_frequency_sorted_descending() {
        let t = TmpDir::new();
        t.write("a.txt", "ab ab cd ab cd ef\n");
        let mut o = opts(t.path(), r"\w+");
        o.only_matching = Some(true);
        o.count_unique = Some(true);
        let r = search(o).expect("ok");
        let vals: Vec<(&str, Option<u32>)> = r.files[0]
            .matches
            .iter()
            .map(|m| (m.value.as_str(), m.count))
            .collect();
        assert_eq!(
            vals,
            vec![("ab", Some(3)), ("cd", Some(2)), ("ef", Some(1))]
        );
    }

    #[test]
    fn unique_requires_only_matching() {
        let t = TmpDir::new();
        t.write("a.txt", "ab ab\n");
        let mut o = opts(t.path(), "ab");
        o.unique = Some(true);
        let err = search(o).expect_err("unique without onlyMatching is invalid");
        assert!(err.reason.contains("onlyMatching:true"));
    }

    #[test]
    fn only_matching_default_off_keeps_whole_line_value() {
        let t = TmpDir::new();
        t.write("a.txt", "prefix_NEEDLE_suffix\n");
        let r = search(opts(t.path(), "NEEDLE")).expect("ok");
        // Without only_matching the value is the full line, unchanged.
        assert_eq!(r.files[0].matches[0].value, "prefix_NEEDLE_suffix");
    }
}
