//! Only-matching spans: one per submatch, unique and counted values, lossy columns.

use super::*;

/// The bytes a search holding `spans` "needle" spans of `file` keeps, as
/// the engine charges them.
fn held_by_spans(file: &str, spans: usize) -> usize {
    use crate::search::text_search::state::record_weight;
    spans * (std::mem::size_of::<crate::types::TextSearchMatch>() + "needle".len())
        + record_weight(file, &[])
}

/// A file keeps every span, as line mode keeps every line, up to the bytes
/// the search may hold: no count per file bounds it, and one byte past the
/// bound refuses the search instead of cutting it.
#[test]
fn span_collection_keeps_every_span_its_held_bound_admits() {
    let t = TmpDir::new();
    let spans = 1_000;
    t.write("many.txt", "needle ".repeat(spans));
    let file = format!("{}/many.txt", t.path());
    let bound = held_by_spans(&file, spans);
    let mut options = opts(t.path(), "needle");
    options.output = SearchOutput::Spans(SpanValues::Every);
    options.max_held_bytes = Some(bound - 1);
    let refused = search(options.clone()).expect_err("one byte past the bound");
    assert_eq!(
        *refused.kind(),
        crate::error::ErrorKind::ResourceExhausted,
        "{refused}"
    );
    options.max_held_bytes = Some(bound);
    let result = search(options).expect("search ok");
    assert_eq!(result.stats.match_count, Some(spans as u32));
    assert_eq!(result.stats.capped, Some(false));
    assert!(result.stats.cap_reasons.is_empty());
    let matches = &result.files[0].matches;
    assert_eq!(matches.len(), spans);
    let offsets = matches
        .iter()
        .map(|m| m.byte_offset as usize)
        .collect::<Vec<_>>();
    assert_eq!(offsets, (0..spans).map(|n| n * 7).collect::<Vec<_>>());
}

/// Spans inside one line past [`crate::security::LONG_LINE_BYTES`] are
/// collected by the long-line path, and held as every other span is: one
/// byte past the bound refuses the search instead of cutting it.
#[test]
fn long_line_spans_are_held_against_the_bound() {
    let t = TmpDir::new();
    let spans = 100;
    let line = format!(
        "{}{}\n",
        "x".repeat(crate::security::LONG_LINE_BYTES),
        " needle".repeat(spans)
    );
    t.write("long.txt", line);
    let file = format!("{}/long.txt", t.path());
    let bound = held_by_spans(&file, spans);
    let mut options = opts(t.path(), "needle");
    options.output = SearchOutput::Spans(SpanValues::Every);
    options.max_held_bytes = Some(bound - 1);
    let refused = search(options.clone()).expect_err("one byte past the bound");
    assert_eq!(
        *refused.kind(),
        crate::error::ErrorKind::ResourceExhausted,
        "{refused}"
    );
    options.max_held_bytes = Some(bound);
    let result = search(options).expect("search ok");
    assert_eq!(result.files[0].matches.len(), spans);
    assert!(result.files[0].matches.iter().all(|m| m.value == "needle"));
}

/// A search past its held bound stops and is refused on every engine and
/// view that keeps rows: lines, spans and their context lines, never a
/// partial result that reads as complete.
#[test]
fn a_search_past_its_held_bound_is_refused_not_cut() {
    let t = TmpDir::new();
    let lines = 1_000;
    t.write("a.txt", "needle\n".repeat(lines));
    t.write("b.txt", "needle\n".repeat(lines));
    let file = format!("{}/a.txt", t.path());
    // Less than one file's spans: no search that keeps rows fits.
    let bound = held_by_spans(&file, lines) / 2;
    for syntax in [PatternSyntax::Literal, PatternSyntax::Pcre2] {
        for output in [SearchOutput::Lines, SearchOutput::Spans(SpanValues::Every)] {
            let mut options = opts(t.path(), "needle");
            options.syntax = syntax;
            options.output = output;
            options.max_held_bytes = Some(bound);
            let refused = search(options.clone()).expect_err("past the bound");
            assert_eq!(
                *refused.kind(),
                crate::error::ErrorKind::ResourceExhausted,
                "{syntax:?} {output:?}: {refused}"
            );
            // Counting keeps no row: the same scope is counted whole.
            options.output = SearchOutput::CountMatches;
            let counted = search(options).expect("a count fits");
            assert_eq!(counted.stats.match_count, Some(2 * lines as u32));
        }
    }
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

/// Review M6: only-matching columns on one long line are computed with a
/// left-to-right cursor, not a rescan of the line prefix per span. 1000 spans
/// at the end of a ~4 MB multibyte line rescanned ~12 GB before the fix: the
/// cursor steps over the line once, whatever the machine's load.
#[test]
fn only_matching_long_line_columns_are_linear_and_exact() {
    let t = TmpDir::new();
    let prefix = "é".repeat(2_000_000);
    let mut line = prefix.clone();
    for _ in 0..1000 {
        line.push_str("😀x");
    }
    line.push('\n');
    let line_bytes = line.len();
    // A file root is searched on this thread, where the cursor's count is kept.
    let file = t.write("long.txt", line);
    let mut o = opts(file.to_string_lossy().into_owned(), "x");
    o.output = SearchOutput::Spans(SpanValues::Every);
    let before = crate::search::text_search::sink::stepped();
    let r = search(o).expect("ok");
    let stepped = crate::search::text_search::sink::stepped() - before;
    let matches = &r.files[0].matches;
    assert_eq!(matches.len(), 1000);
    for (i, m) in matches.iter().enumerate() {
        // 'é' is one UTF-16 unit, '😀' is two.
        assert_eq!(m.column as usize, 2_000_000 + 3 * i + 2);
        assert_eq!(m.value, "x");
    }
    assert!(
        stepped <= line_bytes,
        "the span cursor stepped over {stepped} bytes of a {line_bytes}-byte line"
    );
}

/// A line that is not UTF-8 decodes as every reader decodes it
/// (`text::Decoded::interior`): 8-bit text without a valid non-ASCII UTF-8
/// sequence reads as Latin-1, in matched values, spans and context lines,
/// with columns unchanged. The file is named in `not_utf8_files`, so the
/// caller can say its values are decoded, not verbatim bytes.
#[test]
fn non_utf8_lines_decode_like_every_reader_and_name_their_file() {
    let t = TmpDir::new();
    fs::write(t.0.join("latin.txt"), b"na\xefve\ncaf\xe9 foo bar\n").expect("write");
    t.write("plain.txt", "foo\n");
    let mut o = opts(t.path(), "foo");
    o.context = crate::types::SearchContext {
        before: 1,
        after: 0,
    };
    let r = search(o).expect("ok");
    let latin = r
        .files
        .iter()
        .find(|file| file.path.ends_with("latin.txt"))
        .expect("latin file");
    assert!(latin.matches[0].value.contains("café foo bar"), "{latin:?}");
    assert!(latin.matches[0].value.contains("naïve"), "{latin:?}");
    assert!(!latin.matches[0].value.contains('\u{fffd}'), "{latin:?}");
    let mut o = opts(t.path(), "foo");
    o.output = SearchOutput::Spans(SpanValues::Every);
    let r = search(o).expect("ok");
    let latin = r
        .files
        .iter()
        .find(|file| file.path.ends_with("latin.txt"))
        .expect("latin");
    assert_eq!(
        (latin.matches[0].value.as_str(), latin.matches[0].column),
        ("foo", 5)
    );
    let named = r.stats.not_utf8_files.clone().unwrap_or_default();
    assert_eq!(named.len(), 1, "{named:?}");
    assert!(named[0].ends_with("latin.txt"), "{named:?}");
}

/// D6 (bin16 retest): empty matches follow rg's rule. A line's matches end
/// with the line (an empty one at its end included); an empty match after
/// its line break starts a line the block does not hold and is no match, so
/// no row sits past the end of the file and no count holds one.
#[test]
fn empty_matches_end_with_their_line() {
    let t = TmpDir::new();
    t.write("clean.txt", "nothing secret here\njust words\n");
    let spans = |pattern: &str, output: SearchOutput| {
        let mut options = opts(t.path(), pattern);
        options.output = output;
        search(options).expect("search ok")
    };
    let result = spans("^.*$", SearchOutput::Spans(SpanValues::Every));
    let rows: Vec<(u32, String)> = result.files[0]
        .matches
        .iter()
        .map(|m| (m.line, m.value.clone()))
        .collect();
    assert_eq!(
        rows,
        [
            (1, "nothing secret here".to_owned()),
            (2, "just words".to_owned())
        ]
    );
    assert_eq!(result.stats.match_count, Some(2));
    // `x*` matches empty at each of a line's characters and at its end:
    // 20 on a 19-character line, 11 on a 10-character one, as rg -o counts.
    let result = spans("x*", SearchOutput::Spans(SpanValues::Every));
    assert_eq!(result.files[0].matches.len(), 31);
    assert!(result.files[0].matches.iter().all(|m| m.line <= 2));
    assert_eq!(result.stats.match_count, Some(31));
    let counted = spans("x*", SearchOutput::CountMatches);
    assert_eq!(counted.stats.match_count, Some(31));
}

/// Every row states the bytes of its line its first submatch covers: from
/// `byte_offset`, `match_len` of them are the text the pattern matched
/// first on that line, in every view (whole lines, lines with context,
/// spans, deferred values) and past a multi-byte character.
#[test]
fn every_row_states_the_bytes_of_its_line_its_first_match_covers() {
    let t = TmpDir::new();
    let body = "pub fn alpha() {}\n  é pub(crate) fn beta_two(x: u8) {}\nlet fn_ptr = 1;\nfn gamma() {} fn delta() {}\n";
    t.write("a.rs", body);
    let pattern = r"fn\s+\w+";
    let first = regex::Regex::new(pattern).expect("pattern");
    for (context, output, values) in [
        (0, SearchOutput::Lines, SourceValues::Hashed),
        (1, SearchOutput::Lines, SourceValues::Hashed),
        (0, SearchOutput::Lines, SourceValues::Deferred),
        (
            0,
            SearchOutput::Spans(SpanValues::Every),
            SourceValues::Hashed,
        ),
    ] {
        let mut options = opts(t.path(), pattern);
        options.context = crate::types::SearchContext::symmetric(context);
        let spans = matches!(output, SearchOutput::Spans(_));
        options.output = output;
        options.values = values;
        let result = search(options).expect("search");
        let rows = &result.files[0].matches;
        assert!(!rows.is_empty());
        for row in rows {
            let line = body.lines().nth(row.line as usize - 1).expect("line");
            let from = row.byte_offset as usize;
            let covered = &line[from..from + row.match_len as usize];
            if spans {
                // A span row covers its own text, wherever it sits.
                assert_eq!(covered, row.value, "context {context}");
            } else {
                let found = first.find(line).expect("a matched line");
                assert_eq!(covered, found.as_str(), "context {context}: {line}");
                assert_eq!(from, found.start(), "context {context}: {line}");
            }
        }
    }
}
