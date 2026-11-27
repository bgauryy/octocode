//! Ranges and blocks read through enclosing declarations, and how a cut
//! declaration continues.

use super::*;
use octocode_test_support::json_fixture;

#[test]
fn ranges_read_several_windows_in_one_row() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, numbered(30)).expect("fixture");
    let paths = Paths(t.0.clone());
    // Unordered, overlapping, and past-the-end ranges merge and clamp.
    let req = qj(
        &p,
        json_fixture(
            r#"{
                "ranges": [
                    {"startLine": 20, "endLine": 21},
                    {"startLine": 2, "endLine": 3},
                    {"startLine": 3, "endLine": 4},
                    {"startLine": 29, "endLine": 40}
                ]
            }"#,
        ),
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
    // The page shows the file's last line and states `totalLines`: the cut
    // of range 29-40 needs no warning beside them.
    assert_eq!(r.total_lines, Some(30), "{r:?}");
    assert_eq!(r.warnings, Vec::<String>::new());
    // One range reads that span.
    let old = qj(
        &p,
        json_fixture(r#"{"ranges": [{"startLine": 2, "endLine": 3}]}"#),
    );
    assert_eq!(fetch(&old, &paths).content.as_deref(), Some("l2\nl3\n"));
    // Ranges wholly past the end are an error, not an empty success.
    let past = qj(
        &p,
        json_fixture(r#"{"ranges": [{"startLine": 40, "endLine": 41}]}"#),
    );
    assert!(fetch(&past, &paths).error.is_some());
    // Ordering is checked before execution, never repaired into another selection.
    assert!(
        octocode_runtime_base::contracts::validate_query(
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
        json_fixture(r#"{"ranges": [{"startLine": 8, "endLine": 9}], "block": true}"#),
    );
    let r = fetch(&ranged, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 8, end: 11 }],
        "{r:?}"
    );
    let matched = qj(
        &p,
        json_fixture(
            r#"{
                "matchString": ["return a", "return 1"],
                "contextLines": {"before": 0, "after": 0},
                "block": true
            }"#,
        ),
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
    let short = qj(&p, json_fixture(r#"{"matchString": ["needle_small"]}"#));
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
        json_fixture(
            r#"{
                "matchString": ["needle_small"],
                "contextLines": {"before": 2, "after": 2}
            }"#,
        ),
    );
    let r = fetch(&exact_two, &paths);
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 1, end: 5 }],
        "{r:?}"
    );
    let long = qj(
        &p,
        json_fixture(
            r#"{
                "matchString": ["needle_big"],
                "contextLines": {"before": 2, "after": 2}
            }"#,
        ),
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
        json_fixture(
            r#"{
                "matchString": ["needle_big"],
                "contextLines": {"before": 2, "after": 2},
                "block": true
            }"#,
        ),
    );
    let r = fetch(&block, &paths);
    assert_eq!(r.source_line_ranges, vec![LineRange { start: 12, end: 52 }]);
    // Exact-line reads ask for no more.
    let exact = qj(
        &p,
        json_fixture(
            r#"{
                "matchString": ["needle_small"],
                "contextLines": {"before": 0, "after": 0}
            }"#,
        ),
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
        json_fixture(r#"[{"startLine": 1, "endLine": 10}]"#),
        json_fixture(r#"[{"startLine": 10, "endLine": 20}]"#),
        json_fixture(
            r#"[
                {"startLine": 3, "endLine": 5},
                {"startLine": 30, "endLine": 33}
            ]"#,
        ),
    ] {
        let r = fetch(&qj(&p, serde_json::json!({ "ranges": ranges })), &paths);
        assert!(r.next.is_none(), "{ranges:?}: {r:?}");
    }
    // A read that covers its declaration offers nothing more.
    let whole = qj(
        &p,
        json_fixture(r#"{"ranges": [{"startLine": 1, "endLine": 41}]}"#),
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
            json_fixture(
                r#"{
                    "matchString": ["needle_small"],
                    "contextLines": {"before": 2, "after": 2}
                }"#,
            ),
        ),
        &Paths(t.0.clone()),
    );
    assert_eq!(
        r.source_line_ranges,
        vec![LineRange { start: 1, end: 5 }],
        "{r:?}"
    );
}

/// D1: a `block` read of a declaration longer than one page selects the
/// whole declaration, by range and by match alike. Page 1 is a full page from
/// its first line and `next.nextPage` walks the rest: every line once.
#[test]
fn a_long_block_pages_from_its_first_line_to_its_end() {
    let t = Fixture::new();
    let p = t.0.join("m.py");
    let body: String = (0..3_000)
        .map(|i| format!("    value_{i} = compute({i})\n"))
        .collect();
    // def at line 1, the body on 2-3001, return at 3002.
    let source = format!("def big():\n{body}    return 0\n");
    fs::write(&p, &source).expect("fixture");
    let paths = Paths(t.0.clone());
    for selector in [
        json_fixture(
            r#"{
                "ranges": [{"startLine": 10, "endLine": 12}],
                "block": true
            }"#,
        ),
        json_fixture(r#"{"matchString": ["def big"], "block": true}"#),
        json_fixture(r#"{"matchString": ["value_1500 ="], "block": true}"#),
    ] {
        let mut read = fetch(&qj(&p, selector.clone()), &paths);
        assert!(
            !read.warnings.iter().any(|w| w.starts_with("block:")),
            "{selector}: {:?}",
            read.warnings
        );
        let first = read.pagination.clone().expect("pagination");
        assert_eq!(first.offset, 0, "{selector}: {first:?}");
        assert!(first.has_more, "{selector}: one page cannot hold it");
        // A full page: as many lines as a plain read of the same file takes.
        let plain = fetch(&q(&p), &paths).pagination.expect("pagination");
        assert!(
            first.length + 1 >= plain.length,
            "{selector}: {first:?} against a plain page {plain:?}"
        );
        let mut joined = String::new();
        loop {
            assert_eq!(read.status, ReadStatus::Success, "{selector}: {read:?}");
            joined.push_str(read.content.as_deref().expect("content"));
            let Some(page) = read.next.take().and_then(|next| next.continue_window) else {
                break;
            };
            read = fetch(&page.query, &paths);
        }
        assert_eq!(joined, source, "{selector}");
    }
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
            json_fixture(
                r#"{
                    "matchString": ["first"],
                    "block": true,
                    "contextLines": {"before": 0, "after": 0}
                }"#,
            ),
        ),
        &Paths(t.0.clone()),
    );
    let body = serde_json::to_value(&result).expect("json");
    assert_eq!(
        body["blocks"],
        json_fixture(r#"[{"symbolName": "first", "line": 3, "endLine": 5}]"#),
        "{body}"
    );
    let content = result.content.expect("content");
    // The call on line 10 shows alone, not `second` (8-11) whole.
    assert!(content.contains("def first(a):"), "{content}");
    assert!(!content.contains("def second(b):"), "{content}");
    assert!(content.contains("return first(1)"), "{content}");
}
