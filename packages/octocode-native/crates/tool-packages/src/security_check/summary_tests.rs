//! The summary as a projection of a finished row.
use super::*;
use octocode_test_support::json_fixture;

fn target(_query: Value) -> Target<'static> {
    Target {
        asked_name: Some("lodash"),
        asked_version: Some("4.17.20"),
        unfixed_in_major: 0,
        gives_verdict: true,
        upgrade_prerelease: false,
    }
}

fn package() -> Value {
    json_fixture(
        r#"{
            "operation": "package",
            "ecosystem": "npm",
            "packageName": "lodash",
            "version": "4.17.20"
        }"#,
    )
}

/// The whole row is the `details` view, untouched.
#[test]
fn details_is_the_row_as_built() {
    let row = json_fixture(
        r#"{
            "answer": "x",
            "advisories": [{"id": "A"}],
            "sources": [{"source": "osv", "status": "found"}]
        }"#,
    );
    assert_eq!(
        apply(SecurityCheckView::Details, row.clone(), target(package())),
        row
    );
}

/// A source that answered is silence; one that failed, stopped early or does
/// not cover the target is named with why, and nothing else of it.
#[test]
fn only_a_source_that_leaves_the_row_incomplete_is_named() {
    let row = json_fixture(
        r#"{
            "affected": false,
            "advisories": [],
            "sources": [
                {"source": "osv", "status": "noneKnown"},
                {
                    "source": "github",
                    "status": "found",
                    "readOn": "2026-11-11",
                    "facts": {"repositoryAdvisories": ["GHSA-1"]}
                }
            ]
        }"#,
    );
    let lean = apply(SecurityCheckView::Summary, row, target(package()));
    assert!(lean.get("sources").is_none(), "{lean}");
    for status in ["partial", "failed", "unavailable", "notCovered"] {
        let row = json!({"isPartial": true, "sources": [
            {"source": "osv", "status": "found"},
            {"source": "github", "status": status, "errorCode": "rateLimited", "why": "resets at 2999",
                "readOn": "2026-11-11", "facts": {"repositoryAdvisories": ["GHSA-1"]}},
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
/// one gives the severity and the link, whatever the list's order. The ids
/// name each counted advisory once, in the list's order (two sources'
/// records of one advisory share an id).
#[test]
fn the_indication_counts_live_advisories_and_links_the_most_severe() {
    let row = json_fixture(
        r#"{
            "affected": true,
            "advisories": [
                {
                    "id": "LOW",
                    "severity": "low",
                    "source": "https://github.com/advisories/LOW"
                },
                {
                    "id": "GONE",
                    "severity": "critical",
                    "withdrawn": "2024-01-01T00:00:00Z",
                    "link": "https://osv.dev/vulnerability/GONE"
                },
                {
                    "id": "LOW",
                    "severity": "low",
                    "link": "https://osv.dev/vulnerability/LOW"
                },
                {
                    "id": "WORDLESS",
                    "source": "https://example.test/wordless"
                },
                {
                    "id": "HIGH",
                    "severity": "high",
                    "link": "https://osv.dev/vulnerability/HIGH"
                }
            ]
        }"#,
    );
    let mut target = target(package());
    target.unfixed_in_major = 2;
    let lean = apply(SecurityCheckView::Summary, row, target);
    assert_eq!(lean["advisoryCount"], 4, "{lean}");
    let ids = lean["advisories"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| row["id"].clone())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["LOW", "WORDLESS", "HIGH"], "{lean}");
    assert!(
        lean["advisories"]
            .as_array()
            .into_iter()
            .flatten()
            .all(|row| row.get("link").is_none()),
        "a link the id derives is not restated: {lean}"
    );
    assert_eq!(lean["highestSeverity"], "high");
    assert_eq!(lean["unfixedInMajorCount"], 2);
    assert_eq!(lean["link"], "https://osv.dev/vulnerability/HIGH");
    // None listed: the count says so, and nothing is linked.
    let none = apply(
        SecurityCheckView::Summary,
        json_fixture(r#"{"affected": false, "advisories": []}"#),
        self::target(package()),
    );
    assert_eq!(none["advisoryCount"], 0, "{none}");
    for absent in [
        "advisories",
        "highestSeverity",
        "link",
        "unfixedInMajorCount",
    ] {
        assert!(none.get(absent).is_none(), "{absent}: {none}");
    }
    // A list not finished is not counted.
    let unfinished = apply(
        SecurityCheckView::Summary,
        json_fixture(r#"{"answer": "stopped", "isPartial": true}"#),
        self::target(package()),
    );
    assert!(unfinished.get("advisoryCount").is_none(), "{unfinished}");
    assert!(unfinished.get("advisories").is_none(), "{unfinished}");
}

/// The sentence stays only where no field says it: a row with no verdict,
/// whether or not its list finished. The name and version stay only when the
/// request did not state them.
#[test]
fn the_answer_and_the_coordinates_stay_only_when_they_say_something_new() {
    let row = |extra: Value| {
        let mut row = json_fixture(
            r#"{
                "answer": "sentence",
                "packageName": "lodash",
                "version": "4.17.20"
            }"#,
        );
        if let (Some(fields), Some(extra)) = (row.as_object_mut(), extra.as_object()) {
            fields.extend(extra.clone());
        }
        row
    };
    let verdict = apply(
        SecurityCheckView::Summary,
        row(json_fixture(r#"{"affected": true, "advisories": []}"#)),
        target(package()),
    );
    for echoed in ["answer", "packageName", "version"] {
        assert!(verdict.get(echoed).is_none(), "{echoed}: {verdict}");
    }
    let empty = apply(
        SecurityCheckView::Summary,
        row(json_fixture(r#"{"advisories": []}"#)),
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
    // A verdict of none known counts 0, which proves nothing: the sentence
    // that says so stays.
    let none_known = apply(
        SecurityCheckView::Summary,
        row(json_fixture(r#"{"affected": false, "advisories": []}"#)),
        target(package()),
    );
    assert_eq!(none_known["answer"], "sentence", "{none_known}");
    assert_eq!(none_known["advisoryCount"], 0, "{none_known}");
    // A range the registry resolved, or a URL: the row's coordinates are news.
    let mut resolved = target(package());
    resolved.asked_version = Some("^4");
    resolved.asked_name = None;
    let lean = apply(
        SecurityCheckView::Summary,
        row(json_fixture(r#"{"affected": false, "advisories": []}"#)),
        resolved,
    );
    assert_eq!(lean["version"], "4.17.20", "{lean}");
    assert_eq!(lean["packageName"], "lodash", "{lean}");
    // A row that gives no verdict by design keeps its sentence: its count is
    // of its own records, and only the sentence says so.
    let mut repository = target(json_fixture(
        r#"{"operation": "repository", "owner": "o", "repo": "r"}"#,
    ));
    repository.gives_verdict = false;
    let lean = apply(
        SecurityCheckView::Summary,
        json_fixture(r#"{"answer": "o/r: 5 stars.", "advisories": []}"#),
        repository,
    );
    assert_eq!(lean["answer"], "o/r: 5 stars.", "{lean}");
}

/// The checks of pinned dependencies stay, as no field of the row counts
/// their advisories; no lead reads an advisory the row lists (the details
/// view states it), and every other lead is left to the details view.
#[test]
fn the_leads_are_the_whole_row_and_the_pinned_dependency_checks() {
    let call = |rows: Value| json!({"kind": "related", "tool": "securityCheck", "input": {"queries": rows}});
    let pinned = call(json_fixture(
        r#"[
            {
                "operation": "package",
                "ecosystem": "npm",
                "packageName": "dep",
                "version": "1.0.0"
            }
        ]"#,
    ));
    let row = json!({"affected": true, "advisories": [{"id": "GHSA-29mw-wpgm-hmr9"}], "related": [
        call(json_fixture(r#"[
            {
                "operation": "advisory",
                "id": "GHSA-29mw-wpgm-hmr9",
                "view": "details"
            }
        ]"#)),
        pinned.clone(),
        call(json_fixture(r#"[{"operation": "repository", "owner": "lodash", "repo": "lodash"}]"#)),
    ]});
    let lean = apply(SecurityCheckView::Summary, row, target(package()));
    assert_eq!(lean["related"], json!([pinned]), "{lean}");
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
        json_fixture(r#"["Version not found: check the registry"]"#),
        "{lean}"
    );
}

/// A version that was not evaluated has no count of its own: the package's
/// advisories OSV lists are not counted, ranked or linked as the version's,
/// and the sentence that says what they are stays.
#[test]
fn a_version_not_evaluated_counts_no_advisory_as_its_own() {
    let row = json_fixture(
        r#"{
            "status": "error",
            "errorCode": "notFound",
            "answer": "lodash: the requested version is not on the registry, so it was not evaluated. OSV has 1 known advisory for this package.",
            "versionEvaluated": false,
            "advisories": [
                {"id": "GHSA-1", "severity": "critical", "link": "https://osv.dev/vulnerability/GHSA-1"}
            ]
        }"#,
    );
    let mut asked = target(package());
    asked.unfixed_in_major = 1;
    let lean = apply(SecurityCheckView::Summary, row.clone(), asked);
    for absent in [
        "advisoryCount",
        "advisories",
        "highestSeverity",
        "link",
        "unfixedInMajorCount",
    ] {
        assert!(lean.get(absent).is_none(), "{absent}: {lean}");
    }
    assert_eq!(lean["answer"], row["answer"], "{lean}");
}

/// Fields the summary does not name stay: a new disclosure is never dropped
/// by a list that forgot it.
#[test]
fn a_field_the_summary_does_not_know_stays() {
    let row = json_fixture(
        r#"{
            "affected": false,
            "advisories": [],
            "retryable": true,
            "cautions": [{"rule": "archived"}],
            "warnings": ["w"],
            "outdated": false,
            "versionEvaluated": false,
            "registryConfirmed": false,
            "someLaterDisclosure": 1,
            "next": {
                "nextPage": {
                    "kind": "continue",
                    "tool": "securityCheck",
                    "input": {"cursor": "c"}
                }
            }
        }"#,
    );
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
    let row = json_fixture(
        r#"{
            "isPartial": true,
            "advisories": [],
            "sources": [
                {"source": "osv", "status": "noneKnown"},
                {
                    "source": "github",
                    "status": "found",
                    "errorCode": "serverError",
                    "why": "tags not read",
                    "facts": {"exists": true}
                }
            ]
        }"#,
    );
    let lean = apply(SecurityCheckView::Summary, row, target(package()));
    assert_eq!(
        lean["sources"],
        json_fixture(
            r#"[
                {
                    "source": "github",
                    "status": "found",
                    "errorCode": "serverError",
                    "why": "tags not read"
                }
            ]"#
        ),
        "{lean}"
    );
}

/// A row with no finished list has nothing to count: its sentence says what
/// was read, whatever the operation, so the row never stands on leads alone.
#[test]
fn a_row_with_no_list_keeps_its_sentence() {
    let mut repository = target(json_fixture(
        r#"{"operation": "repository", "owner": "o", "repo": "r"}"#,
    ));
    repository.gives_verdict = false;
    let lean = apply(
        SecurityCheckView::Summary,
        json_fixture(
            r#"{
                "answer": "o/r: GitHub has no such repository.",
                "cautions": [{"rule": "repositoryMissing"}],
                "sources": [{"source": "github", "status": "found"}]
            }"#,
        ),
        repository,
    );
    assert_eq!(
        lean["answer"], "o/r: GitHub has no such repository.",
        "{lean}"
    );
    assert!(lean.get("advisoryCount").is_none(), "{lean}");
}

/// A target no source covers (a release OSV does not name, a distribution
/// version asked with no release) gives no verdict and no count: its empty
/// list is not none known, and its sentence says why.
#[test]
fn a_target_no_source_covers_has_no_count() {
    let row = json_fixture(
        r#"{
            "answer": "openssl@3.1.4-r0: not evaluated.",
            "packageName": "openssl",
            "version": "3.1.4-r0",
            "advisories": [],
            "sources": [{"source": "osv", "status": "notCovered", "why": "no release"}]
        }"#,
    );
    let lean = apply(SecurityCheckView::Summary, row, target(package()));
    assert!(lean.get("advisoryCount").is_none(), "{lean}");
    assert_eq!(lean["answer"], "openssl@3.1.4-r0: not evaluated.", "{lean}");
    // One source that answered makes the empty list a count again.
    let answered = json_fixture(
        r#"{
            "affected": false,
            "advisories": [],
            "sources": [
                {"source": "osv", "status": "noneKnown"},
                {"source": "github", "status": "notCovered", "why": "not on GitHub"}
            ]
        }"#,
    );
    let lean = apply(SecurityCheckView::Summary, answered, target(package()));
    assert_eq!(lean["advisoryCount"], 0, "{lean}");
}

/// A newer release is named once: `latestVersion` stays only when it is not
/// the `upgradeTo` already stated, and `outdated:true` is derived from either.
#[test]
fn the_newer_release_is_named_once() {
    let outdated = |upgrade_to: Option<&str>, latest: &str| {
        let mut row = json!({"affected": true, "advisories": [], "outdated": true,
            "latestVersion": latest});
        if let Some(upgrade) = upgrade_to {
            row["upgradeTo"] = json!(upgrade);
        }
        apply(SecurityCheckView::Summary, row, target(package()))
    };
    let same = outdated(Some("4.18.1"), "4.18.1");
    assert!(same.get("latestVersion").is_none(), "{same}");
    assert!(same.get("outdated").is_none(), "{same}");
    let apart = outdated(Some("4.17.21"), "4.18.1");
    assert_eq!(apart["latestVersion"], "4.18.1", "{apart}");
    assert!(apart.get("outdated").is_none(), "{apart}");
}

/// The summary passes the row's warnings as built, and keeps one lead: the
/// read of a record the row does not list (one only GitHub matches). Reads
/// of advisories the row lists are the details view's, chosen by id,
/// whatever a warning says.
#[test]
fn a_record_the_row_does_not_list_keeps_its_read() {
    let call = |id: &str| {
        json!({"kind": "related", "tool": "securityCheck", "input": {"queries": [
            {"operation": "advisory", "id": id, "view": "details"}]}})
    };
    let row = json!({
        "affected": true,
        "advisories": [
            {"id": "GHSA-listed-aaaa", "aliases": ["CVE-2026-0001"]},
            {"id": "GHSA-listed-bbbb"}
        ],
        "warnings": ["GitHub withdrew GHSA-gone-dddd: not counted.",
            "GitHub's advisory database matches GHSA-only-cccc to this version and OSV does not list it: GitHub's word, not counted in answer; related reads each."],
        "related": [call("GHSA-listed-aaaa"), call("CVE-2026-0001"), call("GHSA-listed-bbbb"), call("GHSA-only-cccc")]
    });
    let warnings = row["warnings"].clone();
    let lean = apply(SecurityCheckView::Summary, row, target(package()));
    assert_eq!(lean["warnings"], warnings, "{lean}");
    assert_eq!(lean["related"], json!([call("GHSA-only-cccc")]), "{lean}");
}

/// An advisory whose database states no severity word ranks by the CVSS
/// rating of its vector (CVE-2023-25193's 7.5: high), above a `moderate`
/// one; the row's link is that advisory's.
#[test]
fn a_vector_only_advisory_ranks_by_its_cvss_rating() {
    let row = json!({
        "affected": true,
        "advisories": [
            {"id": "GHSA-word-aaaa", "severity": "moderate", "link": "https://osv.dev/vulnerability/GHSA-word-aaaa"},
            {"id": "OSV-2023-25193", "cvss": [{"type": "CVSS_V3",
                "vector": "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H"}],
                "link": "https://osv.dev/vulnerability/OSV-2023-25193"}
        ]
    });
    let lean = apply(SecurityCheckView::Summary, row, target(package()));
    assert_eq!(lean["highestSeverity"], "high", "{lean}");
    assert_eq!(
        lean["link"], "https://osv.dev/vulnerability/OSV-2023-25193",
        "{lean}"
    );
    // A word the database states still outranks nothing it does not state.
    let worded = json!({"affected": true, "advisories": [
        {"id": "A", "severity": "critical"},
        {"id": "B", "cvss": [{"type": "CVSS_V3", "vector": "CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:H"}]}
    ]});
    let lean = apply(SecurityCheckView::Summary, worded, target(package()));
    assert_eq!(lean["highestSeverity"], "critical", "{lean}");
}

/// A sentence that explains the pinned-dependency checks in `related` stays:
/// no other field says why those checks are listed.
#[test]
fn the_sentence_explaining_pinned_checks_stays_with_its_leads() {
    let sentence = format!(
        "No advisory. It pins 1 runtime dependency to an exact version: {}",
        "related lists a check of each."
    );
    assert!(super::super::answer::explains_pinned_checks(&sentence));
    assert!(!super::super::answer::explains_pinned_checks(
        "No advisory is listed."
    ));
}
