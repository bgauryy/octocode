//! `matchString` inside `ranges`: hits, context and blocks stay inside the
//! requested lines.

use super::*;

/// `ranges` with `matchString` is a region grep: only hits inside the
/// ranges count, their windows stay inside them, and ranges wholly past the
/// file end are an error, never a whole-file grep.
#[test]
fn ranges_with_match_string_grep_inside_the_ranges_only() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    let text = (1..=30)
        .map(|n| {
            if n % 10 == 5 {
                format!("hit {n}\n")
            } else {
                format!("l{n}\n")
            }
        })
        .collect::<String>();
    fs::write(&p, text).expect("fixture");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    amend(&mut req, serde_json::json!({"matchString":["hit"]}));
    amend(
        &mut req,
        serde_json::json!({"contextLines":{"before":3,"after":3}}),
    );
    local_fetch_query::set_line_spans(&mut req, &[(10, 20)]).expect("ranges");
    let r = fetch(&req, &paths);
    assert_eq!(r.matched_lines, vec![15], "{r:?}");
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 12, end: 18 }],
        "{r:?}"
    );
    local_fetch_query::set_line_spans(&mut req, &[(90, 100)]).expect("ranges");
    let past = fetch(&req, &paths);
    assert!(
        past.error
            .as_deref()
            .is_some_and(|e| e.contains("past the file end")),
        "{past:?}"
    );
}

/// Ranges that all start past the file end repair to the same read of the
/// file's last window, as wide as the widest request; a range that reaches
/// into the file is served and carries no repair.
#[test]
fn ranges_past_the_file_end_lead_to_the_last_window() {
    let t = Fixture::new();
    let p = t.write("short.txt", "l1\nl2\nl3\nl4\nl5\n");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    local_fetch_query::set_line_spans(&mut req, &[(90, 92), (95, 100)]).expect("ranges");
    let past = fetch(&req, &paths);
    assert!(past.error.is_some(), "{past:?}");
    let repair = past
        .next
        .as_ref()
        .and_then(|next| next.repair.as_ref())
        .expect("repair lead");
    assert_eq!(
        local_fetch_query::line_ranges(&repair.query),
        vec![LineRange { start: 1, end: 5 }],
        "{past:?}"
    );
    let fixed = fetch(&repair.query, &paths);
    assert!(fixed.error.is_none(), "{fixed:?}");
    local_fetch_query::set_line_spans(&mut req, &[(3, 4), (9, 10)]).expect("ranges");
    assert!(fetch(&req, &paths).error.is_none());
    local_fetch_query::set_line_spans(&mut req, &[(9, 10)]).expect("ranges");
    let narrow = fetch(&req, &paths);
    assert_eq!(
        narrow
            .next
            .as_ref()
            .and_then(|next| next.repair.as_ref())
            .map(|repair| local_fetch_query::line_ranges(&repair.query)),
        Some(vec![LineRange { start: 4, end: 5 }]),
        "{narrow:?}"
    );
}

#[test]
fn match_ranges_multiline_hits_must_fit_inside_one_requested_region() {
    let fixture = Fixture::new();
    let path = fixture.write("multiline.txt", "alpha\nbeta\ngamma\n");
    let paths = Paths(fixture.0.clone());
    for dialect in ["literal", "rust", "pcre2"] {
        for context in [0, 3] {
            let query = qj(
                &path,
                serde_json::json!({
                    "matchString": ["alpha\nbeta"], "regexEngine":dialect,
                    "ranges":[{"startLine":1,"endLine":1}], "contextLines":{"before":context,"after":context}
                }),
            );
            let result = fetch(&query, &paths);
            assert_eq!(result.content.as_deref(), Some(""), "{dialect}: {result:?}");
            assert!(result.matched_lines.is_empty(), "{dialect}: {result:?}");
            assert!(
                result.source_line_ranges.is_empty(),
                "{dialect}: {result:?}"
            );
        }
        let query = qj(
            &path,
            serde_json::json!({
                "matchString":["alpha\nbeta", "gamma"], "regexEngine":dialect,
                "ranges":[{"startLine":1,"endLine":1}, {"startLine":3,"endLine":3}], "contextLines":{"before":0,"after":0}
            }),
        );
        let result = fetch(&query, &paths);
        assert_eq!(
            result.content.as_deref(),
            Some("gamma\n"),
            "{dialect}: {result:?}"
        );
        assert_eq!(result.matched_lines, [3]);
        assert_eq!(result.source_line_ranges, [LineRange { start: 3, end: 3 }]);
    }
}

#[test]
fn match_ranges_multiline_hits_keep_exact_region_end_and_explicit_blocks() {
    let fixture = Fixture::new();
    let source = "fn scope() {\n    alpha();\n    beta();\n}\n";
    let path = fixture.write("multiline.rs", source);
    let paths = Paths(fixture.0.clone());
    let mut query = qj(
        &path,
        serde_json::json!({
            "matchString": ["    alpha();\n    beta();\n"],
            "ranges":[{"startLine":2,"endLine":3}], "contextLines":{"before":0,"after":0}
        }),
    );
    let result = fetch(&query, &paths);
    assert_eq!(
        result.content.as_deref(),
        Some("    alpha();\n    beta();\n")
    );
    assert_eq!(result.matched_lines, [2, 3]);
    assert_eq!(result.source_line_ranges, [LineRange { start: 2, end: 3 }]);
    amend(&mut query, serde_json::json!({"block":Some(true)}));
    let expanded = fetch(&query, &paths);
    assert_eq!(expanded.content.as_deref(), Some(source));
    assert_eq!(
        expanded.source_line_ranges,
        [LineRange { start: 1, end: 4 }]
    );
}

#[test]
fn match_ranges_keep_implicit_declaration_expansion_inside_requested_lines() {
    let fixture = Fixture::new();
    let path = fixture.write("scope.rs", "fn scope() {\n    let needle = 1;\n}\n");
    let paths = Paths(fixture.0.clone());
    let mut query = qj(
        &path,
        serde_json::json!({"matchString": ["needle"], "ranges":[{"startLine":2,"endLine":2}], "contextLines":{"before":1,"after":1}}),
    );
    let result = fetch(&query, &paths);
    assert_eq!(result.content.as_deref(), Some("    let needle = 1;\n"));
    assert_eq!(result.source_line_ranges, [LineRange { start: 2, end: 2 }]);
    assert_eq!(result.match_ranges, [LineRange { start: 2, end: 2 }]);

    amend(&mut query, serde_json::json!({"block":Some(true)}));
    let explicit = fetch(&query, &paths);
    assert_eq!(
        explicit.content.as_deref(),
        Some("fn scope() {\n    let needle = 1;\n}\n")
    );
}

#[test]
fn match_ranges_keep_long_line_byte_context_inside_requested_lines() {
    let fixture = Fixture::new();
    let source = format!(
        "outside_before\nneedle{}needle\noutside_after\n",
        "x".repeat(3_000)
    );
    let path = fixture.write("long-line.txt", &source);
    let paths = Paths(fixture.0.clone());
    let query = qj(
        &path,
        serde_json::json!({"matchString": ["needle"], "ranges":[{"startLine":2,"endLine":2}]}),
    );
    let result = fetch(&query, &paths);
    let content = result.content.as_deref().expect("bounded match content");
    assert!(!content.contains("outside_before"), "{content}");
    assert!(!content.contains("outside_after"), "{content}");
    assert_eq!(content.matches("needle").count(), 2);
    assert_eq!(result.source_line_ranges, [LineRange { start: 2, end: 2 }]);
    assert_eq!(result.match_ranges, [LineRange { start: 2, end: 2 }]);
}
