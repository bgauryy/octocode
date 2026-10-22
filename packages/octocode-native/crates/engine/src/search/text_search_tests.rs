use super::*;
use crate::test_support::TmpDir;
use crate::text::source::open_regular;
use std::fs;

struct AllowAll;
impl TextSearchPathFilter for AllowAll {
    fn allows(&self, _: &Path, _: bool) -> bool {
        true
    }
}

pub(crate) fn search(opts: TextSearchOptions) -> Result<TextSearchResult> {
    search_cancellable(opts, Arc::new(AllowAll), &|| false)
}

fn opts(path: String, pattern: &str) -> TextSearchOptions {
    TextSearchOptions {
        path,
        pattern: pattern.to_owned(),
        ..Default::default()
    }
}

/// A line of many matches is reachable whole: spans are bounded per file, not
/// per line, so a minified file's one long line yields every match.
#[test]
fn a_long_line_yields_every_span() {
    let t = TmpDir::new();
    let total = 14_024;
    t.write("many.txt", "needle ".repeat(total));
    for count_unique in [false, true] {
        let mut options = opts(t.path(), "needle");
        options.output = SearchOutput::Spans(if count_unique {
            SpanValues::Counted
        } else {
            SpanValues::Every
        });
        let result = search(options).expect("search ok");
        assert_eq!(result.stats.match_count, Some(total as u32));
        assert_eq!(result.stats.capped, Some(false));
        assert_eq!(result.stats.cap_reasons, []);
        if count_unique {
            assert_eq!(result.files[0].matches[0].count, Some(total as u32));
        } else {
            assert_eq!(result.files[0].matches.len(), total);
        }
    }
}

#[test]
fn span_collection_past_the_per_file_bound_reports_incomplete() {
    let t = TmpDir::new();
    let bound = MAX_ONLY_MATCHING_PER_FILE as usize;
    t.write("many.txt", "needle ".repeat(bound + 1));
    let mut options = opts(t.path(), "needle");
    options.output = SearchOutput::Spans(SpanValues::Every);
    let result = search(options).expect("search ok");
    assert_eq!(result.stats.match_count, Some(bound as u32 + 1));
    assert_eq!(result.stats.capped, Some(true));
    assert_eq!(
        result.stats.cap_reasons,
        [CapReason::MaxOnlyMatchingPerFile]
    );
    assert_eq!(result.files[0].matches.len(), bound);
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

/// One matcher option changes which lines (or submatches) count in one file.
#[test]
fn matcher_options_decide_the_match_count() {
    type Configure = fn(&mut TextSearchOptions);
    // (case, content, pattern, option, match_count, first match (line, value), stats.match_count)
    type Case = (
        &'static str,
        &'static str,
        &'static str,
        Configure,
        u32,
        Option<(u32, &'static str)>,
        Option<u32>,
    );
    let cases: [Case; 7] = [
        (
            "smart case default is case-insensitive for lowercase",
            "Hello\nhello\n",
            "hello",
            |_| {},
            2,
            None,
            None,
        ),
        (
            "case_sensitive is exact",
            "Hello\nhello\n",
            "hello",
            |o| o.case = CaseRule::Sensitive,
            1,
            None,
            None,
        ),
        // Literal "a.b" matches only line 1, not "axb".
        (
            "fixed_string treats the pattern literally",
            "a.b\naxb\n",
            "a.b",
            |o| o.syntax = PatternSyntax::Literal,
            1,
            Some((1, "a.b")),
            None,
        ),
        (
            "perl_regex supports lookahead",
            "foobar\nfoobaz\n",
            "foo(?=bar)",
            |o| o.syntax = PatternSyntax::Pcre2,
            1,
            Some((1, "foobar")),
            None,
        ),
        (
            "whole_word requires a word boundary",
            "foo\nfoobar\n",
            "foo",
            |o| o.whole_word = Some(true),
            1,
            Some((1, "foo")),
            None,
        ),
        (
            "count_matches_per_file counts submatches",
            "x x x\nx\n",
            "x",
            |o| o.output = SearchOutput::CountMatches,
            4,
            None,
            Some(4),
        ),
        (
            "count_lines_per_file counts matched lines",
            "x x x\nx\nnope\n",
            "x",
            |o| o.output = SearchOutput::CountLines,
            2,
            None,
            None,
        ),
    ];
    for (case, content, pattern, configure, match_count, first, stats_count) in cases {
        let t = TmpDir::new();
        t.write("a.txt", content);
        let mut o = opts(t.path(), pattern);
        configure(&mut o);
        let r = search(o).expect(case);
        assert_eq!(r.files[0].match_count, match_count, "{case}");
        if let Some((line, value)) = first {
            assert_eq!(r.files[0].matches[0].line, line, "{case}");
            assert_eq!(r.files[0].matches[0].value, value, "{case}");
        }
        if let Some(count) = stats_count {
            assert_eq!(r.stats.match_count, Some(count), "{case}");
        }
    }
}

/// One file-selection option decides which files are listed, in order.
#[test]
fn file_selection_options_decide_the_listed_files() {
    type Configure = fn(&mut TextSearchOptions);
    // (case, files, pattern, option, listed path suffixes in order)
    type Case = (
        &'static str,
        &'static [(&'static str, &'static str)],
        &'static str,
        Configure,
        &'static [&'static str],
    );
    let cases: [Case; 4] = [
        (
            "files_without_match inverts the file set",
            &[("a.txt", "needle\n"), ("b.txt", "haystack\n")],
            "needle",
            |o| o.output = SearchOutput::FilesWithoutMatch,
            &["b.txt"],
        ),
        (
            "lang_type filters by extension",
            &[("a.ts", "target\n"), ("b.py", "target\n")],
            "target",
            |o| o.lang_type = Some("ts".to_owned()),
            &["a.ts"],
        ),
        (
            "include glob restricts files",
            &[("a.ts", "target\n"), ("b.js", "target\n")],
            "target",
            |o| o.include = Some(vec!["*.ts".to_owned()]),
            &["a.ts"],
        ),
        (
            "sort_reverse flips the order",
            &[("a.txt", "m\n"), ("z.txt", "m\n")],
            "m",
            |o| o.sort_reverse = Some(true),
            &["z.txt", "a.txt"],
        ),
    ];
    for (case, files, pattern, configure, listed) in cases {
        let t = TmpDir::new();
        for (name, content) in files {
            t.write(name, content);
        }
        let mut o = opts(t.path(), pattern);
        configure(&mut o);
        let r = search(o).expect(case);
        assert_eq!(r.files.len(), listed.len(), "{case}");
        for (file, suffix) in r.files.iter().zip(listed) {
            assert!(
                file.path.ends_with(suffix),
                "{case}: {} vs {suffix}",
                file.path
            );
        }
    }
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
    t.write("a.txt", format!("{}!\n", "a".repeat(5_000)));
    let path = t.path();
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let mut o = opts(path, "(a+)+$");
        o.syntax = PatternSyntax::Pcre2;
        let _ = tx.send(search(o));
    });
    let outcome = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("catastrophic PCRE2 pattern must terminate, not hang");
    // PCRE2 either completes (the line ends in `!`, so nothing matches) or
    // stops at its JIT stack / match limit with an error naming the regex.
    match outcome {
        Ok(result) => assert!(result.files.is_empty(), "{:?}", result.files),
        Err(error) => assert!(!error.to_string().is_empty(), "{error:?}"),
    }
    handle.join().expect("worker thread panicked");
}

#[test]
fn files_only_lists_paths_without_snippets() {
    let t = TmpDir::new();
    t.write("a.txt", "needle\n");
    t.write("b.txt", "haystack\n");
    let mut o = opts(t.path(), "needle");
    o.output = SearchOutput::FilesWithMatch;
    let r = search(o).expect("ok");
    assert_eq!(r.files.len(), 1);
    assert_eq!(r.files[0].match_count, 1);
    assert!(r.files[0].matches.is_empty());
    assert!(r.files[0].path.ends_with("a.txt"));
    assert_eq!(r.stats.files_matched, Some(1));
    assert!(r.stats.bytes_searched.unwrap_or_default() > 0);
}

/// `invertMatch` selects non-matching *lines*, so the path views
/// follow lines: `files` lists files with at least one non-matching line, and
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
        o.output = if files_only {
            SearchOutput::FilesWithMatch
        } else {
            SearchOutput::FilesWithoutMatch
        };
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
    o.context = crate::types::SearchContext::symmetric(1);
    let r = search(o).expect("ok");
    let v = &r.files[0].matches[0].value;
    assert!(
        v.contains("before") && v.contains("match") && v.contains("after"),
        "{v}"
    );
}

#[test]
fn exclude_globs_drop_files_with_or_without_a_leading_bang() {
    for exclude in ["*.log", "!*.log"] {
        let t = TmpDir::new();
        t.write("a.txt", "target\n");
        t.write("b.log", "target\n");
        let mut o = opts(t.path(), "target");
        o.exclude = Some(vec![exclude.to_owned()]);
        let r = search(o).expect("ok");
        assert_eq!(r.files.len(), 1, "{exclude}");
        assert!(r.files[0].path.ends_with("a.txt"), "{exclude}");
    }
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
    assert_eq!(r.stats.cap_reasons, [CapReason::MaxCollectedFiles]);
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
            covered: 0,
            source: None,
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
    t.write("big.txt", "needle ".repeat(50)); // 350 bytes, over the ceiling
    let mut o = opts(t.path(), "needle");
    o.max_file_bytes = Some(10);
    let r = search(o).expect("ok");
    assert_eq!(r.files.len(), 1);
    assert!(r.files[0].path.ends_with("small.txt"));
    assert_eq!(r.stats.capped, Some(true));
    assert!(r.stats.cap_reasons.contains(&CapReason::MaxFileSize));
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
    assert!(r.stats.cap_reasons.contains(&CapReason::BinaryQuit));
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
        o.sort = SearchSort::Traversal;
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
    o.sort = SearchSort::Traversal;
    assert!(preserves_traversal_order(&o));
    o.sort = SearchSort::Path;
    assert!(!preserves_traversal_order(&o));
}

#[test]
fn respects_gitignore_by_default_and_no_ignore_overrides() {
    let t = TmpDir::new();
    // The `ignore` crate only applies .gitignore inside a git repo
    // (require_git defaults true); a bare `.git` dir marks the temp tree as one.
    fs::create_dir_all(t.0.join(".git")).expect("create .git");
    t.write(".gitignore", "ignored.txt\n");
    t.write("ignored.txt", "target\n");
    t.write("kept.txt", "target\n");

    let r = search(opts(t.path(), "target")).expect("ok");
    assert_eq!(r.files.len(), 1, "gitignore'd file should be skipped");
    assert!(r.files[0].path.ends_with("kept.txt"));

    let mut o = opts(t.path(), "target");
    o.ignore.include_ignored = true;
    let r2 = search(o).expect("ok");
    assert_eq!(r2.files.len(), 2, "--no-ignore searches the ignored file");
}

#[test]
fn empty_result_when_no_match() {
    let t = TmpDir::new();
    t.write("a.txt", "nothing here\n");
    let r = search(opts(t.path(), "absent")).expect("ok");
    assert!(r.files.is_empty());
}

// ── only-matching ───────────────────────────────────────────────

#[test]
fn only_matching_emits_one_match_per_submatch() {
    let t = TmpDir::new();
    t.write("a.txt", "ab ab ab\n");
    let mut o = opts(t.path(), "ab");
    o.output = SearchOutput::Spans(SpanValues::Every);
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
    o.output = SearchOutput::Spans(SpanValues::Every);
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
    o.output = SearchOutput::Spans(SpanValues::Every);
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
    o.output = SearchOutput::Spans(SpanValues::Every);
    o.output = SearchOutput::Spans(SpanValues::Unique);
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
    o.output = SearchOutput::Spans(SpanValues::Every);
    o.output = SearchOutput::Spans(SpanValues::Counted);
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
    o.output = SearchOutput::FilesWithoutMatch;
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
    o.output = SearchOutput::Spans(SpanValues::Every);
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
    o.context = crate::types::SearchContext::symmetric(1);
    let r = search(o).expect("ok");
    let m = &r.files[0].matches;
    assert_eq!(m[0].value, "alpha foo\nFoo bar");
    assert_eq!(m[2].value, "Foo bar\nfoo foo foo\nbaz");
}

#[test]
fn long_line_snippet_keeps_the_match_visible() {
    let t = TmpDir::new();
    t.write("long.txt", format!("{}foo tail\n", "x".repeat(5000)));
    let r = search(opts(t.path(), "foo")).expect("ok");
    let m = &r.files[0].matches[0];
    assert!(m.value.contains("foo"), "{}", &m.value[..60]);
    assert!(m.value.starts_with('…'));
    assert!(m.value.chars().count() <= 500);
    // The whole line, before the snippet window cut it.
    assert_eq!(m.original_chars, Some(5_008));
}

#[test]
fn long_leading_context_does_not_hide_the_match() {
    let t = TmpDir::new();
    t.write("ctx.txt", format!("{}\ntarget here\n", "C".repeat(600)));
    let mut o = opts(t.path(), "target");
    o.context = crate::types::SearchContext::symmetric(1);
    let r = search(o).expect("ok");
    assert!(r.files[0].matches[0].value.contains("target"));
}

/// A file whose first NUL comes before any text (image, object file, archive
/// headers) is opaque binary: skipped, counted, and not a
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
    assert_eq!(r.stats.cap_reasons, [], "{:?}", r.stats);
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
    o.output = SearchOutput::FilesWithoutMatch;
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
    assert_eq!(r.stats.cap_reasons, [CapReason::BinaryQuit]);
}

fn extension_groups(stats: &TextSearchStats) -> Vec<(&str, u32)> {
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
    assert_eq!(r.stats.cap_reasons, [], "{:?}", r.stats);
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
    assert_eq!(r.stats.cap_reasons, [CapReason::BinaryQuit]);
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
    assert_eq!(r.stats.cap_reasons, [CapReason::BinaryQuit]);
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
    assert_eq!(r.stats.cap_reasons, [CapReason::BinaryQuit]);
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
    let (mut file, meta) = open_regular(&t.0.join("real.txt")).expect("a regular file opens");
    assert!(meta.is_file());
    assert_eq!(meta.len(), 7);
    let mut text = String::new();
    std::io::Read::read_to_string(&mut file, &mut text).expect("read");
    assert_eq!(text, "needle\n");
    #[cfg(unix)]
    {
        let not_regular = |path: &Path| {
            let error = open_regular(path).expect_err("refused");
            assert!(
                error.to_string().contains("not a regular file when opened"),
                "{}: {error}",
                path.display()
            );
        };
        std::os::unix::fs::symlink(t.0.join("real.txt"), t.0.join("link.txt")).expect("symlink");
        // O_NOFOLLOW: the link itself is never opened.
        let link = open_regular(&t.0.join("link.txt")).expect_err("symlink refused");
        assert_eq!(link.raw_os_error(), Some(libc::ELOOP), "{link}");
        fs::create_dir(t.0.join("dir")).expect("dir");
        not_regular(&t.0.join("dir"));
        let fifo = t.0.join("pipe");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .is_ok_and(|status| status.success());
        if made {
            // Non-blocking open: a FIFO with no writer must not hang.
            not_regular(&fifo);
        }
    }
}

#[test]
fn match_count_order_keeps_the_most_matched_file_past_the_path_prefix() {
    let t = TmpDir::new();
    for i in 0..30 {
        t.write(&format!("a{i:03}.txt"), "hit\n");
    }
    t.write("zzz-hot.txt", "hit\n".repeat(40));
    let mut o = opts(t.path(), "hit");
    o.sort = SearchSort::MatchCount;
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
    assert_eq!(r.stats.cap_reasons, [CapReason::MaxCollectedFiles]);
    for _ in 0..3 {
        let again = search(o.clone()).expect("ok");
        let paths =
            |files: &[TextSearchFile]| files.iter().map(|f| f.path.clone()).collect::<Vec<_>>();
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
            covered: 0,
            source: None,
        }
    }
    for sort in [SearchSort::MatchCount, SearchSort::Relevance] {
        let mut o = opts("/fixture".to_owned(), "p");
        o.sort = sort;
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
        assert_eq!(paths(&bounded), paths(&full), "{sort:?}");
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
    o.sort = SearchSort::Relevance;
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
    o.output = SearchOutput::FilesWithMatch;
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
    o.sort = SearchSort::Relevance;
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
    o.sort = SearchSort::Path;
    let by_path = search(o).expect("ok");
    assert_eq!(by_path.files.len(), 4);
}

/// An identifier search asks where a name lives: a source file declaring it
/// ranks before denser call sites, tests, and comments. A test file declaring
/// the name keeps the ordinary density order.
#[test]
fn identifier_search_ranks_the_declaring_source_file_first() {
    let t = TmpDir::new();
    t.write("a_calls.ts", "newElementWith(el);\n".repeat(9));
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
        o.sort = SearchSort::Relevance;
        o.syntax = if fixed {
            PatternSyntax::Literal
        } else {
            PatternSyntax::Regex
        };
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
    o.sort = SearchSort::Traversal;
    let polls = AtomicU32::new(0);
    let cancel = || polls.fetch_add(1, Ordering::SeqCst) >= 6;
    let r = search_cancellable(o, Arc::new(AllowAll), &cancel).expect("ok");
    let searched = r.stats.files_searched.unwrap_or(0);
    assert!(searched <= 6, "walk kept going after cancel: {searched}");
    assert!(
        r.stats.cap_reasons.contains(&CapReason::Cancelled),
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

impl TextSearchPathFilter for StallOnNth {
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
    o.syntax = PatternSyntax::Pcre2;
    o.sort = SearchSort::Traversal;
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
    assert_eq!(r.stats.cap_reasons, [CapReason::Pcre2Deadline]);
}

#[test]
fn pcre2_driver_honours_cancellation_while_the_worker_is_stuck() {
    use std::time::{Duration, Instant};
    let t = TmpDir::new();
    for i in 0..4 {
        t.write(&format!("f{i}.txt"), "needle\n");
    }
    let mut o = opts(t.path(), "needle");
    o.syntax = PatternSyntax::Pcre2;
    o.sort = SearchSort::Traversal;
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
    assert!(r.stats.cap_reasons.contains(&CapReason::Cancelled));
}

/// SHA-256 of a fixture file's bytes, for comparing with a search digest.
fn file_digest(path: &Path) -> [u8; 32] {
    Sha256::digest(fs::read(path).expect("fixture bytes")).into()
}

/// A digest-asking search hashes each kept file after its search, and the
/// digest equals a separate hash of the file; without the option nothing is
/// hashed.
#[test]
fn kept_files_carry_the_digest_of_the_bytes_searched() {
    let t = TmpDir::new();
    let long = format!(
        "needle head\n{}needle tail\n",
        "filler line\n".repeat(20_000)
    );
    t.write("long.txt", long);
    t.write("short.txt", "one needle\n");
    t.write("none.txt", "nothing here\n");
    for view in ["normal", "count", "files", "without"] {
        let mut options = opts(t.path(), "needle");
        options.values = SourceValues::Hashed;
        options.output = match view {
            "count" => SearchOutput::CountMatches,
            "files" => SearchOutput::FilesWithMatch,
            "without" => SearchOutput::FilesWithoutMatch,
            _ => SearchOutput::Lines,
        };
        let result = search(options).expect("search");
        assert!(!result.files.is_empty(), "{view}");
        for file in &result.files {
            let path = Path::new(&file.path);
            let source = file.source.expect("digest of a kept file");
            assert_eq!(source.digest, file_digest(path), "{view} {}", file.path);
            let meta = fs::metadata(path).expect("meta");
            assert_eq!(source.size, meta.len());
            assert_eq!(source.modified, Some(meta.modified().expect("mtime")));
        }
    }
    let unhashed = search(opts(t.path(), "needle")).expect("search");
    assert!(unhashed.files.iter().all(|file| file.source.is_none()));
}

/// A symlink the walk admits but does not follow (to a file or a directory)
/// is reported to the path filter, never dropped unseen.
#[cfg(unix)]
#[test]
fn an_unfollowed_symlink_is_reported_to_the_filter() {
    struct Recording(std::sync::Mutex<Vec<String>>);
    impl TextSearchPathFilter for Recording {
        fn allows(&self, _: &Path, _: bool) -> bool {
            true
        }
        fn skipped_symlink(&self, path: &Path) {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            self.0.lock().expect("lock").push(name.into_owned());
        }
    }
    let t = TmpDir::new();
    t.write("real.txt", "needle\n");
    t.write("dir/inner.txt", "needle\n");
    std::os::unix::fs::symlink(t.0.join("real.txt"), t.0.join("file-link")).expect("link");
    std::os::unix::fs::symlink(t.0.join("dir"), t.0.join("dir-link")).expect("link");
    let filter = Arc::new(Recording(std::sync::Mutex::new(Vec::new())));
    let result =
        search_cancellable(opts(t.path(), "needle"), filter.clone(), &|| false).expect("search");
    assert_eq!(result.files.len(), 2);
    let mut seen = filter.0.lock().expect("lock").clone();
    seen.sort();
    assert_eq!(seen, ["dir-link", "file-link"]);
}

/// A kept file's digest proves the opened file whole: a file whose length
/// moved after it was opened carries no digest.
#[test]
fn a_file_that_moved_after_opening_has_no_digest() {
    let t = TmpDir::new();
    let path = t.write("grow.txt", "needle\n");
    let file = std::fs::File::open(&path).expect("open");
    let meta = file.metadata().expect("meta");
    let source = hashed_source(&file, &meta).expect("unchanged file");
    assert_eq!(source.digest, file_digest(&path));
    assert_eq!(source.size, meta.len());
    fs::write(&path, "needle\nmore\n").expect("grow");
    assert_eq!(hashed_source(&file, &meta), None);
}

/// A file quit at its first NUL is searched again up to it; its digest
/// still covers every byte, including the unsearched tail after the NUL.
#[test]
fn a_binary_cut_file_digest_covers_the_whole_file() {
    let t = TmpDir::new();
    let mut bytes = b"needle before the cut\n".repeat(50);
    bytes.push(0);
    bytes.extend(b"after the cut needle\n".repeat(10_000));
    let path = t.0.join("mixed.log");
    fs::write(&path, &bytes).expect("fixture");
    let mut options = opts(t.path(), "needle");
    options.values = SourceValues::Hashed;
    let result = search(options).expect("search");
    assert_eq!(result.files.len(), 1);
    assert_eq!(result.files[0].matches.len(), 50);
    let source = result.files[0].source.expect("digest");
    assert_eq!(source.digest, file_digest(&path));
    assert_eq!(source.size, bytes.len() as u64);
}

/// The relevance rank reads the generated-file header from the bytes the
/// search read, so a header past the first read buffer is still the file's
/// first bytes, and a short file is judged whole.
#[test]
fn relevance_reads_the_generated_header_from_the_searched_bytes() {
    let t = TmpDir::new();
    t.write("short.c", "// @generated\nneedle\n");
    t.write(
        "long.c",
        format!(
            "// Code generated by x. DO NOT EDIT.\n{}",
            "needle\n".repeat(40_000)
        ),
    );
    t.write("hand.c", "needle\n");
    let mut options = opts(t.path(), "needle");
    options.sort = SearchSort::Relevance;
    let result = search(options).expect("search");
    let names = result
        .files
        .iter()
        .map(|file| {
            Path::new(&file.path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        names.first().map(String::as_str),
        Some("hand.c"),
        "{names:?}"
    );
    assert_eq!(names.len(), 3);
}

/// Review M6: only-matching columns on one long line are computed with a
/// left-to-right cursor, not a rescan of the line prefix per span. 1000 spans
/// at the end of a ~4 MB multibyte line rescanned ~12 GB before the fix.
#[test]
fn only_matching_long_line_columns_are_linear_and_exact() {
    let t = TmpDir::new();
    let prefix = "é".repeat(2_000_000);
    let mut line = prefix.clone();
    for _ in 0..1000 {
        line.push_str("😀x");
    }
    line.push('\n');
    t.write("long.txt", line);
    let mut o = opts(t.path(), "x");
    o.output = SearchOutput::Spans(SpanValues::Every);
    let started = std::time::Instant::now();
    let r = search(o).expect("ok");
    let elapsed = started.elapsed();
    let matches = &r.files[0].matches;
    assert_eq!(matches.len(), 1000);
    for (i, m) in matches.iter().enumerate() {
        // 'é' is one UTF-16 unit, '😀' is two.
        assert_eq!(m.column as usize, 2_000_000 + 3 * i + 2);
        assert_eq!(m.value, "x");
    }
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "only-matching span columns took {elapsed:?}"
    );
}

/// `$` matches at the end of a CRLF line on both regex
/// engines, and the shown value carries no trailing `\r`.
#[test]
fn end_anchor_matches_crlf_lines() {
    let t = TmpDir::new();
    t.write(
        "crlf.txt",
        "line one needle\r\nline two needle\r\nneedle inside\r\n",
    );
    for perl in [false, true] {
        let mut o = opts(t.path(), "needle$");
        o.syntax = if perl {
            PatternSyntax::Pcre2
        } else {
            PatternSyntax::Regex
        };
        let r = search(o).expect("search ok");
        let lines: Vec<_> = r
            .files
            .iter()
            .flat_map(|f| f.matches.iter().map(|m| m.line))
            .collect();
        assert_eq!(lines, [1, 2], "perl={perl}");
        assert!(
            r.files[0].matches.iter().all(|m| !m.value.ends_with('\r')),
            "perl={perl}: {:?}",
            r.files[0].matches
        );
    }
}

/// File-level AND/NOT: a matched file is kept only when it also contains
/// every `file_contains_all` term and none of `file_contains_none`; totals
/// count only kept files. Each term is smart-case on its own.
#[test]
fn file_terms_keep_files_containing_all_and_none_of_the_excluded() {
    let t = TmpDir::new();
    t.write("a.rs", "alpha\nbeta\n");
    t.write("b.rs", "alpha\nGAMMA\n");
    t.write("c.rs", "alpha\nbeta\ngamma\n");
    t.write("d.rs", "alpha only\n");
    let mut o = opts(t.path(), "alpha");
    o.file_contains_all = Some(vec!["beta".into()]);
    o.file_contains_none = Some(vec!["gamma".into()]);
    let r = search(o).expect("search ok");
    let paths: Vec<_> = r
        .files
        .iter()
        .map(|f| f.path.rsplit('/').next().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(paths, ["a.rs"], "{paths:?}");
    assert_eq!(r.stats.files_matched, Some(1));
    // An uppercase term is case-sensitive: `GAMMA` excludes only b.rs.
    let mut o = opts(t.path(), "alpha");
    o.file_contains_none = Some(vec!["GAMMA".into()]);
    let r = search(o).expect("search ok");
    let mut paths: Vec<_> = r
        .files
        .iter()
        .map(|f| f.path.rsplit('/').next().unwrap_or_default().to_owned())
        .collect();
    paths.sort();
    assert_eq!(paths, ["a.rs", "c.rs", "d.rs"], "{paths:?}");
}

/// `.gitattributes` `linguist-generated`/`linguist-vendored` mark a file
/// generated for the relevance rank, as git resolves them: the deepest
/// attributes file first, its last matching line deciding, `-attr` unsetting.
#[test]
fn relevance_ranks_gitattributes_generated_files_last() {
    let t = TmpDir::new();
    t.write(".git/HEAD", "ref: refs/heads/main\n");
    t.write(
        ".gitattributes",
        "gen/** linguist-generated=true\nlib/*.ts linguist-generated\nthird/** linguist-vendored\n",
    );
    t.write("lib/.gitattributes", "keep.ts -linguist-generated\n");
    t.write("gen/api.ts", "needle\n".repeat(9));
    t.write("lib/out.ts", "needle\n".repeat(8));
    t.write("third/dep.ts", "needle\n".repeat(7));
    t.write("lib/keep.ts", "needle\nneedle\n");
    t.write("src/a.ts", "needle\n");
    let mut options = opts(t.path(), "needle");
    options.sort = SearchSort::Relevance;
    let result = search(options).expect("search");
    let names: Vec<String> = result
        .files
        .iter()
        .map(|file| {
            Path::new(&file.path)
                .strip_prefix(t.path())
                .unwrap_or(Path::new(&file.path))
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    assert_eq!(
        names,
        [
            "lib/keep.ts",
            "src/a.ts",
            "gen/api.ts",
            "lib/out.ts",
            "third/dep.ts"
        ],
        "{names:?}"
    );
}

/// For a word alternation (`retry|backoff|jitter`), a file holding more of
/// its words outranks one repeating a single common word, then hit count
/// decides. Other searches keep the count order.
#[test]
fn relevance_ranks_alternation_coverage_before_hit_count() {
    let t = TmpDir::new();
    t.write("flood.rs", "retry\n".repeat(40));
    t.write("all.rs", "retry\nbackoff\nJitter\n");
    t.write("two.rs", "retry\nretry\nbackoff\n");
    let rank = |pattern: &str| {
        let mut options = opts(t.path(), pattern);
        options.sort = SearchSort::Relevance;
        search(options)
            .expect("search")
            .files
            .iter()
            .map(|file| {
                Path::new(&file.path)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        rank("retry|backoff|jitter"),
        ["all.rs", "two.rs", "flood.rs"]
    );
    assert_eq!(
        rank(r"\b(retry|backoff|jitter)\b"),
        ["all.rs", "two.rs", "flood.rs"]
    );
    assert_eq!(rank("retry"), ["flood.rs", "two.rs", "all.rs"]);
}

/// Without `includeHidden` the walk leaves dot entries out and reports each
/// one to the path filter (a dot directory once, never its contents); an
/// include glob or `language` type that selects a dot file keeps it, as the
/// `ignore` walker's own rule does. With `includeHidden` nothing is skipped.
#[test]
fn hidden_entries_are_skipped_and_reported() {
    struct Recording(Mutex<BTreeSet<String>>);
    impl TextSearchPathFilter for Recording {
        fn allows(&self, _: &Path, _: bool) -> bool {
            true
        }
        fn skipped_hidden(&self, path: &Path, _: bool) {
            self.0
                .lock()
                .expect("lock")
                .insert(path.file_name().expect("name").to_string_lossy().into());
        }
    }
    let t = TmpDir::new();
    t.write("src/a.js", "needle\n");
    t.write(".github/workflows/ci.yml", "needle\n");
    t.write(".eslintrc.js", "needle\n");
    t.write("src/.local.js", "needle\n");
    let run = |configure: &dyn Fn(&mut TextSearchOptions)| {
        let filter = Arc::new(Recording(Mutex::default()));
        let mut options = opts(t.path(), "needle");
        configure(&mut options);
        let found = search_cancellable(options, filter.clone(), &|| false).expect("search");
        let mut files = found
            .files
            .iter()
            .map(|file| {
                Path::new(&file.path)
                    .strip_prefix(&t.0)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        files.sort();
        let skipped = filter.0.lock().expect("lock").clone();
        (files, skipped.into_iter().collect::<Vec<_>>())
    };
    assert_eq!(
        run(&|_| {}),
        (
            vec!["src/a.js".to_owned()],
            vec![
                ".eslintrc.js".to_owned(),
                ".github".to_owned(),
                ".local.js".to_owned()
            ]
        )
    );
    // The dot directory may hold another `.eslintrc.js`: still a skip.
    assert_eq!(
        run(&|options| options.include = Some(vec!["**/.eslintrc.js".into()])),
        (vec![".eslintrc.js".to_owned()], vec![".github".to_owned()])
    );
    let (typed, skipped) = run(&|options| options.lang_type = Some("js".into()));
    assert_eq!(typed, [".eslintrc.js", "src/.local.js", "src/a.js"]);
    assert_eq!(skipped, [".github"]);
    let (all, skipped) = run(&|options| options.ignore = super::super::walk::IgnorePolicy::ALL);
    assert_eq!(
        all,
        [
            ".eslintrc.js",
            ".github/workflows/ci.yml",
            "src/.local.js",
            "src/a.js"
        ]
    );
    assert!(skipped.is_empty());
}

/// The search applies the ignore rules itself (so it can report what they
/// leave out), and must walk exactly what the `ignore` walker with its own
/// ignore rules walks: nested `.gitignore` files, a `!` re-include, `.ignore`
/// beating `.gitignore`, `.git/info/exclude`, rules above a subdirectory
/// root, a nested repository, a directory outside any repository, and an
/// include glob or `language` type over ignored and dot entries, under each
/// `includeIgnored`/`includeHidden` combination. Every reported ignored
/// entry is one the walker left out, and nothing under it was searched.
#[test]
fn the_search_walks_exactly_what_the_ignore_walker_walks() {
    use super::super::walk::IgnorePolicy;
    struct Reported(Mutex<Vec<String>>);
    impl TextSearchPathFilter for Reported {
        fn allows(&self, _: &Path, _: bool) -> bool {
            true
        }
        fn skipped_ignored(&self, path: &Path, _: bool) {
            self.0
                .lock()
                .expect("lock")
                .push(path.to_string_lossy().into_owned());
        }
    }
    let repo = TmpDir::new();
    repo.write(".git/info/exclude", "excluded.txt\n");
    repo.write(".gitignore", "*.log\nbuild/\n!keep.log\n!.keepme\n");
    repo.write("app/.gitignore", "gen/\n");
    repo.write("app/.ignore", "!gen/\nlocal.txt\n");
    repo.write("nested/.git/HEAD", "ref: refs/heads/main\n");
    repo.write("nested/.gitignore", "inner.rs\n");
    for file in [
        "a.rs",
        "a.log",
        "keep.log",
        ".keepme",
        ".hidden.rs",
        "excluded.txt",
        "build/out.rs",
        "build/.dot.rs",
        "app/main.rs",
        "app/gen/x.rs",
        "app/local.txt",
        "app/nested/deep.log",
        "app/nested/ok.rs",
        "app/.cache/c.rs",
        "nested/inner.rs",
        "nested/outer.rs",
        "nested/x.log",
    ] {
        repo.write(file, "x\n");
    }
    let plain = TmpDir::new();
    plain.write(".gitignore", "a.txt\n");
    plain.write(".ignore", "b.txt\n");
    for file in ["a.txt", "b.txt", "c.txt", "sub/b.txt", "sub/.d.txt"] {
        plain.write(file, "x\n");
    }
    let roots = [
        repo.0.clone(),
        repo.0.join("app"),
        repo.0.join("nested"),
        plain.0.clone(),
    ];
    let policies = [
        IgnorePolicy::default(),
        IgnorePolicy::new(Some(true), None),
        IgnorePolicy::new(None, Some(true)),
        IgnorePolicy::ALL,
    ];
    type Scope = (Option<Vec<String>>, Option<String>);
    let scopes: [Scope; 3] = [
        (None, None),
        (Some(vec!["*.log".into(), "**/*.rs".into()]), None),
        (None, Some("rust".into())),
    ];
    let files_of = |root: &Path, paths: Vec<String>| -> BTreeSet<String> {
        paths
            .into_iter()
            .map(|path| {
                Path::new(&path)
                    .strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    };
    for root in &roots {
        for policy in policies {
            for (include, lang) in &scopes {
                let mut walker = super::super::walk::walk_builder(root, policy, None);
                if let Some(include) = include {
                    walker.overrides(
                        super::super::glob::compile(root, include, &[])
                            .expect("globs")
                            .overrides(),
                    );
                }
                if let Some(lang) = lang {
                    let mut types = TypesBuilder::new();
                    types.add_defaults();
                    types.select(lang);
                    walker.types(types.build().expect("types"));
                }
                let walked = files_of(
                    root,
                    walker
                        .build()
                        .filter_map(std::result::Result::ok)
                        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
                        .map(|entry| entry.path().to_string_lossy().into_owned())
                        .collect(),
                );
                let reported = Arc::new(Reported(Mutex::default()));
                // Every fixture line is non-empty: `.` lists every searched file.
                let mut options = opts(root.to_string_lossy().into_owned(), ".");
                options.ignore = policy;
                options.include.clone_from(include);
                options.lang_type.clone_from(lang);
                options.output = SearchOutput::FilesWithMatch;
                let found =
                    search_cancellable(options, reported.clone(), &|| false).expect("search");
                let searched = files_of(
                    root,
                    found.files.into_iter().map(|file| file.path).collect(),
                );
                let case = format!("{root:?} {policy:?} {include:?} {lang:?}");
                assert_eq!(searched, walked, "{case}");
                let reported = files_of(root, reported.0.lock().expect("lock").clone());
                assert!(
                    !policy.include_ignored || reported.is_empty(),
                    "{case}: {reported:?}"
                );
                for entry in &reported {
                    assert!(
                        !walked.contains(entry),
                        "{case}: reported {entry} was walked"
                    );
                    let below = format!("{entry}/");
                    assert!(
                        searched.iter().all(|file| !file.starts_with(&below)),
                        "{case}: searched under ignored {entry}"
                    );
                }
                if !policy.include_ignored && root == &repo.0 && include.is_none() && lang.is_none()
                {
                    for left_out in ["a.log", "excluded.txt", "build", "nested/inner.rs"] {
                        assert!(
                            reported.contains(left_out),
                            "{case}: {left_out} unreported: {reported:?}"
                        );
                    }
                    // A nested repository's own rules decide below it.
                    assert!(searched.contains("nested/x.log"), "{case}: {searched:?}");
                }
            }
        }
    }
}

/// A file root is searched on the calling thread: the parallel walker's
/// pool (and its idle 1 ms polls) is a fixed cost one entry never repays.
/// A directory root still walks on the pool. Both find the same match.
#[test]
fn a_file_root_is_searched_on_the_calling_thread() {
    struct Threads(Mutex<Vec<std::thread::ThreadId>>);
    impl TextSearchPathFilter for Threads {
        fn allows(&self, _: &Path, is_dir: bool) -> bool {
            if !is_dir {
                self.0
                    .lock()
                    .expect("lock")
                    .push(std::thread::current().id());
            }
            true
        }
    }
    let t = TmpDir::new();
    t.write("one.rs", "fn needle() {}\n");
    let file = Path::new(&t.path()).join("one.rs");
    for (root, serial) in [
        (file.to_string_lossy().into_owned(), true),
        (t.path(), false),
    ] {
        let threads = Arc::new(Threads(Mutex::new(Vec::new())));
        let result = search_cancellable(opts(root.clone(), "needle"), threads.clone(), &|| false)
            .expect("search ok");
        assert_eq!(result.files.len(), 1, "{root}");
        assert_eq!(result.files[0].matches[0].line, 1, "{root}");
        let seen = threads.0.lock().expect("lock").clone();
        assert_eq!(seen.len(), 1, "{root}");
        assert_eq!(seen[0] == std::thread::current().id(), serial, "{root}");
    }
}

#[test]
fn deferred_lines_preserve_every_anchor_rank_digest_and_clip_length() {
    let t = TmpDir::new();
    let body = format!(
        "// needle\r\nlet needle = 1;\r\nneedle();\r\nneedle();\r\né😀{}needle\r\n",
        "x".repeat(100_000)
    );
    t.write("dense.rs", &body);
    for (context, only, unique, multiline) in [
        (0, false, false, false),
        (2, false, false, false),
        (0, true, false, false),
        (0, true, true, false),
        (0, false, false, true),
    ] {
        let mut options = opts(t.path(), "needle");
        options.context = crate::types::SearchContext::symmetric(context);
        options.output = match (only, unique) {
            (false, _) => SearchOutput::Lines,
            (true, false) => SearchOutput::Spans(SpanValues::Every),
            (true, true) => SearchOutput::Spans(SpanValues::Counted),
        };
        options.multiline = Some(multiline);
        options.values = SourceValues::Hashed;
        options.max_snippet_chars = Some(40);
        let before = search(options.clone()).expect("owned scan");
        options.values = SourceValues::Deferred;
        let after = search(options).expect("compact scan");
        assert_eq!(before.stats.match_count, after.stats.match_count);
        assert_eq!(before.stats.matched_lines, after.stats.matched_lines);
        for (a, b) in before.files.iter().zip(&after.files) {
            assert_eq!(a.source, b.source);
            assert_eq!(a.matches.len(), b.matches.len());
            for (a, b) in a.matches.iter().zip(&b.matches) {
                assert_eq!(
                    (
                        a.line,
                        a.column,
                        a.byte_offset,
                        a.rank,
                        a.value_digest,
                        a.count,
                        a.original_chars,
                        a.original_bytes
                    ),
                    (
                        b.line,
                        b.column,
                        b.byte_offset,
                        b.rank,
                        b.value_digest,
                        b.count,
                        b.original_chars,
                        b.original_bytes
                    )
                );
                if context == 0 && !only && !multiline {
                    assert_eq!(b.deferred, Some(0));
                    assert_eq!(b.value.capacity(), 0);
                } else {
                    assert_eq!(a.value, b.value);
                    assert_eq!(b.deferred, None);
                }
            }
        }
    }
}

#[test]
fn asymmetric_context_is_assembled_without_the_other_side() {
    let t = TmpDir::new();
    t.write("a.txt", "first\n\nneedle\nafter\nlast\n");
    for (before, after, expected) in [
        (2, 0, "first\n\nneedle"),
        (0, 2, "needle\nafter\nlast"),
        (1, 1, "\nneedle\nafter"),
        (100, 0, "first\n\nneedle"),
        (0, 100, "needle\nafter\nlast"),
    ] {
        let mut o = opts(t.path(), "needle");
        o.context = crate::types::SearchContext { before, after };
        let r = search(o).expect("search");
        assert_eq!(r.files[0].matches[0].value, expected, "{before}/{after}");
    }
}

#[test]
fn asymmetric_context_surrounds_the_whole_multiline_match() {
    let t = TmpDir::new();
    t.write("a.txt", "before\nstart\nend\nafter\nlast\n");
    let mut o = opts(t.path(), "start\nend");
    o.multiline = Some(true);
    o.syntax = PatternSyntax::Regex;
    o.context = crate::types::SearchContext {
        before: 1,
        after: 2,
    };
    let r = search(o).expect("search");
    assert_eq!(
        r.files[0].matches[0].value,
        "before\nstart\nend\nafter\nlast"
    );
}
