//! Diagnostics: individual and merged reports, coordinates, severity
//! names and unpublished (partial) results.

use super::*;
use octocode_test_support::json_fixture;

#[test]
fn diagnostic_reports_are_listed_as_individual_diagnostics() {
    let q = query(json_fixture(
        r#"{"operation": "diagnostic", "path": "file:///repo/a.ts"}"#,
    ));
    let report = json_fixture(
        r#"{
            "kind": "full",
            "items": [
                {
                    "message": "first",
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 1}
                    }
                },
                {
                    "message": "second",
                    "range": {
                        "start": {"line": 1, "character": 0},
                        "end": {"line": 1, "character": 1}
                    }
                }
            ],
            "version": 3,
            "source": "push"
        }"#,
    );
    let items = diagnostic_items(Some(report));
    let envelope = items_payload(&q, "diagnostics", items, TEST_BUDGET);
    let listed = envelope["matches"].as_array().expect("matches");
    assert_eq!(listed.len(), 2, "{envelope}");
    assert_eq!(listed[0]["message"], "first");
    assert_eq!(envelope["pagination"]["total"], 2);

    // A clean file (or a pull `unchanged` report) is an empty result, not
    // a one-element list wrapping the report object.
    for report in [
        Some(json_fixture(r#"{"kind": "full", "items": []}"#)),
        Some(json_fixture(r#"{"kind": "unchanged", "resultId": "1"}"#)),
    ] {
        let items = diagnostic_items(report);
        let envelope = with_next(&q, items_payload(&q, "diagnostics", items, TEST_BUDGET));
        assert_eq!(envelope["status"], "empty", "{envelope}");
        assert_eq!(envelope["category"], "noDiagnostics");
    }
}

/// A full report is a paginated list, including reports larger than the old
/// push cache's item quota. Copy each continuation unchanged and reconstruct
/// the complete selected evidence in order, once.
#[test]
fn diagnostic_pages_reconstruct_large_reports_and_tiny_windows() {
    for (count, limit) in [(31, 1), (4_097, 97)] {
        let first = query(serde_json::json!({
            "operation": "diagnostic", "path": "/repo/a.ts", "limit": limit
        }));
        let expected = diagnostic_items(Some(serde_json::json!({
            "kind": "full",
            "items": (0..count).map(|index| serde_json::json!({
                "message": format!("diagnostic {index}: λ 東京"),
                "range": {"start": {"line": index, "character": 2}, "end": {"line": index, "character": 3}}
            })).collect::<Vec<_>>()
        })));
        let mut current = first;
        let mut collected = Vec::new();
        loop {
            let row = items_payload(&current, "diagnostics", expected.clone(), TEST_BUDGET);
            assert!(row.get("errorCode").is_none(), "{row}");
            assert!(row.get("terminalLimit").is_none(), "{row}");
            let page = row["matches"].as_array().expect("diagnostic page");
            assert!(!page.is_empty() && page.len() <= limit);
            collected.extend(page.iter().cloned());
            assert!(collected.len() <= count, "continuation repeated evidence");
            let Some(next) = row.pointer("/next/nextPage/input/queries/0") else {
                assert_eq!(row["pagination"]["hasMore"], false);
                break;
            };
            assert_eq!(row["pagination"]["hasMore"], true);
            current = serde_json::from_value(next.clone()).expect("executable continuation");
        }
        assert_eq!(serde_json::Value::Array(collected), expected);
    }
}

#[test]
fn a_large_indivisible_diagnostic_is_kept_whole() {
    let q = query(json_fixture(
        r#"{"operation": "diagnostic", "path": "/repo/a.ts"}"#,
    ));
    let items = diagnostic_items(Some(serde_json::json!({"kind": "full", "items": [{
        "message": "λ東京".repeat(100_000), "data": {"evidence": "complete"}
    }]})));
    let row = items_payload(&q, "diagnostics", items.clone(), 1);
    assert_eq!(row["matches"], items);
    assert_eq!(row["pagination"]["hasMore"], false);
    assert!(row.get("terminalLimit").is_none());
}

/// Identical diagnostics (the same message, code, severity, source, link
/// and related notes) at several ranges are listed once, with `count` and
/// every place in `sites`, in report order: the repeated text is stated
/// once and no location is lost. A diagnostic seen once keeps its own
/// `line` and `column`.
#[test]
fn identical_diagnostics_are_listed_once_with_every_range() {
    let item = |line: u64, message: &str| {
        serde_json::json!({
            "message": message, "code": "proc-macro-disabled", "severity": 4, "source": "rust-analyzer",
            "codeDescription": {"href": "https://rust-analyzer.github.io/manual.html#proc-macro-disabled"},
            "range": {"start": {"line": line, "character": 0}, "end": {"line": line, "character": 4}}
        })
    };
    let report = serde_json::json!({"kind": "full", "items": [
        item(9, "proc-macro expansion is disabled"),
        item(2, "other"),
        item(4, "proc-macro expansion is disabled"),
        item(7, "proc-macro expansion is disabled"),
    ]});
    let items = diagnostic_items(Some(report));
    let listed = dedupe_diagnostics(items);
    let listed = listed.as_array().expect("diagnostics");
    assert_eq!(listed.len(), 2, "{listed:?}");
    let range = |line: u64| serde_json::json!({"line": line + 1, "column": 1});
    assert_eq!(listed[0]["count"], 3, "{listed:?}");
    assert_eq!(
        listed[0]["sites"],
        serde_json::json!([range(9), range(4), range(7)])
    );
    assert!(listed[0].get("line").is_none(), "{listed:?}");
    assert_eq!(listed[0]["severity"], "hint");
    assert_eq!(listed[1]["message"], "other");
    assert_eq!(
        (&listed[1]["line"], &listed[1]["column"]),
        (&range(2)["line"], &range(2)["column"])
    );
    assert!(listed[1].get("count").is_none() && listed[1].get("sites").is_none());
}

#[test]
fn diagnostic_deduplication_preserves_first_order_and_complete_values() {
    let message = "λ東京 diagnostic detail\n".repeat(4_096);
    let first = serde_json::json!({
        "message": message, "severity": "error", "data": {"first": 1, "second": [2, 3]},
        "line": 9, "column": 2
    });
    // Object field order does not change diagnostic identity.
    let repeated: serde_json::Value = serde_json::from_str(&format!(
        "{{\"data\":{{\"second\":[2,3],\"first\":1}},\"severity\":\"error\",\"message\":{},\"line\":3,\"column\":4}}",
        serde_json::to_string(&message).expect("JSON text")
    ))
    .expect("reordered diagnostic");
    let second = json_fixture(r#"{"message": "second", "line": 2, "column": 1}"#);
    let third = serde_json::json!({"message": message, "severity": "warning", "line": 5});
    let listed = dedupe_diagnostics(serde_json::json!([first, second, repeated, third]));
    assert_eq!(listed.as_array().expect("diagnostics").len(), 3);
    assert_eq!(listed[0]["message"], message);
    assert_eq!(listed[0]["count"], 2);
    assert_eq!(
        listed[0]["sites"],
        json_fixture(r#"[{"line": 9, "column": 2}, {"line": 3, "column": 4}]"#)
    );
    assert_eq!(listed[1], second);
    assert_eq!(listed[2], third);
}

#[test]
fn diagnostics_use_the_one_based_public_coordinates() {
    let report = json_fixture(
        r#"{
            "kind": "full",
            "items": [
                {
                    "message": "m",
                    "code": "macro-error",
                    "range": {
                        "start": {"line": 973, "character": 4},
                        "end": {"line": 1015, "character": 5}
                    },
                    "relatedInformation": [
                        {
                            "message": "r",
                            "location": {
                                "uri": "file:///repo/a.rs",
                                "range": {
                                    "start": {"line": 0, "character": 0},
                                    "end": {"line": 0, "character": 2}
                                }
                            }
                        }
                    ]
                }
            ]
        }"#,
    );
    let items = diagnostic_items(Some(report));
    let item = &items[0];
    assert!(item.get("range").is_none(), "{item}");
    assert_eq!(
        (&item["line"], &item["column"], &item["endLine"]),
        (
            &serde_json::json!(974),
            &serde_json::json!(5),
            &serde_json::json!(1016)
        )
    );
    let related = &item["relatedInformation"][0]["location"];
    assert!(related.get("range").is_none(), "{related}");
    assert_eq!(related["line"], 1);
}

/// A server that pushed nothing inside the wait has not said "no
/// diagnostics": the empty row is partial, with an optional reattempt, never a clean result.
#[test]
fn unpublished_diagnostics_are_partial_not_clean() {
    let q = query(json_fixture(
        r#"{"operation": "diagnostic", "path": "file:///repo/a.ts"}"#,
    ));
    let mut row = items_payload(&q, "diagnostics", serde_json::json!([]), TEST_BUDGET);
    mark_diagnostics_unpublished(&mut row, &q, 5_000);
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        json_fixture(r#"["diagnosticsNotPublished"]"#),
        "{row}"
    );
    assert_eq!(
        octocode_runtime_base::test_support::checks::related_call(&row, "lspSearch", |_| true)["tool"],
        "lspSearch",
        "{row}"
    );
    assert!(
        row["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("5000 ms")),
        "{row}"
    );
    assert!(
        row["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("shared memory budget")),
        "a memory-evicted report is unavailable rather than clean: {row}"
    );
    octocode_runtime_base::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("an unpublished-diagnostics row satisfies the output contract");
}

#[test]
fn empty_diagnostics_carry_no_read_recovery_and_symbol_reads_are_matched() {
    let q = query(json_fixture(
        r#"{"operation": "diagnostic", "path": "/repo/a.ts"}"#,
    ));
    let row = with_next(
        &q,
        items_payload(&q, "diagnostics", serde_json::json!([]), TEST_BUDGET),
    );
    assert_eq!(row["category"], "noDiagnostics");
    assert!(row.get("next").is_none(), "{row}");
    // The category is the per-call fact: no reason restates it and no
    // static advice repeats on it.
    assert!(row.get("reason").is_none(), "{row}");
    assert!(row.get("hints").is_none(), "{row}");

    let q = query(json_fixture(
        r#"{
            "operation": "definition",
            "path": "/repo/a.ts",
            "symbolName": "run",
            "line": 3
        }"#,
    ));
    let row = failure(&q, "file:///repo/a.ts", ErrorCode::NotFound, "x", true);
    let read = &octocode_runtime_base::test_support::checks::related_call(
        &row,
        "localFetch",
        |_| true,
    )["input"]["queries"][0];
    assert_eq!(read["path"], "/repo/a.ts");
    assert_eq!(read["matchString"], json_fixture(r#"["run"]"#));
}

#[test]
fn diagnostic_severity_is_published_by_name() {
    let items = diagnostic_items(Some(json_fixture(
        r#"[
            {
                "severity": 1,
                "message": "e",
                "range": {
                    "start": {"line": 0, "character": 0},
                    "end": {"line": 0, "character": 1}
                }
            },
            {"severity": 2, "message": "w"},
            {"severity": 3, "message": "i"},
            {"severity": 4, "message": "h"},
            {"severity": 9, "message": "unknown"},
            {"message": "none"}
        ]"#,
    )));
    let severities = items
        .as_array()
        .expect("items")
        .iter()
        .map(|item| item.get("severity").cloned())
        .collect::<Vec<_>>();
    assert_eq!(
        severities,
        vec![
            Some(serde_json::json!("error")),
            Some(serde_json::json!("warning")),
            Some(serde_json::json!("information")),
            Some(serde_json::json!("hint")),
            Some(serde_json::json!(9)),
            None,
        ]
    );
    assert_eq!(
        (&items[0]["line"], &items[0]["column"]),
        (&serde_json::json!(1), &serde_json::json!(1))
    );
    assert!(items[0].get("endLine").is_none());
}
