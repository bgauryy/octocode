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

    let mut joined = String::new();
    loop {
        let r = execute_local_fetch(
            &req,
            &paths,
            &Safe,
            &NeverCancel,
            Some(4),
            None,
            &Default::default(),
        );
        assert_eq!(r.status, ReadStatus::Success);
        joined.push_str(
            r.content
                .as_deref()
                .expect("test fixture operation should succeed"),
        );
        let Some(next) = r.next.and_then(|n| n.continue_window) else {
            break;
        };
        req = next.query;
    }
    assert_eq!(joined, "one\ntwo 😀\nthree\n");
}

#[test]
fn regex_line_anchors_match_every_line() {
    let t = Fixture::new();
    let p = t.0.join("Cargo.toml");
    fs::write(&p, "[package]\nversion = \"1\"\n[dep]\nversion = \"2\"\n")
        .expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    amend(
        &mut req,
        serde_json::json!({"matchString":["^version.*\"$"]}),
    );
    amend(
        &mut req,
        serde_json::json!({"regexEngine":Some(octocode_contract_types::RegexEngine::Rust)}),
    );
    amend(
        &mut req,
        serde_json::json!({"contextLines":{"before":0,"after":0}}),
    );
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
    amend(&mut req, serde_json::json!({"matchString":["hit"]}));
    amend(
        &mut req,
        serde_json::json!({"contextLines":{"before":0,"after":0}}),
    );
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
    let (paths, req) = hit_lines(&t, 30, &[3, 25]);
    let expected = fetch(&req, &paths);
    let mut query = req;
    let mut content = String::new();
    let mut lines = Vec::new();
    let mut pages = 0;
    loop {
        let row = execute_local_fetch(
            &query,
            &paths,
            &Safe,
            &NeverCancel,
            Some(56),
            None,
            &Default::default(),
        );
        assert_eq!(row.error_code, None, "{row:?}");
        content.push_str(row.content.as_deref().expect("content"));
        for range in &row.source_line_ranges {
            lines.extend(range.start..=range.end);
        }
        pages += 1;
        assert!(pages < 10);
        let Some(next) = row.next.and_then(|next| next.continue_window) else {
            break;
        };
        query = next.query;
    }
    assert!(pages > 1);
    assert_eq!(Some(content), expected.content);
    assert_eq!(lines, vec![2, 3, 4, 24, 25, 26]);
}

#[test]
fn long_line_matches_preserve_every_byte_and_explicit_context() {
    let fixture = Fixture::new();
    let line = format!(
        "{}needle{}needle{}\n",
        "x".repeat(6000),
        "y".repeat(6000),
        "z".repeat(6000)
    );
    let source = format!("before\n{line}after\n");
    let path = fixture.write("bundle.min.js", &source);
    let paths = Paths(fixture.0.clone());
    for context in [0, 1] {
        let mut request = qj(
            &path,
            serde_json::json!({"matchString":["needle"],"contextLines":{"before":context,"after":context}}),
        );
        let mut joined = String::new();
        loop {
            let out = execute_local_fetch(
                &request,
                &paths,
                &Safe,
                &NeverCancel,
                Some(1000),
                None,
                &Default::default(),
            );
            joined.push_str(out.content.as_deref().expect("content"));
            let Some(next) = out.next.and_then(|next| next.continue_window) else {
                break;
            };
            request = next.query;
        }
        assert_eq!(
            joined,
            if context == 0 {
                line.clone()
            } else {
                source.clone()
            }
        );
    }
}

#[test]
fn multiline_matches_preserve_every_matched_source_line() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "before\nfirst\nsecond\nafter\n").expect("fixture");
    let paths = Paths(t.0.clone());
    for (pattern, is_regex) in [("first\nsecond", false), (r"first\nsecond", true)] {
        let mut req = q(&p);
        amend(&mut req, serde_json::json!({"matchString":[pattern]}));
        amend(
            &mut req,
            serde_json::json!({"regexEngine":is_regex.then_some(octocode_contract_types::RegexEngine::Rust)}),
        );
        amend(
            &mut req,
            serde_json::json!({"contextLines":{"before":0,"after":0}}),
        );
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
    let fixture = Fixture::new();
    let source = numbered(300)
        .replace("\nl10\n", "\nl10 needle\n")
        .replace("\nl250\n", "\nl250 needle\n");
    let path = fixture.write("a.txt", &source);
    let paths = Paths(fixture.0.clone());
    let mut query = qj(
        &path,
        serde_json::json!({"matchString":["needle"],"contextLines":{"before":2,"after":2}}),
    );
    let mut matched = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    loop {
        let out = execute_local_fetch(
            &query,
            &paths,
            &Safe,
            &NeverCancel,
            Some(32),
            None,
            &Default::default(),
        );
        assert_eq!(out.error_code, None);
        for range in &out.source_line_ranges {
            for line in range.start..=range.end {
                assert!(seen.insert(line), "duplicate source line {line}");
            }
        }
        matched.extend(out.matched_lines);
        let Some(next) = out.next.and_then(|next| next.continue_window) else {
            break;
        };
        query = next.query;
    }
    assert_eq!(matched, vec![10, 250]);
    assert_eq!(seen, (8..=12).chain(248..=252).collect());
}
