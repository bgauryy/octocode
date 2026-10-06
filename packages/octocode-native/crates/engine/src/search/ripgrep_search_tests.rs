use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct AllowAll;
impl RipgrepPathFilter for AllowAll {
    fn allows(&self, _: &Path, _: bool) -> bool {
        true
    }
}

pub(crate) fn search(opts: RipgrepSearchOptions) -> Result<RipgrepParseResult> {
    search_cancellable(opts, Arc::new(AllowAll), &|| false)
}

/// Unique temp dir per test (no Date/rand needed): pid + atomic counter.
struct TmpDir(PathBuf);
impl TmpDir {
    fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("octo-rg-test-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");
        Self(dir)
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
fn span_collection_reports_dropped_matches_as_incomplete() {
    let t = TmpDir::new();
    t.write("many.txt", &"needle ".repeat(1001));
    for count_unique in [false, true] {
        let mut options = opts(t.path(), "needle");
        options.only_matching = Some(true);
        options.count_unique = Some(count_unique);
        let result = search(options).expect("search ok");
        assert_eq!(result.stats.match_count, Some(1001));
        assert_eq!(result.stats.capped, Some(true));
        assert_eq!(
            result.stats.cap_reason.as_deref(),
            Some("maxOnlyMatchingPerLine")
        );
        if count_unique {
            assert_eq!(result.files[0].matches[0].count, Some(1000));
        } else {
            assert_eq!(result.files[0].matches.len(), 1000);
        }
    }
}

#[test]
fn span_collection_at_the_bound_remains_complete() {
    let t = TmpDir::new();
    t.write("many.txt", &"needle ".repeat(1000));
    let mut options = opts(t.path(), "needle");
    options.only_matching = Some(true);
    let result = search(options).expect("search ok");
    assert_eq!(result.files[0].matches.len(), 1000);
    assert_eq!(result.stats.capped, Some(false));
    assert_eq!(result.stats.cap_reason, None);
}

#[test]
fn collection_errors_do_not_report_complete_absence() {
    let t = TmpDir::new();
    let missing = t.0.join("missing").to_string_lossy().into_owned();
    let result = search(opts(missing, "needle")).expect("partial search");
    assert!(result.files.is_empty());
    assert_eq!(result.stats.error_count, Some(1));
    assert!(
        result
            .stats
            .first_error
            .as_ref()
            .is_some_and(|e| !e.is_empty())
    );
    assert_eq!(result.stats.capped, Some(false));
}

#[cfg(unix)]
#[test]
fn collection_errors_preserve_successful_files() {
    use std::os::unix::fs::PermissionsExt;
    let t = TmpDir::new();
    t.write("good.txt", "needle\n");
    t.write("denied.txt", "needle\n");
    let denied = t.0.join("denied.txt");
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o000)).unwrap();
    // Privileged test runners can read mode-000 files; the missing-root test
    // remains deterministic on those hosts.
    let inaccessible = fs::read(&denied).is_err();
    let result = search(opts(t.path(), "needle"));
    fs::set_permissions(&denied, fs::Permissions::from_mode(0o600)).unwrap();
    if !inaccessible {
        return;
    }
    let result = result.expect("partial search");
    assert_eq!(result.files.len(), 1);
    assert!(result.files[0].path.ends_with("good.txt"));
    assert_eq!(result.stats.error_count, Some(1));
    assert!(
        result
            .stats
            .first_error
            .as_ref()
            .is_some_and(|e| e.contains("denied.txt"))
    );
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
    assert!(
        r.stats
            .search_time
            .as_deref()
            .is_some_and(|s| s.ends_with('s'))
    );
}

#[test]
fn max_depth_prunes_the_native_walk_and_statistics() {
    let t = TmpDir::new();
    t.write("root.ts", "needle\n");
    t.write("one/nested.ts", "needle\n");
    t.write("one/two/deep.ts", "needle\n");

    let mut options = opts(t.path(), "needle");
    options.max_depth = Some(0);
    let result = search(options).expect("search ok");

    assert_eq!(result.files.len(), 1);
    assert!(result.files[0].path.ends_with("root.ts"));
    assert_eq!(result.stats.files_searched, Some(1));
    assert_eq!(result.stats.files_matched, Some(1));
    assert_eq!(result.stats.match_count, Some(1));
    assert_eq!(result.stats.matched_lines, Some(1));
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
fn perl_regex_catastrophic_backtracking_terminates() {
    // `(a+)+$` against a long line of 'a' followed by a non-matching char is
    // the classic exponential-backtracking blowup. The JIT-stack cap makes
    // PCRE2 fail fast (or complete) rather than spinning; either way the call
    // must return. Run on a worker thread so a regression surfaces as a
    // timeout instead of hanging the whole suite.
    use std::sync::mpsc;
    use std::time::Duration;

    let t = TmpDir::new();
    t.write("a.txt", &format!("{}!\n", "a".repeat(5_000)));
    let path = t.path();
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let mut o = opts(path, "(a+)+$");
        o.perl_regex = Some(true);
        // Result intentionally ignored: PCRE2 may return Ok (no match) or an
        // Err (JIT stack / match limit hit) — the assertion is termination.
        let _ = search(o);
        let _ = tx.send(());
    });
    assert!(
        rx.recv_timeout(Duration::from_secs(30)).is_ok(),
        "catastrophic PCRE2 pattern must terminate, not hang"
    );
    handle.join().expect("worker thread panicked");
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

/// `invertMatch` selects non-matching *lines* (rg -v), so the path views
/// follow rg: `files` lists files with at least one non-matching line, and
/// `filesWithout` lists files whose every line matches. Files that merely
/// lack the pattern are plain `filesWithout`, never `invertMatch`+`files`.
#[test]
fn invert_match_path_views_follow_rg_line_semantics() {
    let t = TmpDir::new();
    t.write("all.txt", "needle\nneedle\n");
    t.write("mixed.txt", "needle\nhay\n");
    t.write("none.txt", "hay\n");
    let listed = |files_only: bool| {
        let mut o = opts(t.path(), "needle");
        o.invert_match = Some(true);
        o.files_only = Some(files_only);
        o.files_without_match = Some(!files_only);
        let mut names = search(o)
            .expect("ok")
            .files
            .into_iter()
            .map(|file| {
                Path::new(&file.path)
                    .file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    };
    assert_eq!(listed(true), ["mixed.txt", "none.txt"]);
    assert_eq!(listed(false), ["all.txt"]);
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
fn match_work_avoids_discarded_snippets_and_redundant_scans() {
    let count_lines = match_work(Mode::CountLines, false);
    assert!(!count_lines.materialize_line);
    assert!(!count_lines.enumerate_submatches);

    for mode in [Mode::FilesOnly, Mode::FilesWithoutMatch, Mode::CountMatches] {
        let work = match_work(mode, false);
        assert!(!work.materialize_line);
        assert!(work.enumerate_submatches);
        assert!(!work.collect_spans);
    }

    let normal = match_work(Mode::Normal, false);
    assert!(normal.materialize_line);
    assert!(normal.enumerate_submatches);
    assert!(!normal.collect_spans);

    let only_matching = match_work(Mode::Normal, true);
    assert!(only_matching.materialize_line);
    assert!(only_matching.enumerate_submatches);
    assert!(only_matching.collect_spans);
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
    // A pruned directory is disclosed, root-relative: absence there is unproven.
    assert_eq!(r.stats.pruned_dirs, Some(vec!["skip".to_owned()]));
}

#[test]
fn the_search_root_itself_is_never_pruned() {
    let t = TmpDir::new();
    t.write("build/a.txt", "target\n");
    let mut o = opts(format!("{}/build", t.path()), "target");
    o.exclude_dir = Some(vec!["build".to_owned()]);
    let r = search(o).expect("ok");
    assert_eq!(r.files.len(), 1);
    assert_eq!(r.stats.pruned_dirs, None);
}

#[test]
fn caps_collected_files_and_reports_non_exhaustive_stats() {
    let t = TmpDir::new();
    t.write("a.txt", "needle\n");
    t.write("b.txt", "needle\n");
    t.write("c.txt", "needle\n");
    let mut o = opts(t.path(), "needle");
    o.max_collected_files = Some(2);

    let r = search(o).expect("ok");

    assert_eq!(r.files.len(), 2);
    assert_eq!(r.stats.capped, Some(true));
    assert_eq!(r.stats.cap_reason.as_deref(), Some("maxCollectedFiles"));
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
fn sort_then_cap_retains_deterministic_sorted_prefix() {
    // The collection cap must be a STABLE truncation of the sorted prefix,
    // not a race-dependent subset chosen during the parallel walk.
    // Feeding an unsorted record set exceeding a small cap must always retain
    // the same sorted prefix, identical across repeated runs.
    fn rec(path: &str) -> FileRec {
        FileRec {
            path: path.to_owned(),
            entry: FileEntry::new(),
            matched_lines: 1,
            submatches: 1,
            om_matches: Vec::new(),
            sort_time: None,
            line_weight: 0,
            demoted: false,
            generated: false,
            declares: false,
        }
    }
    let make = || {
        vec![
            rec("m.txt"),
            rec("a.txt"),
            rec("z.txt"),
            rec("b.txt"),
            rec("c.txt"),
        ]
    };
    let mut o = opts("/fixture".to_owned(), "p");
    o.max_collected_files = Some(3);
    let run = || {
        let mut recs = make();
        let capped = sort_and_cap(&o, Mode::Normal, &mut recs);
        (
            recs.iter().map(|r| r.path.clone()).collect::<Vec<_>>(),
            capped,
        )
    };
    let (first, capped_first) = run();
    assert_eq!(first, vec!["a.txt", "b.txt", "c.txt"]);
    assert!(capped_first);
    for _ in 0..10 {
        let (again, capped_again) = run();
        assert_eq!(again, first, "sorted-prefix truncation must be stable");
        assert!(capped_again);
    }

    // Under the cap: no truncation, all records kept in sorted order.
    let mut under = vec![rec("b.txt"), rec("a.txt")];
    let mut o2 = opts("/fixture".to_owned(), "p");
    o2.max_collected_files = Some(3);
    assert!(!sort_and_cap(&o2, Mode::Normal, &mut under));
    assert_eq!(
        under.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
        vec!["a.txt", "b.txt"]
    );
}

#[test]
fn oversize_file_is_skipped_with_diagnostic_and_normal_file_still_matches() {
    // A file above the byte ceiling is skipped before it is searched and
    // surfaced as a `maxFileSize` diagnostic; a normal file still matches.
    let t = TmpDir::new();
    t.write("small.txt", "needle\n"); // 7 bytes, under the ceiling
    t.write("big.txt", &"needle ".repeat(50)); // 350 bytes, over the ceiling
    let mut o = opts(t.path(), "needle");
    o.max_file_bytes = Some(10);
    let r = search(o).expect("ok");
    assert_eq!(r.files.len(), 1);
    assert!(r.files[0].path.ends_with("small.txt"));
    assert_eq!(r.stats.capped, Some(true));
    assert!(
        r.stats
            .cap_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("maxFileSize"))
    );
    // The skipped file is not counted as covered (no overstated filesSearched).
    assert_eq!(r.stats.files_searched, Some(1));
}

#[test]
fn binary_quit_file_is_flagged_not_silently_absent() {
    // A file quit as binary (NUL byte) is reflected in a diagnostic, and
    // an ordinary text file still matches.
    let t = TmpDir::new();
    t.write("data.bin", "needle before\u{0}needle after\n");
    t.write("text.txt", "needle plain\n");
    let r = search(opts(t.path(), "needle")).expect("ok");
    assert!(r.files.iter().any(|f| f.path.ends_with("text.txt")));
    assert_eq!(r.stats.capped, Some(true));
    assert!(
        r.stats
            .cap_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("binaryQuit"))
    );
}

#[test]
fn pcre2_worker_slots_are_bounded_and_released() {
    // The worker-slot bound rejects new acquisitions once saturated and
    // frees a slot on release — tested directly on the counter logic.
    use std::sync::atomic::AtomicUsize;
    let counter = AtomicUsize::new(0);
    let max = 3;
    assert!(try_acquire_worker_slot(&counter, max));
    assert!(try_acquire_worker_slot(&counter, max));
    assert!(try_acquire_worker_slot(&counter, max));
    // Saturated: the 4th acquisition is rejected and the counter is unchanged.
    assert!(!try_acquire_worker_slot(&counter, max));
    assert_eq!(counter.load(Ordering::Relaxed), 3);
    // Releasing frees exactly one slot.
    release_worker_slot(&counter);
    assert_eq!(counter.load(Ordering::Relaxed), 2);
    assert!(try_acquire_worker_slot(&counter, max));
    assert!(!try_acquire_worker_slot(&counter, max));
}

#[test]
fn traversal_sort_output_is_stable_across_runs() {
    // With sort:"traversal" the walk is forced single-threaded so the
    // emitted order is stable run-to-run on the same tree.
    let t = TmpDir::new();
    for i in 0..40 {
        t.write(&format!("dir{}/file{i:02}.txt", i % 5), "needle\n");
    }
    let run = || {
        let mut o = opts(t.path(), "needle");
        o.sort = Some("traversal".to_owned());
        search(o)
            .expect("ok")
            .files
            .into_iter()
            .map(|f| f.path)
            .collect::<Vec<_>>()
    };
    let first = run();
    assert_eq!(first.len(), 40);
    for _ in 0..5 {
        assert_eq!(run(), first, "traversal order must be stable across runs");
    }
}

#[test]
fn explicit_traversal_sort_bypasses_post_collection_sorting() {
    let mut o = opts("/fixture".to_owned(), "m");
    o.sort = Some("traversal".to_owned());
    assert!(preserves_traversal_order(&o));
    o.sort = None;
    assert!(!preserves_traversal_order(&o));
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

#[test]
fn files_without_match_omits_binary_files_quit_before_matching() {
    let t = TmpDir::new();
    t.write("data.bin", "\u{0}needle\n");
    t.write("plain.txt", "nothing here\n");
    t.write("hit.txt", "needle\n");
    let mut o = opts(t.path(), "needle");
    o.files_without_match = Some(true);
    let r = search(o).expect("ok");
    let paths: Vec<_> = r.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths.len(), 1, "{paths:?}");
    assert!(paths[0].ends_with("plain.txt"));
}

#[test]
fn non_utf8_line_spans_and_columns_map_through_lossy_decoding() {
    let t = TmpDir::new();
    fs::write(t.0.join("latin.txt"), b"caf\xe9\xe9\xe9 foo bar\n").expect("write");
    let mut o = opts(t.path(), "foo");
    o.only_matching = Some(true);
    let r = search(o).expect("ok");
    let m = &r.files[0].matches[0];
    assert_eq!(m.value, "foo");
    assert_eq!(m.column, 7);
    let r = search(opts(t.path(), "bar")).expect("ok");
    assert_eq!(r.files[0].matches[0].column, 11);
}

#[test]
fn context_snippet_is_contiguous_and_includes_neighbouring_match_lines() {
    let t = TmpDir::new();
    t.write("a.txt", "alpha foo\nFoo bar\nfoo foo foo\nbaz\n");
    let mut o = opts(t.path(), "foo");
    o.context_lines = Some(1);
    o.case_sensitive = Some(false);
    let r = search(o).expect("ok");
    let m = &r.files[0].matches;
    assert_eq!(m[0].value, "alpha foo\nFoo bar");
    assert_eq!(m[2].value, "Foo bar\nfoo foo foo\nbaz");
}

#[test]
fn long_line_snippet_keeps_the_match_visible() {
    let t = TmpDir::new();
    t.write("long.txt", &format!("{}foo tail\n", "x".repeat(5000)));
    let r = search(opts(t.path(), "foo")).expect("ok");
    let m = &r.files[0].matches[0];
    assert!(m.value.contains("foo"), "{}", &m.value[..60]);
    assert!(m.value.starts_with('…'));
    assert!(m.value.chars().count() <= 500);
    assert!(m.original_chars.is_some());
}

#[test]
fn long_leading_context_does_not_hide_the_match() {
    let t = TmpDir::new();
    t.write("ctx.txt", &format!("{}\ntarget here\n", "C".repeat(600)));
    let mut o = opts(t.path(), "target");
    o.context_lines = Some(1);
    let r = search(o).expect("ok");
    assert!(r.files[0].matches[0].value.contains("target"));
}

/// A file whose first NUL comes before any text (image, object file, archive
/// headers) is opaque binary: skipped like rg skips it, counted, and not a
/// coverage gap. Only a NUL after text leaves searchable text unread.
#[test]
fn opaque_binary_files_are_skipped_not_a_coverage_gap() {
    let t = TmpDir::new();
    fs::write(
        t.0.join("logo.png"),
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR needle",
    )
    .expect("png");
    fs::write(t.0.join("addon.node"), b"\x7fELF\x02\x01\x01\0needle").expect("elf");
    fs::write(t.0.join("font.ttf"), b"\0\x01\0\0needle").expect("ttf");
    t.write("text.txt", "needle plain\n");
    let r = search(opts(t.path(), "needle")).expect("ok");
    assert_eq!(r.files.len(), 1, "{:?}", r.files);
    assert!(r.files[0].path.ends_with("text.txt"));
    assert_eq!(r.stats.cap_reason, None, "{:?}", r.stats);
    assert_ne!(r.stats.capped, Some(true), "{:?}", r.stats);
    assert_eq!(r.stats.binary_files, None, "{:?}", r.stats);
    assert_eq!(r.stats.skipped_binary_count, Some(3), "{:?}", r.stats);
    assert_eq!(
        extension_groups(&r.stats),
        [("node", 1), ("png", 1), ("ttf", 1)],
        "{:?}",
        r.stats
    );
    assert_eq!(r.stats.files_searched, Some(4), "{:?}", r.stats);

    // files-without-match still never lists a file it could not read as text.
    let mut o = opts(t.path(), "needle");
    o.files_without_match = Some(true);
    assert!(search(o).expect("ok").files.is_empty());
}

/// Bytes of a binary header that happen to match are not text hits: the
/// file is skipped and counted, never returned with rows a text read fails on.
#[test]
fn a_match_inside_an_opaque_binary_header_is_not_a_hit() {
    let t = TmpDir::new();
    fs::write(t.0.join("logo.png"), b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").expect("png");
    let r = search(opts(t.path(), "PNG")).expect("ok");
    assert!(r.files.is_empty(), "{:?}", r.files);
    assert_eq!(r.stats.skipped_binary_count, Some(1), "{:?}", r.stats);
    assert_eq!(r.stats.files_matched, Some(0), "{:?}", r.stats);
}

#[test]
fn binary_quit_keeps_matches_before_the_nul() {
    let t = TmpDir::new();
    fs::write(
        t.0.join("mixed.txt"),
        b"alpha before\0alpha after\nalpha before\n",
    )
    .expect("fixture");
    let r = search(opts(t.path(), "alpha")).expect("ok");
    assert_eq!(r.files.len(), 1, "{:?}", r.stats);
    let matches = &r.files[0].matches;
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].line, 1);
    assert_eq!(matches[0].value, "alpha before");
    assert_eq!(r.stats.match_count, Some(1));
    assert_eq!(r.stats.cap_reason.as_deref(), Some("binaryQuit"));
}

fn extension_groups(stats: &RipgrepStats) -> Vec<(&str, u32)> {
    stats
        .skipped_binary_extensions
        .iter()
        .flatten()
        .map(|group| (group.extension.as_str(), group.count))
        .collect()
}

/// A format whose magic is printable (woff2, OpenType, SQLite) still puts
/// its NUL inside the leading bytes, before any complete line: it is binary
/// like an image, not text cut short. Skipped, counted by extension.
#[test]
fn a_printable_magic_before_a_leading_nul_is_opaque_binary() {
    let t = TmpDir::new();
    fs::write(t.0.join("a.woff2"), b"wOF2\0\x01\0\0needle").expect("woff2");
    fs::write(t.0.join("b.woff2"), b"wOF2\0\x01\0\0needle").expect("woff2");
    fs::write(t.0.join("c.OTF"), b"OTTO\0\x0b\0\x80needle").expect("otf");
    fs::write(t.0.join("store"), b"SQLite format 3\0needle").expect("sqlite");
    t.write("text.txt", "needle plain\n");
    let r = search(opts(t.path(), "needle")).expect("ok");
    assert_eq!(r.files.len(), 1, "{:?}", r.files);
    assert_eq!(r.stats.cap_reason, None, "{:?}", r.stats);
    assert_eq!(r.stats.binary_files, None, "{:?}", r.stats);
    assert_eq!(r.stats.skipped_binary_count, Some(4), "{:?}", r.stats);
    // Most files first; the extension is lowercased like structureSearch
    // matches it, and an extensionless file groups under "".
    assert_eq!(
        extension_groups(&r.stats),
        [("woff2", 2), ("", 1), ("otf", 1)],
        "{:?}",
        r.stats
    );
    // Only the extensionless group names its files.
    let names = |extension: &str| {
        r.stats
            .skipped_binary_extensions
            .iter()
            .flatten()
            .find(|group| group.extension == extension)
            .map(|group| group.names.clone())
    };
    assert_eq!(names(""), Some(vec!["store".to_owned()]));
    assert_eq!(names("woff2"), Some(Vec::new()));
}

/// Text lines before a NUL are searchable text the cut leaves unread: a
/// coverage gap even when nothing before the NUL matched.
#[test]
fn text_lines_before_a_nul_stay_a_coverage_gap() {
    let t = TmpDir::new();
    fs::write(t.0.join("log.txt"), b"first line\nsecond line\n\0needle\n").expect("log");
    let r = search(opts(t.path(), "needle")).expect("ok");
    assert!(r.files.is_empty(), "{:?}", r.files);
    assert_eq!(r.stats.cap_reason.as_deref(), Some("binaryQuit"));
    assert_eq!(r.stats.skipped_binary_count, None, "{:?}", r.stats);
    assert_eq!(r.stats.binary_file_count, Some(1));
    let named = r.stats.binary_files.unwrap_or_default();
    assert!(
        named.len() == 1 && named[0].ends_with("log.txt"),
        "{named:?}"
    );
}

/// A match before the NUL is evidence from a file the search could not
/// finish, so even a short single-line prefix keeps the file a named gap.
#[test]
fn a_match_before_a_leading_nul_keeps_the_file_a_gap() {
    let t = TmpDir::new();
    fs::write(t.0.join("a.woff2"), b"wOF2\0\x01\0\0").expect("woff2");
    let r = search(opts(t.path(), "wOF2")).expect("ok");
    assert_eq!(r.files.len(), 1, "{:?}", r.files);
    assert_eq!(r.stats.cap_reason.as_deref(), Some("binaryQuit"));
    assert_eq!(r.stats.skipped_binary_count, None, "{:?}", r.stats);
}

/// A long run of text before a NUL is not a format header, line break or
/// not: it stays a coverage gap.
#[test]
fn a_long_text_run_before_a_nul_stays_a_gap() {
    let t = TmpDir::new();
    let mut body = "x".repeat(4096).into_bytes();
    body.extend_from_slice(b"\0needle\n");
    fs::write(t.0.join("bundle.js"), body).expect("bundle");
    let r = search(opts(t.path(), "needle")).expect("ok");
    assert_eq!(r.stats.cap_reason.as_deref(), Some("binaryQuit"));
    assert_eq!(r.stats.skipped_binary_count, None, "{:?}", r.stats);
}

#[test]
fn binary_prefix_counts_matches_across_earlier_lines() {
    let t = TmpDir::new();
    fs::write(
        t.0.join("mixed.txt"),
        b"alpha one\nbeta\nalpha two\0alpha three\n",
    )
    .expect("fixture");
    let r = search(opts(t.path(), "alpha")).expect("ok");
    let lines: Vec<u32> = r.files[0].matches.iter().map(|m| m.line).collect();
    assert_eq!(lines, vec![1, 3]);
}

#[test]
fn open_regular_refuses_a_symlink_swapped_in_after_the_walk() {
    let t = TmpDir::new();
    t.write("real.txt", "needle\n");
    assert!(open_regular(&t.0.join("real.txt")).is_ok());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(t.0.join("real.txt"), t.0.join("link.txt")).expect("symlink");
        assert!(open_regular(&t.0.join("link.txt")).is_err());
        fs::create_dir(t.0.join("dir")).expect("dir");
        assert!(open_regular(&t.0.join("dir")).is_err());
        let fifo = t.0.join("pipe");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .is_ok_and(|status| status.success());
        if made {
            // Non-blocking open: a FIFO with no writer must not hang.
            assert!(open_regular(&fifo).is_err());
        }
    }
}

#[test]
fn match_count_order_keeps_the_most_matched_file_past_the_path_prefix() {
    let t = TmpDir::new();
    for i in 0..30 {
        t.write(&format!("a{i:03}.txt"), "hit\n");
    }
    t.write("zzz-hot.txt", &"hit\n".repeat(40));
    let mut o = opts(t.path(), "hit");
    o.sort = Some("matchCount".into());
    o.max_collected_files = Some(10);
    let r = search(o.clone()).expect("ok");
    assert_eq!(r.files.len(), 10);
    assert!(
        r.files[0].path.ends_with("zzz-hot.txt"),
        "{}",
        r.files[0].path
    );
    // Ties keep ascending path order, so the retained set is deterministic.
    assert!(r.files[1].path.ends_with("a000.txt"));
    assert!(r.files[9].path.ends_with("a008.txt"));
    // Totals count every matched file, not only the retained ones.
    assert_eq!(r.stats.files_matched, Some(31));
    assert_eq!(r.stats.match_count, Some(70));
    assert_eq!(r.stats.cap_reason.as_deref(), Some("maxCollectedFiles"));
    for _ in 0..3 {
        let again = search(o.clone()).expect("ok");
        let paths =
            |files: &[RipgrepFile]| files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&again.files), paths(&r.files));
    }
}

#[test]
fn bounded_retention_matches_a_full_sort() {
    // Worker top-k retention must keep the same records as one full sort under
    // every density order, including relevance's secondary keys.
    fn rec(path: String, hits: u32, salt: u32) -> FileRec {
        FileRec {
            path,
            entry: FileEntry::new(),
            matched_lines: hits,
            submatches: hits,
            om_matches: Vec::new(),
            sort_time: None,
            line_weight: salt % 5,
            demoted: salt.is_multiple_of(3),
            generated: salt.is_multiple_of(7),
            declares: salt.is_multiple_of(4),
        }
    }
    for sort in ["matchCount", "relevance"] {
        let mut o = opts("/fixture".to_owned(), "p");
        o.sort = Some(sort.into());
        let all: Vec<(String, u32, u32)> = (0..200u32)
            .map(|i| (format!("f{:03}", (i * 37) % 200), (i * 13) % 7, i))
            .collect();
        let mut full: Vec<FileRec> = all
            .iter()
            .map(|(p, h, salt)| rec(p.clone(), *h, *salt))
            .collect();
        sort_recs(&o, Mode::Normal, &mut full);
        full.truncate(5);
        let mut bounded = Vec::new();
        for (p, h, salt) in &all {
            retain_into(
                &o,
                Mode::Normal,
                Some(5),
                &mut bounded,
                rec(p.clone(), *h, *salt),
            );
        }
        sort_and_cap(
            &{
                let mut capped = o.clone();
                capped.max_collected_files = Some(5);
                capped
            },
            Mode::Normal,
            &mut bounded,
        );
        let paths = |recs: &[FileRec]| recs.iter().map(|r| r.path.clone()).collect::<Vec<_>>();
        assert_eq!(paths(&bounded), paths(&full), "{sort}");
    }
}

#[test]
fn relevance_orders_by_count_then_source_path_then_line_weight_then_path() {
    let t = TmpDir::new();
    t.write("a_comment.rs", "// needle here\n");
    t.write("b_decl.rs", "fn needle() {}\n");
    t.write("c_tests/x_test.rs", "fn needle() {}\n");
    t.write("d_two.rs", "// needle\n// needle\n");
    // A pattern (not a bare identifier) ranks by density first.
    let mut o = opts(t.path(), "ne+dle");
    o.sort = Some("relevance".into());
    let r = search(o.clone()).expect("ok");
    let names = r
        .files
        .iter()
        .map(|f| f.path.rsplit('/').next().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["d_two.rs", "b_decl.rs", "a_comment.rs", "x_test.rs"]
    );
    // Path-list views: source paths first, then path.
    o.files_only = Some(true);
    let r = search(o).expect("ok");
    let names = r
        .files
        .iter()
        .map(|f| f.path.rsplit('/').next().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["a_comment.rs", "b_decl.rs", "d_two.rs", "x_test.rs"]
    );
}

/// A generated file (by its header or path) ranks after every hand-written
/// file, however many hits it holds; tests keep their place after the count.
#[test]
fn relevance_ranks_generated_files_after_hand_written_ones() {
    let t = TmpDir::new();
    t.write(
        "commands.def",
        "/* Automatically generated by generate-command-code.py, do not edit. */\nneedle\nneedle\nneedle\n",
    );
    t.write("api.generated.ts", "needle\nneedle\n");
    t.write("src/one.c", "x = needle;\n");
    t.write("tests/two_test.c", "needle;\nneedle;\n");
    let mut o = opts(t.path(), "ne+dle");
    o.sort = Some("relevance".into());
    let r = search(o.clone()).expect("ok");
    let names = r
        .files
        .iter()
        .map(|f| f.path.rsplit('/').next().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["two_test.c", "one.c", "commands.def", "api.generated.ts"]
    );
    // Every hit is still returned; only the order changes.
    let total: usize = r.files.iter().map(|f| f.matches.len()).sum();
    assert_eq!(total, 8);
    o.sort = Some("path".into());
    let by_path = search(o).expect("ok");
    assert_eq!(by_path.files.len(), 4);
}

/// An identifier search asks where a name lives: a source file declaring it
/// ranks before denser call sites, tests, and comments. A test file declaring
/// the name keeps the ordinary density order.
#[test]
fn identifier_search_ranks_the_declaring_source_file_first() {
    let t = TmpDir::new();
    t.write("a_calls.ts", &"newElementWith(el);\n".repeat(9));
    t.write(
        "b_tests/x.test.ts",
        "export const newElementWith = 1;\nnewElementWith();\n",
    );
    t.write(
        "z_src/mutate.ts",
        "export const newElementWith = <T>(el: T) => el;\n",
    );
    t.write(
        "y_src/MoreObjects.java",
        "  public static <T> T newElementWith(@Nullable T first) {\n",
    );
    let names = |pattern: &str, fixed: bool| {
        let mut o = opts(t.path(), pattern);
        o.sort = Some("relevance".into());
        o.fixed_string = Some(fixed);
        o.whole_word = Some(fixed);
        search(o)
            .expect("ok")
            .files
            .iter()
            .map(|f| f.path.rsplit('/').next().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    for fixed in [false, true] {
        assert_eq!(
            names("newElementWith", fixed),
            ["MoreObjects.java", "mutate.ts", "a_calls.ts", "x.test.ts"],
            "fixed {fixed}"
        );
    }
    // Any regex beyond a bare identifier keeps count-first order.
    assert_eq!(
        names("newElementWith\\b", false),
        ["a_calls.ts", "x.test.ts", "MoreObjects.java", "mutate.ts"]
    );
}

#[test]
fn aggregate_totals_saturate_instead_of_overflowing() {
    let state = CollectState::new();
    state.record_kept(u32::MAX, u32::MAX);
    state.record_kept(u32::MAX, u32::MAX);
    let r = build_result(
        &opts("/fixture".to_owned(), "p"),
        Mode::CountMatches,
        state.snapshot(),
    );
    assert_eq!(r.stats.match_count, Some(u32::MAX));
    assert_eq!(r.stats.matched_lines, Some(u32::MAX));
    assert_eq!(r.stats.files_matched, Some(2));
    assert_eq!(saturate_u32(u64::from(u32::MAX) * 2), u32::MAX);
}

#[test]
fn cancellation_stops_the_walk_mid_tree() {
    let t = TmpDir::new();
    for i in 0..60 {
        t.write(&format!("f{i:02}.txt"), "needle\n");
    }
    let mut o = opts(t.path(), "needle");
    o.sort = Some("traversal".into());
    let polls = AtomicU32::new(0);
    let cancel = || polls.fetch_add(1, Ordering::SeqCst) >= 6;
    let r = search_cancellable(o, Arc::new(AllowAll), &cancel).expect("ok");
    let searched = r.stats.files_searched.unwrap_or(0);
    assert!(searched <= 6, "walk kept going after cancel: {searched}");
    assert!(
        r.stats
            .cap_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("cancelled")),
        "{:?}",
        r.stats
    );
    assert_eq!(r.stats.capped, Some(true));
}

/// Test filter that stalls inside the walk on the Nth file it sees, standing in
/// for one uninterruptible PCRE2 match.
struct StallOnNth {
    seen: AtomicU32,
    nth: u32,
    stall: std::time::Duration,
}

impl RipgrepPathFilter for StallOnNth {
    fn allows(&self, _: &Path, is_dir: bool) -> bool {
        if !is_dir && self.seen.fetch_add(1, Ordering::SeqCst) + 1 == self.nth {
            std::thread::sleep(self.stall);
        }
        true
    }
}

#[test]
fn pcre2_hard_deadline_returns_files_finished_so_far() {
    use std::time::{Duration, Instant};
    let t = TmpDir::new();
    for i in 0..6 {
        t.write(&format!("f{i}.txt"), "needle\n");
    }
    let mut o = opts(t.path(), "needle");
    o.perl_regex = Some(true);
    o.sort = Some("traversal".into());
    let filter = Arc::new(StallOnNth {
        seen: AtomicU32::new(0),
        nth: 4,
        stall: Duration::from_millis(1500),
    });
    let started = Instant::now();
    let r = search_with_limits(
        o,
        filter,
        &|| false,
        Pcre2Limits {
            deadline: Duration::from_millis(100),
            grace: Duration::from_millis(100),
        },
    )
    .expect("ok");
    assert!(
        started.elapsed() < Duration::from_millis(1200),
        "driver must not wait for the stuck worker"
    );
    assert_eq!(r.files.len(), 3, "{:?}", r.stats);
    assert_eq!(r.stats.files_searched, Some(3));
    assert_eq!(r.stats.files_matched, Some(3));
    assert_eq!(r.stats.capped, Some(true));
    assert_eq!(r.stats.cap_reason.as_deref(), Some("pcre2Deadline"));
}

#[test]
fn pcre2_driver_honours_cancellation_while_the_worker_is_stuck() {
    use std::time::{Duration, Instant};
    let t = TmpDir::new();
    for i in 0..4 {
        t.write(&format!("f{i}.txt"), "needle\n");
    }
    let mut o = opts(t.path(), "needle");
    o.perl_regex = Some(true);
    o.sort = Some("traversal".into());
    let filter = Arc::new(StallOnNth {
        seen: AtomicU32::new(0),
        nth: 2,
        stall: Duration::from_millis(1500),
    });
    let started = Instant::now();
    let cancel = || started.elapsed() >= Duration::from_millis(100);
    let r = search_with_limits(o, filter, &cancel, PCRE2_LIMITS).expect("ok");
    assert!(started.elapsed() < Duration::from_millis(1200));
    assert_eq!(r.files.len(), 1, "{:?}", r.stats);
    assert!(
        r.stats
            .cap_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("cancelled"))
    );
}
