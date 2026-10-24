//! Ranges and blocks read through enclosing declarations, and how a cut
//! declaration continues.

use super::*;

#[test]
fn ranges_read_several_windows_in_one_row() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, numbered(30)).expect("fixture");
    let paths = Paths(t.0.clone());
    // Unordered, overlapping, and past-the-end ranges merge and clamp.
    let req = qj(
        &p,
        serde_json::json!({"ranges":[{"startLine":20,"endLine":21}, {"startLine":2,"endLine":3}, {"startLine":3,"endLine":4}, {"startLine":29,"endLine":40}]}),
    );
    let r = fetch(&req, &paths);
    assert_eq!(r.error, None, "{r:?}");
    assert_eq!(
        r.content.as_deref(),
        Some(
            "l2\nl3\nl4\n... [lines 5-19 not requested] ...\nl20\nl21\n... [lines 22-28 not requested] ...\nl29\nl30\n"
        )
    );
    assert_eq!(
        r.source_line_ranges,
        vec![
            LineRange { start: 2, end: 4 },
            LineRange { start: 20, end: 21 },
            LineRange { start: 29, end: 30 }
        ]
    );
    assert!(
        r.warnings.iter().any(|w| w.contains("29-40")),
        "{:?}",
        r.warnings
    );
    // One range reads that span.
    let old = qj(
        &p,
        serde_json::json!({"ranges":[{"startLine":2,"endLine":3}]}),
    );
    assert_eq!(fetch(&old, &paths).content.as_deref(), Some("l2\nl3\n"));
    // Ranges wholly past the end are an error, not an empty success.
    let past = qj(
        &p,
        serde_json::json!({"ranges":[{"startLine":40,"endLine":41}]}),
    );
    assert!(fetch(&past, &paths).error.is_some());
    // Ordering is checked before execution, never repaired into another selection.
    assert!(
        crate::contracts::validate_query(
            "localFetch",
            serde_json::json!({
                "path":p,"ranges":[{"startLine":3,"endLine":2}]
            })
        )
        .is_err()
    );
}

#[test]
fn block_reads_through_the_enclosing_declaration() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    fs::write(
            &p,
            "import os\n\ndef first(a):\n    x = 1\n    return a\n\n\ndef second(b):\n    if b:\n        return 1\n    return 2\n",
        )
        .expect("fixture");
    let paths = Paths(t.0.clone());
    let ranged = qj(
        &p,
        serde_json::json!({"ranges":[{"startLine":8,"endLine":9}], "block": true}),
    );
    let r = fetch(&ranged, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 8, end: 11 }],
        "{r:?}"
    );
    let matched = qj(
        &p,
        serde_json::json!({"matchString": ["return a", "return 1"], "contextLines":{"before":0,"after":0}, "block": true}),
    );
    let r = fetch(&matched, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![
            LineRange { start: 3, end: 5 },
            LineRange { start: 8, end: 11 }
        ],
        "{r:?}"
    );
    assert_eq!(r.matched_lines, vec![5, 10]);
}

/// Context is exact; declaration expansion requires block:true.
#[test]
fn match_context_is_exact_and_blocks_are_explicit() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    let small: String = (2..=10).map(|i| format!("    s{i} = {i}\n")).collect();
    let big: String = (13..=52).map(|i| format!("    b{i} = {i}\n")).collect();
    // small(): lines 1-10, needle at 3; big(): lines 12-52, needle at 14.
    fs::write(
        &p,
        format!(
            "def small():\n{}\ndef big():\n{}",
            small.replace("s3 = 3", "needle_small = 3"),
            big.replace("b14 = 14", "needle_big = 14")
        ),
    )
    .expect("fixture");
    let paths = Paths(t.0.clone());
    let short = qj(&p, serde_json::json!({"matchString": ["needle_small"]}));
    let r = fetch(&short, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 3, end: 3 }],
        "{r:?}"
    );
    assert_eq!(r.matched_lines, vec![3]);
    assert!(r.next.is_none(), "{r:?}");
    // The caller's own window is kept exactly.
    let exact_two = qj(
        &p,
        serde_json::json!({"matchString": ["needle_small"], "contextLines":{"before":2,"after":2}}),
    );
    let r = fetch(&exact_two, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 1, end: 5 }],
        "{r:?}"
    );
    let long = qj(
        &p,
        serde_json::json!({"matchString": ["needle_big"], "contextLines":{"before":2,"after":2}}),
    );
    let r = fetch(&long, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 12, end: 16 }],
        "{r:?}"
    );
    // The caller chose this window: nothing of the result is unread.
    assert_eq!(r.is_partial, None, "{r:?}");
    assert!(r.next.is_none(), "no reread lead: {r:?}");
    // `block` reads the declaration the window cut.
    let block = qj(
        &p,
        serde_json::json!({"matchString": ["needle_big"], "contextLines":{"before":2,"after":2}, "block": true}),
    );
    let r = fetch(&block, &paths);
    assert_eq!(r.source_line_ranges, vec![LineRange { start: 12, end: 52 }]);
    // Exact-line reads ask for no more.
    let exact = qj(
        &p,
        serde_json::json!({"matchString": ["needle_small"], "contextLines":{"before":0,"after":0}}),
    );
    let r = fetch(&exact, &paths);
    assert_eq!(r.source_line_ranges, vec![LineRange { start: 3, end: 3 }]);
}

/// A `ranges` read returns exactly its lines, wherever they cut a
/// declaration, with no automatic reread lead.
#[test]
fn a_cut_ranges_read_offers_no_reread_lead() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    let body: String = (2..=40).map(|i| format!("    x{i} = {i}\n")).collect();
    fs::write(&p, format!("def big():\n{body}    return 0\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    for ranges in [
        serde_json::json!([{"startLine":1,"endLine":10}]),
        serde_json::json!([{"startLine":10,"endLine":20}]),
        serde_json::json!([{"startLine":3,"endLine":5},{"startLine":30,"endLine":33}]),
    ] {
        let r = fetch(&qj(&p, serde_json::json!({ "ranges": ranges })), &paths);
        assert!(r.next.is_none(), "{ranges:?}: {r:?}");
    }
    // A read that covers its declaration offers nothing more.
    let whole = qj(
        &p,
        serde_json::json!({"ranges":[{"startLine":1,"endLine":41}]}),
    );
    let r = fetch(&whole, &paths);
    assert!(r.next.is_none(), "{r:?}");
}

/// A match in a large source keeps its plain context windows: inlining a
/// cut declaration's few lines never costs a whole-file declarations parse.
#[test]
fn a_large_source_match_does_not_parse_to_inline_a_cut_declaration() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    let small: String = (2..=10).map(|i| format!("    s{i} = {i}\n")).collect();
    let filler = "# filler line that only pads the module out\n".repeat(7_000);
    fs::write(
        &p,
        format!(
            "def small():\n{}{filler}",
            small.replace("s3 = 3", "needle_small = 3")
        ),
    )
    .expect("fixture");
    let r = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": ["needle_small"], "contextLines":{"before":2,"after":2}}),
        ),
        &Paths(t.0.clone()),
    );
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 1, end: 5 }],
        "{r:?}"
    );
}

#[test]
fn an_oversized_block_continues_through_the_declaration_end() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    let body: String = (0..450).map(|i| format!("    x{i} = {i}\n")).collect();
    // def at line 1, 450 body lines (2-451), return at 452.
    fs::write(&p, format!("def big():\n{body}    return 0\n")).expect("fixture");
    let paths = Paths(t.0.clone());
    let ranged = qj(
        &p,
        serde_json::json!({"ranges":[{"startLine":10,"endLine":12}], "block": true}),
    );
    let r = fetch(&ranged, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange {
            start: 10,
            end: 409
        }]
    );
    let rest = r
        .next
        .and_then(|next| next.continue_block)
        .expect("next.continueBlock");
    assert_eq!(
        local_fetch_query::line_ranges(&rest.query),
        vec![LineRange {
            start: 410,
            end: 452
        }],
        "{rest:?}"
    );
    assert!(
        !local_fetch_query::block(&rest.query),
        "a block re-read would widen back: {rest:?}"
    );
    let r = fetch(&rest.query, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange {
            start: 410,
            end: 452
        }]
    );
    // A match inside an oversized requested declaration keeps its context
    // window; the declaration's unread rest is a page of this read.
    let matched = qj(
        &p,
        serde_json::json!({"matchString": ["x200 = 200"], "contextLines":{"before":1,"after":1}, "block": true}),
    );
    let r = fetch(&matched, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange {
            start: 201,
            end: 203
        }]
    );
    let whole = r
        .next
        .and_then(|next| next.continue_block)
        .expect("next.continueBlock");
    assert_eq!(
        local_fetch_query::line_ranges(&whole.query),
        vec![
            LineRange { start: 1, end: 200 },
            LineRange {
                start: 204,
                end: 452
            }
        ],
        "{whole:?}"
    );
    assert!(
        !local_fetch_query::has_matches(&whole.query) && !local_fetch_query::block(&whole.query)
    );
}

/// With a declaration hit present, a call-site hit keeps its window
/// instead of widening to its caller; the widened declaration is named.
#[test]
fn block_reports_widened_declarations() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    fs::write(&p, PY_TWO).expect("fixture");
    let result = fetch(
        &qj(
            &p,
            serde_json::json!({"matchString": ["first"], "block": true, "contextLines":{"before":0,"after":0}}),
        ),
        &Paths(t.0.clone()),
    );
    let body = serde_json::to_value(&result).expect("json");
    assert_eq!(
        body["blocks"],
        serde_json::json!([{"symbolName": "first", "line": 3, "endLine": 5}]),
        "{body}"
    );
    let content = result.content.expect("content");
    // The call on line 10 shows alone, not `second` (8-11) whole.
    assert!(content.contains("def first(a):"), "{content}");
    assert!(!content.contains("def second(b):"), "{content}");
    assert!(content.contains("return first(1)"), "{content}");
}
