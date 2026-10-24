//! Diagnostics: individual and merged reports, coordinates, severity
//! names and unpublished (partial) results.

use super::*;

#[test]
fn diagnostic_reports_are_listed_as_individual_diagnostics() {
    let q = query(serde_json::json!({"operation": "diagnostic", "path": "file:///repo/a.ts"}));
    let report = serde_json::json!({
        "kind": "full",
        "items": [
            {"message": "first", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}},
            {"message": "second", "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 1}}}
        ],
        "version": 3,
        "source": "push",
        "truncated": false
    });
    let (items, truncated) = diagnostic_items(Some(report));
    assert!(!truncated);
    let envelope = items_payload(&q, "diagnostics", items, TEST_BUDGET);
    let listed = envelope["matches"].as_array().expect("matches");
    assert_eq!(listed.len(), 2, "{envelope}");
    assert_eq!(listed[0]["message"], "first");
    assert_eq!(envelope["pagination"]["totalItems"], 2);

    // A clean file (or a pull `unchanged` report) is an empty result, not
    // a one-element list wrapping the report object.
    for report in [
        Some(serde_json::json!({"kind": "full", "items": []})),
        Some(serde_json::json!({"kind": "unchanged", "resultId": "1"})),
    ] {
        let (items, _) = diagnostic_items(report);
        let envelope = with_next(&q, items_payload(&q, "diagnostics", items, TEST_BUDGET));
        assert_eq!(envelope["status"], "empty", "{envelope}");
        assert_eq!(envelope["category"], "noDiagnostics");
    }
    let (_, truncated) = diagnostic_items(Some(serde_json::json!({
        "kind": "full", "items": [{"message": "x"}], "truncated": true
    })));
    assert!(truncated);
}

/// Identical diagnostics (the same message, code, severity, source, link
/// and related notes) at several ranges are listed once, with `count` and
/// every range in `displayRanges`, in report order: the repeated text is
/// stated once and no location is lost. A diagnostic seen once keeps its
/// one `displayRange`.
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
    let (items, _) = diagnostic_items(Some(report));
    let listed = dedupe_diagnostics(items);
    let listed = listed.as_array().expect("diagnostics");
    assert_eq!(listed.len(), 2, "{listed:?}");
    let range = |line: u64| serde_json::json!({"startLine": line + 1, "startCharacter": 1, "endLine": line + 1});
    assert_eq!(listed[0]["count"], 3, "{listed:?}");
    assert_eq!(
        listed[0]["displayRanges"],
        serde_json::json!([range(9), range(4), range(7)])
    );
    assert!(listed[0].get("displayRange").is_none(), "{listed:?}");
    assert_eq!(listed[0]["severity"], "hint");
    assert_eq!(listed[1]["message"], "other");
    assert_eq!(listed[1]["displayRange"], range(2));
    assert!(listed[1].get("count").is_none() && listed[1].get("displayRanges").is_none());
}

#[test]
fn diagnostics_use_the_one_based_public_coordinates() {
    let report = serde_json::json!({"kind": "full", "items": [{
        "message": "m",
        "code": "macro-error",
        "range": {"start": {"line": 973, "character": 4}, "end": {"line": 1015, "character": 5}},
        "relatedInformation": [{"message": "r", "location": {"uri": "file:///repo/a.rs",
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 2}}}}]
    }]});
    let (items, _) = diagnostic_items(Some(report));
    let item = &items[0];
    assert!(item.get("range").is_none(), "{item}");
    assert_eq!(
        item["displayRange"],
        serde_json::json!({"startLine": 974, "startCharacter": 5, "endLine": 1016})
    );
    let related = &item["relatedInformation"][0]["location"];
    assert!(related.get("range").is_none(), "{related}");
    assert_eq!(related["displayRange"]["startLine"], 1);
    assert!(disabled_macro_diagnostics(&items));
    assert!(!disabled_macro_diagnostics(
        &serde_json::json!([{"code": "E0308"}])
    ));
}

/// A server that pushed nothing inside the wait has not said "no
/// diagnostics": the empty row is partial, with a retry, never a clean result.
#[test]
fn unpublished_diagnostics_are_partial_not_clean() {
    let q = query(serde_json::json!({
        "operation": "diagnostic", "path": "file:///repo/a.ts"
    }));
    let mut row = items_payload(&q, "diagnostics", serde_json::json!([]), TEST_BUDGET);
    mark_diagnostics_unpublished(&mut row, &q, 5_000);
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["diagnosticsNotPublished"]),
        "{row}"
    );
    assert_eq!(row["next"]["retry"]["tool"], "lspSearch", "{row}");
    assert!(
        row["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("5000 ms")),
        "{row}"
    );
    crate::contracts::validate_output("lspSearch", &public_row(&row))
        .expect("an unpublished-diagnostics row satisfies the output contract");
}

#[test]
fn empty_diagnostics_carry_no_read_recovery_and_symbol_reads_are_matched() {
    let q = query(serde_json::json!({"operation": "diagnostic", "path": "/repo/a.ts"}));
    let row = with_next(
        &q,
        items_payload(&q, "diagnostics", serde_json::json!([]), TEST_BUDGET),
    );
    assert_eq!(row["category"], "noDiagnostics");
    assert!(row.get("next").is_none(), "{row}");
    // The row states the per-call fact; no static advice repeats on it.
    assert_eq!(
        row["reason"],
        "The language server reported no diagnostics for this document."
    );
    assert!(row.get("hints").is_none(), "{row}");

    let q = query(serde_json::json!({
        "operation": "definition", "path": "/repo/a.ts",
        "symbolName": "run",
        "line": 3
    }));
    let row = failure(
        &q,
        "file:///repo/a.ts",
        ErrorCode::AnchorUnresolved,
        "x",
        true,
    );
    let read = &crate::tools::test_support::checks::related_call(&row, "localFetch", |_| true)["input"]
        ["queries"][0];
    assert_eq!(read["path"], "/repo/a.ts");
    assert_eq!(read["matchString"], serde_json::json!(["run"]));
}

#[test]
fn diagnostic_severity_is_published_by_name() {
    let (items, _) = diagnostic_items(Some(serde_json::json!([
        {"severity": 1, "message": "e", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}}},
        {"severity": 2, "message": "w"},
        {"severity": 3, "message": "i"},
        {"severity": 4, "message": "h"},
        {"severity": 9, "message": "unknown"},
        {"message": "none"}
    ])));
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
        items[0]["displayRange"],
        serde_json::json!({"startLine": 1, "startCharacter": 1, "endLine": 1})
    );
}
