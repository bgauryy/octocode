use super::*;

/// Live PCRE2 workers.
fn active_workers() -> usize {
    ACTIVE_PCRE2_WORKERS.load(std::sync::atomic::Ordering::Relaxed)
}

#[test]
fn finds_lookaround_and_multiline_ranges_in_original_bytes() {
    let text = "let a = 1;\nfn alpha() {\n  beta()\n}\n";
    let ranges = pcre2_find_ranges(r"(?<=fn )alpha", false, text, 100).expect("ranges");
    assert_eq!(ranges, vec![(14, 19)]);
    assert_eq!(&text[14..19], "alpha");
    let multiline =
        pcre2_find_ranges(r"alpha\(\) \{\n\s+beta", false, text, 100).expect("multiline");
    assert_eq!(multiline.len(), 1);
    assert_eq!(&text[multiline[0].0..multiline[0].1], "alpha() {\n  beta");
    // ^ anchors per line.
    assert_eq!(
        pcre2_find_ranges("^fn", false, text, 100).expect("anchor"),
        vec![(11, 13)]
    );
}

#[test]
fn case_and_backreferences_follow_the_flags() {
    let text = "Foo foo FOO abab";
    assert_eq!(
        pcre2_find_ranges("foo", true, text, 100)
            .expect("caseless")
            .len(),
        3
    );
    assert_eq!(
        pcre2_find_ranges("foo", false, text, 100).expect("sensitive"),
        vec![(4, 7)]
    );
    assert_eq!(
        pcre2_find_ranges(r"(ab)\1", false, text, 100).expect("backref"),
        vec![(12, 16)]
    );
    assert_eq!(
        pcre2_find_ranges("o", true, text, 2).expect("capped").len(),
        2
    );
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
    let sink = pcre2_scan_ranges("o", false, "o o o o", &[], StopAfter(3, vec![])).unwrap();
    assert_eq!(sink.1, vec![(0, 1), (2, 3), (4, 5)]);
}

#[test]
fn invalid_patterns_and_deadlines_are_typed() {
    assert!(matches!(
        pcre2_find_ranges("(", false, "x", 10),
        Err(Pcre2RangesError::InvalidPattern(_))
    ));
    let before = active_workers();
    // A zero deadline stops waiting at once; a scan that still beats it
    // is a complete answer, never an invalid pattern.
    let slow = scan_ranges(
        "a",
        false,
        (&"a".repeat(1 << 20), &[]),
        CappedSpans::new(usize::MAX),
        Duration::ZERO,
    )
    .map(CappedSpans::into_spans);
    assert!(
        matches!(&slow, Err(Pcre2RangesError::Unavailable(message)) if message.contains("deadline"))
            || slow.as_ref().is_ok_and(|ranges| ranges.len() == 1 << 20),
        "{:?}",
        slow.as_ref().map(Vec::len)
    );
    // The worker releases its slot when it finishes.
    for _ in 0..200 {
        if active_workers() <= before {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// A match a redaction's own placeholder makes is none, as on the Rust
/// path; a match holding the whole placeholder counts.
#[test]
fn matches_of_written_text_are_not_the_sources() {
    let text = "a [REDACTED-X] b REDACTED ] [REDACTED]";
    let written = [2..14, 28..38];
    let scan = |pattern: &str| {
        pcre2_scan_ranges(pattern, false, text, &written, CappedSpans::new(usize::MAX))
            .map(CappedSpans::into_spans)
            .expect("scan")
    };
    assert_eq!(scan("REDACTED"), vec![(17, 25)]);
    assert_eq!(scan(r"(?<=\[)RED"), Vec::<(usize, usize)>::new());
    assert_eq!(scan(r"\]"), vec![(26, 27)]);
    assert_eq!(
        scan(r"a .* b"),
        pcre2_find_ranges(r"a .* b", false, text, usize::MAX).expect("scan")
    );
}
