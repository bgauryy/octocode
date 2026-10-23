//! Matcher options, columns, path views, and the per-line work a mode does.

use super::*;

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
fn empty_result_when_no_match() {
    let t = TmpDir::new();
    t.write("a.txt", "nothing here\n");
    let r = search(opts(t.path(), "absent")).expect("ok");
    assert!(r.files.is_empty());
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
