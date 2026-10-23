//! Context and snippets: assembly, clipping around the match, deferred lines.

use super::*;

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
