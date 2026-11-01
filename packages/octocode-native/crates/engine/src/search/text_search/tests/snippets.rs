//! Complete selected context, spans, source identity and deferred lines.

use super::*;
use crate::types::{SearchContext, TextSearchMatch};

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
fn deferred_lines_preserve_complete_values_and_every_anchor_rank_digest() {
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
                        a.count
                    ),
                    (
                        b.line,
                        b.column,
                        b.byte_offset,
                        b.rank,
                        b.value_digest,
                        b.count
                    )
                );
                if context == 0 && !only && !multiline {
                    assert_eq!(b.deferred, Some(0));
                    assert_eq!(b.value.capacity(), 0);
                    let source_line = body.lines().nth(b.line as usize - 1).expect("source line");
                    let materialized =
                        crate::search::text_parser::line_match(source_line, b.line, b.column);
                    assert_eq!(a.value, materialized.value);
                    assert_eq!(a.value_digest, materialized.value_digest);
                    assert_eq!(a.byte_offset, materialized.byte_offset);
                    assert_eq!(a.rank, materialized.rank);
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

#[test]
fn complete_selected_unicode_values_survive_without_size_reduction() {
    let t = TmpDir::new();
    let line = format!(
        "BEGIN {}needle{} END",
        "é😀".repeat(2_000),
        "界".repeat(2_000)
    );
    t.write("long.txt", format!("{line}\n"));
    let options = opts(t.path(), "needle");
    let result = search(options).expect("search");
    let hit = &result.files[0].matches[0];
    assert_eq!(hit.value, line);
    assert_eq!(hit.value_digest, TextSearchMatch::digest_of(&line));
    assert_eq!(hit.column, ("BEGIN ".len() + 2_000 * 3) as u32);
}

#[test]
fn complete_selected_long_context_keeps_every_requested_line() {
    let t = TmpDir::new();
    let before = "é😀".repeat(1_000);
    let after = "界".repeat(1_000);
    let expected = format!("{before}\n\nneedle\n{after}");
    t.write("context.txt", format!("OUTSIDE\n{expected}\nOUTSIDE\n"));
    let mut options = opts(t.path(), "needle");
    options.context = SearchContext {
        before: 2,
        after: 1,
    };
    let result = search(options).expect("search");
    let hit = &result.files[0].matches[0];
    assert_eq!(hit.value, expected);
    assert_eq!(hit.line, 4);
    assert_eq!(hit.value_digest, TextSearchMatch::digest_of(&expected));
}

#[test]
fn complete_long_spans_preserve_distinct_same_line_offsets() {
    let t = TmpDir::new();
    let first = "é".repeat(3_000);
    let second = "界".repeat(3_000);
    t.write("spans.txt", format!("{first} {second}\n"));
    let mut options = opts(t.path(), "é+|界+");
    options.syntax = PatternSyntax::Regex;
    options.output = SearchOutput::Spans(SpanValues::Every);
    let result = search(options).expect("spans");
    let hits = &result.files[0].matches;
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].value, first);
    assert_eq!(hits[1].value, second);
    assert_eq!(hits[0].byte_offset, 0);
    assert_eq!(hits[1].byte_offset, 6001);
    assert_eq!(hits[1].column, 3001);
}

#[test]
fn deferred_mode_keeps_bom_utf16_sources_eager_and_complete() {
    for big_endian in [false, true] {
        let t = TmpDir::new();
        let line = format!("{}needle{}", "é😀".repeat(1_000), "界".repeat(1_000));
        let body = format!("{line}\r\n");
        let mut bytes = if big_endian {
            vec![0xfe, 0xff]
        } else {
            vec![0xff, 0xfe]
        };
        for unit in body.encode_utf16() {
            bytes.extend(if big_endian {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            });
        }
        std::fs::write(std::path::Path::new(&t.path()).join("bom.txt"), bytes)
            .expect("UTF-16 source");
        let mut options = opts(t.path(), "needle");
        options.values = SourceValues::Hashed;
        let eager = search(options.clone()).expect("eager source");
        options.values = SourceValues::Deferred;
        let deferred = search(options).expect("deferred request");
        assert_eq!(eager.files.len(), 1);
        assert_eq!(deferred.files.len(), 1);
        let hit = &deferred.files[0].matches[0];
        assert_eq!(hit.value, line);
        assert_eq!(hit.deferred, None);
        assert_eq!(hit.value_digest, eager.files[0].matches[0].value_digest);
        assert_eq!(hit.byte_offset, eager.files[0].matches[0].byte_offset);
        assert_eq!(hit.column, 3_000);
        assert_eq!(deferred.files[0].source, eager.files[0].source);
    }
}
