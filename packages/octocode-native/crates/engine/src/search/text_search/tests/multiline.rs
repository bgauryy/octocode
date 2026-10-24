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
