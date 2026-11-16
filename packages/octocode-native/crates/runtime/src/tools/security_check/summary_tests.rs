//! The summary as a projection of a finished row.
use super::*;

fn target(query: Value) -> Target<'static> {
    let Value::Object(query) = query else {
        panic!("a query row");
    };
    Target {
        query,
        asked_name: Some("lodash"),
        asked_version: Some("4.17.20"),
        unfixed_in_major: 0,
        gives_verdict: true,
    }
}

fn package() -> Value {
    json!({"operation": "package", "ecosystem": "npm", "packageName": "lodash", "version": "4.17.20"})
}

/// The whole row is the `details` view, untouched.
#[test]
fn details_is_the_row_as_built() {
    let row = json!({"answer": "x", "advisories": [{"id": "A"}], "sources": [{"source": "osv", "status": "found"}]});
    assert_eq!(
        apply(SecurityCheckView::Details, row.clone(), target(package())),
        row
    );
}

/// A source that answered is silence; one that failed, stopped early or does
/// not cover the target is named with why, and nothing else of it.
#[test]
fn only_a_source_that_leaves_the_row_incomplete_is_named() {
    let row = json!({"affected": false, "advisories": [], "sources": [
        {"source": "osv", "status": "noneKnown"},
        {"source": "github", "status": "found", "observedAt": "2026-11-11", "facts": {"repositoryAdvisories": ["GHSA-1"]}},
    ]});
    let lean = apply(SecurityCheckView::Summary, row, target(package()));
    assert!(lean.get("sources").is_none(), "{lean}");
    for status in ["partial", "failed", "unavailable", "notCovered"] {
        let row = json!({"isPartial": true, "sources": [
            {"source": "osv", "status": "found"},
            {"source": "github", "status": status, "errorCode": "rateLimited", "why": "resets at 2999",
                "observedAt": "2026-11-11", "facts": {"repositoryAdvisories": ["GHSA-1"]}},
        ]});
        let lean = apply(SecurityCheckView::Summary, row, target(package()));
        assert_eq!(
            lean["sources"],
            json!([{"source": "github", "status": status, "errorCode": "rateLimited", "why": "resets at 2999"}]),
            "{status}"
        );
        assert_eq!(lean["isPartial"], true);
    }
}

/// A withdrawn advisory is listed and never counted; the most severe counted
/// one gives the severity and the link, whatever the list's order.
#[test]
fn the_indication_counts_live_advisories_and_links_the_most_severe() {
    let row = json!({"affected": true, "advisories": [
        {"id": "GONE", "severity": "critical", "withdrawn": "2024-01-01T00:00:00Z", "link": "https://osv.dev/vulnerability/GONE"},
        {"id": "LOW", "severity": "low", "link": "https://osv.dev/vulnerability/LOW"},
        {"id": "WORDLESS", "source": "https://example.test/wordless"},
        {"id": "HIGH", "severity": "high", "link": "https://osv.dev/vulnerability/HIGH"},
    ]});
    let mut target = target(package());
    target.unfixed_in_major = 2;
    let lean = apply(SecurityCheckView::Summary, row, target);
    assert_eq!(lean["advisoryCount"], 3, "{lean}");
    assert_eq!(lean["highestSeverity"], "high");
    assert_eq!(lean["unfixedInMajorCount"], 2);
    assert_eq!(lean["link"], "https://osv.dev/vulnerability/HIGH");
    assert!(lean.get("advisories").is_none(), "{lean}");
    // None listed: the count says so, and nothing is linked.
    let none = apply(
        SecurityCheckView::Summary,
        json!({"affected": false, "advisories": []}),
        self::target(package()),
    );
    assert_eq!(none["advisoryCount"], 0, "{none}");
    for absent in ["highestSeverity", "link", "unfixedInMajorCount"] {
        assert!(none.get(absent).is_none(), "{absent}: {none}");
    }
    // A list not finished is not counted.
    let unfinished = apply(
        SecurityCheckView::Summary,
        json!({"answer": "stopped", "isPartial": true}),
        self::target(package()),
    );
    assert!(unfinished.get("advisoryCount").is_none(), "{unfinished}");
}

/// The sentence stays only where no field says it: a row with no verdict,
/// whether or not its list finished. The name and version stay only when the
/// request did not state them.
#[test]
fn the_answer_and_the_coordinates_stay_only_when_they_say_something_new() {
    let row = |extra: Value| {
        let mut row = json!({"answer": "sentence", "packageName": "lodash", "version": "4.17.20"});
        if let (Some(fields), Some(extra)) = (row.as_object_mut(), extra.as_object()) {
            fields.extend(extra.clone());
        }
        row
    };
    let verdict = apply(
        SecurityCheckView::Summary,
        row(json!({"affected": true, "advisories": []})),
        target(package()),
    );
    for echoed in ["answer", "packageName", "version"] {
        assert!(verdict.get(echoed).is_none(), "{echoed}: {verdict}");
    }
    let empty = apply(
        SecurityCheckView::Summary,
        row(json!({"advisories": []})),
        target(package()),
    );
    // No verdict on a finished empty list: the sentence says what the 0 does
    // not prove.
    assert_eq!(empty["answer"], "sentence", "{empty}");
    let no_verdict = apply(
        SecurityCheckView::Summary,
        row(json!({})),
        target(package()),
    );
    assert_eq!(no_verdict["answer"], "sentence", "{no_verdict}");
    // A range the registry resolved, or a URL: the row's coordinates are news.
    let mut resolved = target(package());
    resolved.asked_version = Some("^4");
    resolved.asked_name = None;
    let lean = apply(
        SecurityCheckView::Summary,
        row(json!({"affected": false, "advisories": []})),
        resolved,
    );
    assert_eq!(lean["version"], "4.17.20", "{lean}");
    assert_eq!(lean["packageName"], "lodash", "{lean}");
    // A row that gives no verdict by design keeps no sentence.
    let mut repository = target(json!({"operation": "repository", "repo": "o/r"}));
    repository.gives_verdict = false;
    let lean = apply(
        SecurityCheckView::Summary,
        json!({"answer": "o/r: 5 stars.", "advisories": []}),
        repository,
    );
    assert!(lean.get("answer").is_none(), "{lean}");
}

/// One lead reads the same target whole; the checks of pinned dependencies
/// stay, as no field of the row counts their advisories; every other lead is
/// listed by the whole row.
#[test]
fn the_leads_are_the_whole_row_and_the_pinned_dependency_checks() {
    let call = |rows: Value| json!({"kind": "related", "tool": "securityCheck", "input": {"queries": rows}});
    let pinned = call(
        json!([{"operation": "package", "ecosystem": "npm", "packageName": "dep", "version": "1.0.0"}]),
    );
    let row = json!({"affected": false, "advisories": [], "related": [
        call(json!([{"operation": "advisory", "id": "GHSA-29mw-wpgm-hmr9", "view": "details"}])),
        pinned.clone(),
        call(json!([{"operation": "repository", "repo": "lodash/lodash"}])),
    ]});
    let lean = apply(SecurityCheckView::Summary, row, target(package()));
    let mut whole = package();
    whole["view"] = json!("details");
    assert_eq!(
        lean["related"],
        json!([call(json!([whole])), pinned]),
        "{lean}"
    );
}

/// A version the registry does not publish fails the same in details, so the
/// summary leads only to the nearest published versions, and keeps the hint.
#[test]
fn a_missing_version_leads_to_the_nearest_versions_alone() {
    let call = |queries: Value| json!({"kind": "related", "tool": "securityCheck", "input": {"queries": queries}});
    let mut nearest = package();
    nearest["version"] = json!("4.17.21");
    let row = json!({"status": "error", "errorCode": "notFound",
        "error": "lodash has no published version matching \"4.17.99\".",
        "hints": ["Version not found: check the registry"], "versionEvaluated": false,
        "advisories": [], "related": [call(json!([nearest.clone()]))]});
    let lean = apply(SecurityCheckView::Summary, row, target(package()));
    assert_eq!(lean["related"], json!([call(json!([nearest]))]), "{lean}");
    assert_eq!(
        lean["hints"],
        json!(["Version not found: check the registry"]),
        "{lean}"
    );
}

/// Fields the summary does not name stay: a new disclosure is never dropped
/// by a list that forgot it.
#[test]
fn a_field_the_summary_does_not_know_stays() {
    let row = json!({"affected": false, "advisories": [], "retryable": true,
        "cautions": [{"rule": "archived"}], "warnings": ["w"], "outdated": true,
        "versionEvaluated": false, "registryConfirmed": false, "someLaterDisclosure": 1,
        "next": {"nextPage": {"kind": "continue", "tool": "securityCheck", "input": {"cursor": "c"}}}});
    let lean = apply(SecurityCheckView::Summary, row.clone(), target(package()));
    for kept in [
        "retryable",
        "cautions",
        "warnings",
        "outdated",
        "versionEvaluated",
        "registryConfirmed",
        "someLaterDisclosure",
        "next",
    ] {
        assert_eq!(lean[kept], row[kept], "{kept}");
    }
}

/// A source that answered and still names a read it could not make is named:
/// its status alone would read as whole.
#[test]
fn a_found_source_that_names_a_failed_read_is_named() {
    let row = json!({"isPartial": true, "advisories": [], "sources": [
        {"source": "osv", "status": "noneKnown"},
        {"source": "github", "status": "found", "errorCode": "serverError", "why": "tags not read",
            "facts": {"exists": true}},
    ]});
    let lean = apply(SecurityCheckView::Summary, row, target(package()));
    assert_eq!(
        lean["sources"],
        json!([{"source": "github", "status": "found", "errorCode": "serverError", "why": "tags not read"}]),
        "{lean}"
    );
}

/// A row with no finished list has nothing to count: its sentence says what
/// was read, whatever the operation, so the row never stands on leads alone.
#[test]
fn a_row_with_no_list_keeps_its_sentence() {
    let mut repository = target(json!({"operation": "repository", "repo": "o/r"}));
    repository.gives_verdict = false;
    let lean = apply(
        SecurityCheckView::Summary,
        json!({"answer": "o/r: GitHub has no such repository.", "cautions": [{"rule": "repositoryMissing"}],
            "sources": [{"source": "github", "status": "found"}]}),
        repository,
    );
    assert_eq!(
        lean["answer"], "o/r: GitHub has no such repository.",
        "{lean}"
    );
    assert!(lean.get("advisoryCount").is_none(), "{lean}");
}

/// Withdrawn GitHub records are no match and never counted; the summary
/// states them as one count (the `details` view lists each with its date),
/// and its `related` details call stays to read them.
#[test]
fn withdrawn_warnings_collapse_to_one_count_in_the_summary() {
    let withdrew = |id: &str| {
        format!(
            "GitHub withdrew {id} at 2021-10-01T22:04:28Z: not a match; related reads it whole."
        )
    };
    let other = "OSV lists a range GitHub does not.".to_owned();
    let warnings = json!([
        withdrew("GHSA-aaaa-bbbb-cccc"),
        other,
        withdrew("GHSA-dddd-eeee-ffff"),
        withdrew("GHSA-gggg-hhhh-iiii"),
    ]);
    let row = json!({"affected": true, "advisories": [], "warnings": warnings});
    let details = apply(SecurityCheckView::Details, row.clone(), target(package()));
    assert_eq!(details["warnings"], warnings, "details lists each");
    let lean = apply(SecurityCheckView::Summary, row, target(package()));
    let lean = lean["warnings"].as_array().expect("warnings");
    assert_eq!(lean.len(), 2, "{lean:?}");
    let count = lean[0].as_str().expect("count");
    assert!(count.contains("withdrew 3 advisories"), "{count}");
    assert!(count.contains("related"), "{count}");
    assert!(!count.contains("GHSA-"), "{count}");
    assert_eq!(lean[1], "OSV lists a range GitHub does not.");
}
