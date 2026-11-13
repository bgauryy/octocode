use super::*;

#[test]
fn multiline_views_count_physical_hit_lines_once() {
    let t = TmpDir::new();
    t.write("hits.txt", "before\na\nb b\nc\na\nb\nafter\n");
    for syntax in [PatternSyntax::Regex, PatternSyntax::Pcre2] {
        for output in [
            SearchOutput::Lines,
            SearchOutput::Spans(SpanValues::Every),
            SearchOutput::CountLines,
            SearchOutput::CountMatches,
            SearchOutput::FilesWithMatch,
        ] {
            let mut options = opts(t.path(), r"a\nb|\bb\b|\bc\b");
            options.syntax = syntax;
            options.multiline = Some(true);
            options.output = output;
            let result = search(options).expect("multiline search");
            assert_eq!(result.stats.matched_lines, Some(5), "{syntax:?} {output:?}");
            if !matches!(output, SearchOutput::CountLines) {
                assert_eq!(result.stats.match_count, Some(4), "{syntax:?} {output:?}");
            }
        }
    }
}

#[test]
fn multiline_spans_point_to_each_match_in_source() {
    for newline in ["\n", "\r\n"] {
        let t = TmpDir::new();
        let text = ["before", "😀a", "é b", "😀c c", "after"].join(newline);
        t.write("hits.txt", &text);
        for syntax in [PatternSyntax::Regex, PatternSyntax::Pcre2] {
            let mut options = opts(t.path(), r"a\r?\né b|c");
            options.syntax = syntax;
            options.multiline = Some(true);
            options.output = SearchOutput::Spans(SpanValues::Every);
            let result = search(options).expect("multiline spans");
            let matches = &result.files[0].matches;
            let coordinates: Vec<_> = matches
                .iter()
                .map(|m| (m.line, m.column, m.byte_offset))
                .collect();
            assert_eq!(coordinates, [(2, 2, 4), (4, 2, 4), (4, 4, 6)]);
            assert_eq!(matches[0].last_line, Some(3));
            for hit in matches {
                let line_start: usize = text
                    .split_inclusive('\n')
                    .take(hit.line as usize - 1)
                    .map(str::len)
                    .sum();
                let start = line_start + hit.byte_offset as usize;
                assert_eq!(&text[start..start + hit.value.len()], hit.value);
            }
        }
    }
}

#[test]
fn multiline_span_preserves_a_matched_terminal_newline() {
    let t = TmpDir::new();
    t.write("hits.txt", "a\nb\n");
    for syntax in [PatternSyntax::Regex, PatternSyntax::Pcre2] {
        let mut options = opts(t.path(), "a\\nb\\n");
        options.syntax = syntax;
        options.multiline = Some(true);
        options.output = SearchOutput::Spans(SpanValues::Every);
        let result = search(options).expect("match including final newline");
        assert_eq!(result.files[0].matches[0].value, "a\nb\n");
        assert_eq!(result.stats.matched_lines, Some(2));
    }
}

/// Multiline mode joins matches on adjacent lines into one searched block;
/// a row is still one match, or the matches that share a line, with its own
/// first and last line: separate one-line hits stay separate rows, and a
/// match over several lines states its last.
#[test]
fn a_multiline_block_is_one_row_per_group_of_matches_sharing_lines() {
    let t = TmpDir::new();
    t.write(
        "hits.txt",
        "x one\nx two\nfold(\n  y x\nx three\nmid\nx four\n",
    );
    for syntax in [PatternSyntax::Regex, PatternSyntax::Pcre2] {
        for context in [
            crate::types::SearchContext::default(),
            crate::types::SearchContext::symmetric(1),
        ] {
            // A newline alternative that never matches still makes the
            // search multiline: the one-line hits stay one row each.
            let mut options = opts(t.path(), r"\bx\b|zz\n");
            options.syntax = syntax;
            options.multiline = Some(true);
            options.context = context;
            let result = search(options).expect("multiline lines");
            let rows: Vec<_> = result.files[0]
                .matches
                .iter()
                .map(|m| (m.line, m.last_line))
                .collect();
            assert_eq!(
                rows,
                [(1, None), (2, None), (4, None), (5, None), (7, None)],
                "{syntax:?} {context:?}"
            );
            assert_eq!(result.stats.match_count, Some(5), "{syntax:?}");
            assert_eq!(result.stats.matched_lines, Some(5), "{syntax:?}");

            // A match over lines 3-4 that shares line 4 with no other: its
            // own row, beside the one-line hits around it.
            let mut options = opts(t.path(), r"fold\(\n.*|\bx\b");
            options.syntax = syntax;
            options.multiline = Some(true);
            options.context = context;
            let result = search(options).expect("multiline lines");
            let matches = &result.files[0].matches;
            let rows: Vec<_> = matches.iter().map(|m| (m.line, m.last_line)).collect();
            assert_eq!(
                rows,
                [(1, None), (2, None), (3, Some(4)), (5, None), (7, None)],
                "{syntax:?} {context:?}"
            );
            assert_eq!(result.stats.match_count, Some(5), "{syntax:?}");
            assert_eq!(result.stats.matched_lines, Some(6), "{syntax:?}");
            if context.is_empty() {
                assert_eq!(matches[2].value, "fold(\n  y x");
                assert_eq!(matches[0].value, "x one");
            }
        }
    }
}

/// A long line's row in a multiline block is the window one-line mode shows,
/// at the hit's own byte, beside its neighbour's own row.
#[test]
fn a_long_line_in_a_multiline_block_keeps_its_window() {
    let t = TmpDir::new();
    let filler = "y".repeat(crate::security::LONG_LINE_BYTES + 1);
    let long = format!("{filler} needle");
    t.write("hits.txt", format!("needle one\n{long}\nzz\n"));
    for context in [
        crate::types::SearchContext::default(),
        crate::types::SearchContext::symmetric(1),
    ] {
        let mut options = opts(t.path(), r"needle|zz\nqq");
        options.syntax = PatternSyntax::Regex;
        options.multiline = Some(true);
        options.context = context;
        let result = search(options).expect("multiline lines");
        let matches = &result.files[0].matches;
        let rows: Vec<_> = matches.iter().map(|m| (m.line, m.last_line)).collect();
        assert_eq!(rows, [(1, None), (2, None)], "{context:?}");
        assert!(matches[1].value.len() < long.len(), "{context:?}: a window");
        assert_eq!(matches[1].byte_offset as usize, filler.len() + 1);
    }
}
