//! Redacted secrets: anchors, counts, and the security limit.

use super::*;

#[test]
fn redacted_chunk_and_range_reads_keep_their_source_anchor() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, numbered(30).replace("l12\n", "l12 SECRET\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    let mut chunk = q(&p);
    local_fetch_query::set_line_span(&mut chunk, 11, 15).expect("range");
    let mut range = q(&p);
    local_fetch_query::set_line_span(&mut range, 11, 13).expect("range");
    for (req, expected) in [(chunk, (11, 15)), (range, (11, 13))] {
        let r = fetch(&req, &paths);
        assert!(
            r.content
                .as_deref()
                .is_some_and(|c| c.contains("[REDACTED]")),
            "{r:?}"
        );
        assert_eq!(
            r.source_line_ranges,
            vec![LineRange {
                start: expected.0,
                end: expected.1
            }]
        );
        assert!(
            r.warnings.iter().any(|w| w.contains("not verbatim")),
            "{:?}",
            r.warnings
        );
    }
}

/// matchString runs on redacted text, so a match inside a redacted value is
/// not selected. When other lines do match, the row says how many redacted
/// lines lie outside the result instead of dropping them silently.
#[test]
fn a_partial_match_names_the_redacted_lines_it_could_not_see() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "CRE one\nvalue SECRET\nlast\n").expect("fixture");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    amend(&mut req, serde_json::json!({"matchString":["CRE"]}));
    amend(
        &mut req,
        serde_json::json!({"contextLines":{"before":0,"after":0}}),
    );
    let r = fetch(&req, &paths);
    assert_eq!(r.content.as_deref(), Some("CRE one\n"));
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("1 redacted line") && w.contains("matchString")),
        "{:?}",
        r.warnings
    );
}

/// A redacted line the result shows as context is not "outside" it, and
/// the count, the same on every page, is stated by the first page only.
#[test]
fn redacted_context_lines_are_not_counted_outside_the_result() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    let mut source = String::from("CRE one\nvalue SECRET\n");
    source.push_str(&"filler\n".repeat(10));
    source.push_str("other SECRET\n");
    source.push_str(&"filler\n".repeat(3));
    source.push_str("CRE two\n");
    fs::write(&p, &source).expect("fixture");
    let paths = Paths(t.0.clone());
    let outside = |result: &LocalFetchResult| {
        result
            .warnings
            .iter()
            .filter(|w| w.contains("outside this result"))
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut req = q(&p);
    amend(&mut req, serde_json::json!({"matchString":["CRE"]}));
    // Line 2 is context of the first hit; line 13 lies outside the result.
    amend(
        &mut req,
        serde_json::json!({"contextLines":{"before":1,"after":1}}),
    );
    let first = fetch(&req, &paths);
    assert_eq!(
        outside(&first),
        vec!["1 redacted line outside this result: matchString cannot see text behind a [REDACTED…] placeholder; a plain read shows where they are.".to_owned()],
        "{:?}",
        first.warnings
    );
    // Both redacted lines shown as context: nothing lies outside.
    amend(
        &mut req,
        serde_json::json!({"contextLines":{"before":4,"after":4}}),
    );
    assert_eq!(outside(&fetch(&req, &paths)), Vec::<String>::new());
    // A later page of the same read does not repeat the count.
    amend(
        &mut req,
        serde_json::json!({"contextLines":{"before":1,"after":1}}),
    );
    let page = execute_local_fetch(&req, &paths, &Safe, &NeverCancel, Some(26), None);
    assert_eq!(outside(&page).len(), 1, "{:?}", page.warnings);
    let next = page
        .next
        .and_then(|next| next.continue_window)
        .expect("a second page")
        .query;
    let second = fetch(&next, &paths);
    assert_eq!(
        outside(&second),
        Vec::<String>::new(),
        "{:?}",
        second.warnings
    );
}

#[test]
fn ranges_matches_redaction_and_binary() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "zero\nneedle SECRET\nlast\n").expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    amend(&mut req, serde_json::json!({"matchString":["needle"]}));
    amend(
        &mut req,
        serde_json::json!({"contextLines":{"before":0,"after":0}}),
    );
    let r = fetch(&req, &paths);
    let expected_hash = hex::encode(sha2::Sha256::digest(b"zero\nneedle SECRET\nlast\n"));
    assert_eq!(r.source_sha256.as_deref(), Some(expected_hash.as_str()));
    assert_eq!(r.content.as_deref(), Some("needle [REDACTED]\n"));
    assert_eq!(r.match_ranges, vec![LineRange { start: 2, end: 2 }]);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 2, end: 2 }],
        "line-preserving redaction keeps the source anchor"
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("not verbatim")),
        "{:?}",
        r.warnings
    );
    let bin = t.0.join("b.bin");
    fs::write(&bin, [0, 1, 2]).expect("test fixture operation should succeed");
    assert_eq!(
        fetch(&q(&bin), &paths).error_code,
        Some(ErrorCode::BinaryFileUnsupported)
    );
}

/// Only placeholders this read introduced count: literal `[REDACTED…]` text
/// in the source is verbatim, even in a file with real redactions.
#[test]
fn literal_placeholder_text_is_not_counted_as_a_redaction() {
    let t = Fixture::new();
    let p = t.0.join("a.rs");
    let mut source = String::from("let shown = \"[REDACTED-literal]\";\n");
    source.push_str(&"let filler = 1;\n".repeat(2_000));
    source.push_str("let key = \"SECRET\";\n");
    fs::write(&p, &source).expect("fixture");
    let paths = Paths(t.0.clone());
    let redacted = |warnings: &[String]| {
        warnings
            .iter()
            .find(|warning| warning.starts_with("redactedContent"))
            .cloned()
    };
    let head = fetch(
        &qj(
            &p,
            serde_json::json!({"ranges":[{"startLine":1,"endLine":3}]}),
        ),
        &paths,
    );
    assert_eq!(redacted(&head.warnings), None, "{:?}", head.warnings);
    let both = fetch(
        &qj(
            &p,
            serde_json::json!({"ranges":[{"startLine":1,"endLine":1}, {"startLine":2002,"endLine":2002}]}),
        ),
        &paths,
    );
    let warning = redacted(&both.warnings).expect("the real redaction is named");
    assert!(warning.contains("redactedContent: 1 "), "{warning}");
    // A match read redacts the whole source first: its literal line is no
    // unmatchable redacted line, the real one is.
    let matched = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": ["shown"], "contextLines":{"before":0,"after":0}}),
        ),
        &paths,
    );
    assert_eq!(redacted(&matched.warnings), None, "{:?}", matched.warnings);
    assert!(
        matched
            .warnings
            .iter()
            .any(|warning| warning.starts_with("1 redacted line outside")),
        "{:?}",
        matched.warnings
    );
}

#[test]
fn security_limit_is_terminal_but_offers_a_bounded_line_read() {
    struct Limited;
    impl ContentScan for Limited {
        fn sanitize(
            &self,
            _: &str,
            _: &Path,
        ) -> Result<(String, Vec<String>), (ErrorCode, String)> {
            Err((ErrorCode::ContentSecurityLimit, "limited".into()))
        }
    }
    let t = Fixture::new();
    let p = t.0.join("large.txt");
    fs::write(&p, "one\ntwo\n").expect("security fixture should be written");
    let result = execute_local_fetch(
        &q(&p),
        &Paths(t.0.clone()),
        &Limited,
        &NeverCancel,
        None,
        None,
    );
    assert_eq!(result.error_code, Some(ErrorCode::ContentSecurityLimit));
    assert_eq!(result.terminal_limit, Some(true));
    assert!(
        result
            .next
            .and_then(|next| next.read_bounded_lines)
            .is_some()
    );
}
