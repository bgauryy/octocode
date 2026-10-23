//! The windows around `matchString` hits: context, omission markers, byte
//! windows on minified lines, and the ranges each page cites.

use super::*;

#[test]
fn exact_line_and_continuation_union() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "one\ntwo 😀\nthree\n").expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.unit = Some(WindowUnit::Lines);
    req.length = wire_positive(1);
    let mut joined = String::new();
    loop {
        let r = fetch(&req, &paths);
        assert_eq!(r.status, ReadStatus::Success);
        joined.push_str(
            r.content
                .as_deref()
                .expect("test fixture operation should succeed"),
        );
        let Some(next) = r.next.and_then(|n| n.continue_window) else {
            break;
        };
        req = next.query
    }
    assert_eq!(joined, "one\ntwo 😀\nthree\n")
}

#[test]
fn regex_line_anchors_match_every_line() {
    let t = Fixture::new();
    let p = t.0.join("Cargo.toml");
    fs::write(&p, "[package]\nversion = \"1\"\n[dep]\nversion = \"2\"\n")
        .expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String(
        "^version.*\"$".parse().expect("match string"),
    ));
    req.regex = Some(octocode_contract_types::ReadRegex::Rust);
    req.context_lines = Some(0);
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
    assert_eq!(wire["matchedLines"], serde_json::json!([2, 4]));
}

#[test]
fn match_windows_are_separated_by_omission_marker() {
    let t = Fixture::new();
    let (paths, req) = hit_lines(&t, 30, &[3, 25]);
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
    assert_eq!(
        wire["content"],
        "l2\nhit3\nl4\n... [lines 5-23 not requested] ...\nl24\nhit25\nl26\n"
    );
    assert_eq!(
        wire["sourceLineRanges"],
        serde_json::json!([{"line": 2, "endLine": 4},{"line": 24, "endLine": 26}])
    );
    assert_eq!(wire["matchedLines"], serde_json::json!([3, 25]));
}

#[test]
fn adjacent_match_windows_have_no_marker() {
    let t = Fixture::new();
    let (paths, req) = hit_lines(&t, 10, &[3, 5]);
    let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
    assert_eq!(wire["content"], "l2\nhit3\nl4\nhit5\nl6\n");
    assert_eq!(
        wire["sourceLineRanges"],
        serde_json::json!([{"line": 2, "endLine": 6}])
    );
}

#[test]
fn omission_marker_survives_redaction() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(
        &p,
        numbered(10)
            .replace("l2\n", "hit\n")
            .replace("l9\n", "hit SECRET\n"),
    )
    .expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("hit".parse().expect("match string")));
    req.context_lines = Some(0);
    let r = fetch(&req, &paths);
    assert_eq!(
        r.content.as_deref(),
        Some("hit\n... [lines 3-8 not requested] ...\nhit [REDACTED]\n")
    );
    // Redaction keeps line counts, so the anchors stay; the warning says
    // the text is not verbatim.
    assert_eq!(
        r.source_line_ranges,
        vec![
            LineRange { start: 2, end: 2 },
            LineRange { start: 9, end: 9 }
        ]
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("not verbatim")),
        "{:?}",
        r.warnings
    );
}

#[test]
fn paged_match_windows_map_source_ranges_around_marker() {
    let t = Fixture::new();
    let (paths, mut req) = hit_lines(&t, 30, &[3, 25]);
    req.unit = Some(WindowUnit::Lines);
    req.length = wire_positive(4);
    let first = fetch(&req, &paths);
    assert_eq!(
        first.content.as_deref(),
        Some("l2\nhit3\nl4\n... [lines 5-23 not requested] ...\n")
    );
    assert_eq!(
        first.source_line_ranges,
        vec![LineRange { start: 2, end: 4 }]
    );
    let next = first
        .next
        .and_then(|n| n.continue_window)
        .expect("second page")
        .query;
    let second = fetch(&next, &paths);
    assert_eq!(second.content.as_deref(), Some("l24\nhit25\nl26\n"));
    assert_eq!(
        second.source_line_ranges,
        vec![LineRange { start: 24, end: 26 }]
    );
}

#[test]
fn matches_on_minified_lines_default_to_byte_windows() {
    let t = Fixture::new();
    let p = t.0.join("bundle.min.js");
    let filler = "x".repeat(3_000);
    let source = format!("{filler}needle{filler}needle{filler}\nshort needle\n");
    fs::write(&p, &source).expect("fixture");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("needle".parse().expect("match string")));
    let r = fetch(&req, &paths);
    let content = r.content.expect("content");
    assert_eq!(content.matches("needle").count(), 3, "{content:?}");
    assert!(content.len() < 2_000, "{} bytes", content.len());
    for part in content.split('\n').filter(|part| !part.is_empty()) {
        assert!(
            part.starts_with("... [") || source.contains(part),
            "{part:?}"
        );
    }
    // The cut windows are not the whole lines: one read reaches them.
    let next = r.next.clone().expect("next");
    assert_eq!(
        next.whole_lines
            .as_ref()
            .map(|read| read.query.context_lines),
        Some(Some(0)),
        "{next:?}"
    );
    assert!(next.continue_window.is_none(), "{next:?}");
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 1, end: 2 }],
        "anchor kept"
    );
    assert!(
        r.warnings
            .iter()
            .any(|w| w.starts_with("longMatchedLines:")),
        "{:?}",
        r.warnings
    );
    assert!(
        !r.warnings.iter().any(|w| w.contains("next.wholeLines")),
        "the warning does not restate the continuation: {:?}",
        r.warnings
    );
    // An explicit contextLines keeps whole lines.
    req.context_lines = Some(0);
    let r = fetch(&req, &paths);
    assert!(
        r.warnings.iter().all(|w| !w.contains("contextLines")),
        "{:?}",
        r.warnings
    );
}

/// An explicit `contextLines` on a minified line still starts the match
/// window at the hit: neighbouring whole lines cannot be shown without the
/// line's kilobytes, so the window is the hit's bytes and `next.wholeLines`
/// reads the whole line. Only `contextLines:0` asks for the matched line itself,
/// paged from its start.
#[test]
fn an_explicit_context_on_a_minified_line_still_windows_the_hit() {
    let t = Fixture::new();
    let p = t.0.join("bundle.min.js");
    let filler = "x".repeat(30_000);
    fs::write(&p, format!("{filler}needle{filler}\nshort line\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("needle".parse().expect("match string")));
    for context in [1, 5] {
        req.context_lines = Some(context);
        let r = fetch(&req, &paths);
        let content = r.content.as_deref().expect("content");
        assert!(content.len() < 1_000, "{} bytes", content.len());
        assert!(content.contains("needle"), "{content:?}");
        assert!(
            content.starts_with("... [29800 bytes omitted] ...\n"),
            "{content:?}"
        );
        let next = r.next.expect("next");
        assert!(next.whole_lines.is_some(), "{next:?}");
        assert!(next.continue_window.is_none(), "{next:?}");
    }
    req.context_lines = Some(0);
    let whole = fetch(&req, &paths);
    let content = whole.content.as_deref().expect("content");
    assert!(content.starts_with("xxxx"), "the line from its start");
    assert!(!content.contains("omitted"), "{content:.80}");
    assert!(whole.next.expect("next").continue_window.is_some());
}

/// Byte windows (the long-line match view) never fabricate text where
/// windows overlap.
#[test]
fn byte_windows_overlapping_do_not_duplicate_source() {
    let source = "aaa needle bbb\nline2\nline3\nline4\nccc needle ddd\nneedle x needle\n";
    let spans = crate::tools::source_view::extraction::literal_ranges(source, "needle", true);
    let (content, _) = crate::tools::source_view::extraction::byte_windows(source, spans, 4);
    for part in content.split("\n").filter(|part| !part.is_empty()) {
        assert!(
            part.starts_with("... [") || source.contains(part),
            "fabricated text {part:?} in {content:?}"
        );
    }
    assert!(content.contains("bytes omitted"), "{content:?}");
}

#[test]
fn multiline_matches_preserve_every_matched_source_line() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "before\nfirst\nsecond\nafter\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for (pattern, is_regex) in [("first\nsecond", false), (r"first\nsecond", true)] {
        let mut req = q(&p);
        req.match_string = Some(MatchString::String(pattern.parse().expect("match string")));
        req.regex = is_regex.then_some(octocode_contract_types::ReadRegex::Rust);
        req.context_lines = Some(0);
        let wire = serde_json::to_value(fetch(&req, &paths)).expect("serializable");
        assert_eq!(wire["content"], "first\nsecond\n", "{pattern:?}: {wire}");
        assert_eq!(wire["matchedLines"], serde_json::json!([2, 3]));
        assert_eq!(
            wire["sourceLineRanges"],
            serde_json::json!([{"line": 2, "endLine": 3}])
        );
    }
}

/// Each page names only the matched windows and lines it shows: a later
/// page never restates what an earlier page sent.
#[test]
fn match_ranges_cover_only_the_page_that_shows_them() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    let mut text = numbered(300);
    text = text
        .replace("\nl10\n", "\nl10 needle\n")
        .replace("\nl250\n", "\nl250 needle\n");
    fs::write(&p, &text).expect("fixture");
    let paths = Paths(t.0.clone());
    let req = qj(
        &p,
        serde_json::json!({"matchString": "needle", "contextLines": 2, "unit": "lines", "length": 5}),
    );
    let first = fetch(&req, &paths);
    assert_eq!(first.error, None, "{first:?}");
    let wire = serde_json::to_value(&first).expect("serialize");
    // The page's windows are its source lines: nothing to restate.
    assert!(wire.get("matchRanges").is_none(), "{wire}");
    assert_eq!(wire["matchedLines"], serde_json::json!([10]), "{wire}");
    let next = first
        .next
        .and_then(|next| next.continue_window)
        .expect("second page")
        .query;
    let second = serde_json::to_value(fetch(&next, &paths)).expect("serialize");
    assert!(
        !second.to_string().contains("\"start\":8"),
        "page 2 never restates page 1's window: {second}"
    );
    assert!(second.get("matchRanges").is_none(), "{second}");
    assert_eq!(second["matchedLines"], serde_json::json!([250]), "{second}");
}
