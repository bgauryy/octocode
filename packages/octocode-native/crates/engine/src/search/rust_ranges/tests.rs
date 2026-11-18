use super::*;

#[test]
fn finds_multiline_ranges_in_original_bytes() {
    let text = "let a = 1;\nfn alpha() {\n  beta()\n}\n";
    let ranges = rust_find_ranges(r"alpha\(\) \{\n\s+beta", false, text, 100).expect("ranges");
    assert_eq!(ranges.len(), 1);
    assert_eq!(&text[ranges[0].0..ranges[0].1], "alpha() {\n  beta");
    // ^ anchors per line.
    assert_eq!(
        rust_find_ranges("^fn", false, text, 100).expect("anchor"),
        vec![(11, 13)]
    );
    // $ also anchors before CRLF.
    assert_eq!(
        rust_find_ranges("a$", false, "a\r\nb", 100).expect("crlf"),
        vec![(0, 1)]
    );
}

#[test]
fn case_and_limit_follow_the_arguments() {
    let text = "Foo foo FOO";
    assert_eq!(rust_find_ranges("foo", true, text, 100).unwrap().len(), 3);
    assert_eq!(
        rust_find_ranges("foo", false, text, 100).unwrap(),
        vec![(4, 7)]
    );
    assert_eq!(rust_find_ranges("o", true, text, 2).unwrap().len(), 2);
    // The caller passes one past its limit to tell a capped scan apart.
    assert_eq!(rust_find_ranges("o", true, text, 0).unwrap().len(), 0);
}

#[test]
fn a_scan_offers_matches_until_its_sink_stops_it() {
    struct StopAfter(usize, Vec<(usize, usize)>);
    impl SpanSink for StopAfter {
        fn accept(&mut self, span: (usize, usize)) -> bool {
            self.1.push(span);
            self.1.len() < self.0
        }
    }
    let sink = rust_scan_ranges("o", false, "o o o o", StopAfter(3, vec![])).unwrap();
    assert_eq!(sink.1, vec![(0, 1), (2, 3), (4, 5)]);
    assert!(rust_scan_ranges("(", false, "x", StopAfter(1, vec![])).is_err());
}

#[test]
fn the_verdict_never_depends_on_the_text_size() {
    let big = "ab\n".repeat(200_000);
    assert_eq!(rust_find_ranges("ab", false, &big, 5).unwrap().len(), 5);
    assert!(rust_find_ranges("(ab", false, &big, 5).is_err());
}

#[test]
fn backreferences_and_lookaround_are_invalid_for_this_engine() {
    // Same verdict and reason `localSearch` gives (`regex:"pcre2"` owns both).
    assert_eq!(
        rust_find_ranges(r"(\w)\1", false, "aa", 10),
        Err(RustRangesError::InvalidPattern(
            "backreferences are not supported".into()
        ))
    );
    let lookaround = rust_find_ranges("(?<=fn )x", false, "fn x", 10);
    assert!(
        matches!(&lookaround, Err(RustRangesError::InvalidPattern(reason)) if reason.contains("look-around")),
        "{lookaround:?}"
    );
    assert_eq!(
        rust_find_ranges("(", false, "x", 10),
        Err(RustRangesError::InvalidPattern("unclosed group".into()))
    );
}

#[test]
fn matches_the_reason_localsearch_reports() {
    for pattern in [r"(\w)\1", "(", "[", "a{2,1}"] {
        let checked = crate::search::text_pattern::validate_text_pattern(pattern, false, false);
        let Err(RustRangesError::InvalidPattern(reason)) = rust_find_ranges(pattern, false, "x", 1)
        else {
            panic!("{pattern} must not compile");
        };
        assert_eq!(
            reason,
            parse_reason(&checked.error.expect("validator error")),
            "{pattern}"
        );
    }
}
