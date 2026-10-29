//! Only-matching spans: one per submatch, unique and counted values, lossy columns.

use super::*;

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
