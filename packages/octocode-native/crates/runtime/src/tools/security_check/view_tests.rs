//! Each operation in both views, against recorded doubles: the summary states
//! what decides, `details` states the whole row, and the summary's lead is the
//! call that returns it.
use super::*;
use crate::tools::security_check::summary::DETAIL_FIELDS;
use crate::tools::test_support::assert_row_contract;

/// The fields a summary computes from the list it leaves to `details`.
const SUMMARY_ONLY: [&str; 4] = [
    "advisoryCount",
    "highestSeverity",
    "unfixedInMajorCount",
    "link",
];
/// Fields a summary narrows or states only when they say something new.
const NARROWED: [&str; 5] = ["answer", "packageName", "version", "sources", "related"];

fn keys(row: &Value) -> Vec<&str> {
    row.as_object()
        .map(|fields| fields.keys().map(String::as_str).collect())
        .unwrap_or_default()
}

/// `details` holds everything: each of its fields is in the summary too, or
/// is one the summary leaves to it; and the summary adds only what it counts
/// from the list, which `details` states whole instead.
fn assert_details_holds_the_summary(summary: &Value, details: &Value) {
    for field in keys(details) {
        assert!(
            summary.get(field).is_some()
                || DETAIL_FIELDS.contains(&field)
                || NARROWED.contains(&field),
            "{field} is in neither view's account: {summary}"
        );
    }
    for field in keys(summary) {
        // The lead to the whole row is the summary's own.
        assert!(
            details.get(field).is_some() || SUMMARY_ONLY.contains(&field) || field == "related",
            "{field} is only in the summary: {details}"
        );
        if !SUMMARY_ONLY.contains(&field) && !NARROWED.contains(&field) {
            assert_eq!(summary[field], details[field], "{field}");
        }
    }
    for field in SUMMARY_ONLY {
        assert!(details.get(field).is_none(), "{field}: {details}");
    }
}

/// The one `related` call of a summary: its first query row.
fn details_call(summary: &Value) -> Value {
    let call = &summary["related"][0];
    assert_eq!(call["kind"], "related", "{summary}");
    assert_eq!(call["tool"], "securityCheck", "{summary}");
    let row = call["input"]["queries"][0].clone();
    assert_eq!(row["view"], "details", "{summary}");
    row
}

fn fixed(id: &str, severity: &str, versions: &[&str]) -> Value {
    let events = versions
        .iter()
        .map(|version| json!({"fixed": version}))
        .collect::<Vec<_>>();
    json!({"id": id, "summary": format!("{id} in one line"), "details": "Long text.",
        "database_specific": {"severity": severity},
        "references": [{"type": "WEB", "url": "https://example.test/ref"}],
        "affected": [{"package": {"name": "lodash", "ecosystem": "npm"},
            "ranges": [{"type": "SEMVER", "events": events}]}]})
}

/// The registry with its latest release readable, so a row states `outdated`.
fn registry_with_latest() -> RouteHttp {
    let latest = json!({"name": "lodash", "version": "4.18.1"});
    registry()
        .json("/lodash/latest", latest.clone())
        .json("/lodash/4.18.1", latest)
}

/// lodash@4.17.20 with one advisory its own major fixes, one only a later
/// major fixes, and a withdrawn one; OSV knows none at 5.0.0.
fn affected_package() -> RouteHttp {
    let mut withdrawn = fixed("GHSA-GONE", "CRITICAL", &["4.17.21"]);
    withdrawn["withdrawn"] = json!("2024-01-01T00:00:00Z");
    registry_with_latest().osv_pages(vec![
        (
            200,
            json!({"vulns": [
                fixed("GHSA-LATER", "MODERATE", &["5.0.0"]),
                fixed("GHSA-HIGHER", "HIGH", &["4.17.21", "5.0.0"]),
                withdrawn,
            ]}),
        ),
        (200, json!({})),
    ])
}

fn package_query(view: &str) -> Value {
    json!({"operation": "package", "ecosystem": "npm", "packageName": "lodash",
        "version": "4.17.20", "view": view})
}

#[tokio::test]
async fn a_package_summary_is_the_verdict_the_counts_and_one_lead() {
    let summary = answer(&affected_package(), package_query("summary"))
        .await
        .expect("summary");
    let details = answer(&affected_package(), package_query("details"))
        .await
        .expect("details");
    assert_eq!(summary["affected"], true, "{summary}");
    assert_eq!(summary["upgradeTo"], "5.0.0", "{summary}");
    assert_eq!(summary["advisoryCount"], 2, "{summary}");
    assert_eq!(summary["highestSeverity"], "high", "{summary}");
    assert_eq!(summary["unfixedInMajorCount"], 1, "{summary}");
    assert_eq!(
        summary["link"], "https://osv.dev/vulnerability/GHSA-HIGHER",
        "{summary}"
    );
    assert_eq!(summary["outdated"], true, "{summary}");
    // Outdated names the release to move to; the rest of the release facts
    // stay in details.
    assert_eq!(summary["latestVersion"], "4.18.1", "{summary}");
    for detail in DETAIL_FIELDS
        .iter()
        .filter(|detail| **detail != "latestVersion")
        .chain(&["answer", "packageName", "version"])
    {
        assert!(summary.get(*detail).is_none(), "{detail}: {summary}");
    }
    // What the summary counted is what the whole row lists.
    let listed = details["advisories"].as_array().expect("advisories");
    assert_eq!(listed.len(), 3, "{details}");
    assert!(
        details["upgradeNote"]
            .as_str()
            .is_some_and(|note| note.contains("GHSA-LATER")),
        "{details}"
    );
    assert_eq!(details["latestVersion"], "4.18.1", "{details}");
    assert_details_holds_the_summary(&summary, &details);
    // The lead is the call that returns the whole row.
    let replayed = answer(&affected_package(), details_call(&summary))
        .await
        .expect("replayed");
    assert_eq!(replayed, details);
    assert_eq!(summary["related"].as_array().map(Vec::len), Some(1));
    assert_row_contract(ToolId::SecurityCheck, &summary);
    assert_row_contract(ToolId::SecurityCheck, &details);
}

/// An advisory OSV lists only at a skipped candidate does not affect the asked
/// version: the major-line note and its count name only the version's own, and
/// the skip is said apart (vite 5.4.0: 6.4.4 skipped for GHSA-8g5g).
#[tokio::test]
async fn the_major_note_names_only_the_asked_versions_advisories() {
    let package = || {
        RouteHttp::new()
            .json(
                "/lodash",
                json!({"name": "lodash", "dist-tags": {"latest": "6.4.5"},
                    "versions": {"5.4.0": {"name": "lodash", "version": "5.4.0"}}}),
            )
            .json(
                "/lodash/5.4.0",
                json!({"name": "lodash", "version": "5.4.0"}),
            )
            .osv_pages(vec![
                (
                    200,
                    json!({"vulns": [fixed("GHSA-OWN", "HIGH", &["6.4.4"])]}),
                ),
                (
                    200,
                    json!({"vulns": [fixed("GHSA-AT-CANDIDATE", "LOW", &["6.4.5"])]}),
                ),
                (200, json!({})),
            ])
    };
    let query = |view: &str| {
        json!({"operation": "package", "ecosystem": "npm", "packageName": "lodash",
            "version": "5.4.0", "view": view})
    };
    let details = answer(&package(), query("details")).await.expect("details");
    assert_eq!(details["upgradeTo"], "6.4.5", "{details}");
    let note = details["upgradeNote"].as_str().expect("note");
    assert!(
        note.starts_with("No 5.x release is listed as fixing GHSA-OWN;"),
        "{note}"
    );
    assert!(
        note.contains("Skipped 6.4.4 (OSV lists GHSA-AT-CANDIDATE at it)."),
        "{note}"
    );
    assert_eq!(note.matches("GHSA-AT-CANDIDATE").count(), 1, "{note}");
    let summary = answer(&package(), query("summary")).await.expect("summary");
    assert_eq!(summary["unfixedInMajorCount"], 1, "{summary}");
}

/// A dispute changes neither the verdict, the count nor the upgrade, so the
/// summary does not read each alias;`details` does, and names the dispute.
#[tokio::test]
async fn a_summary_skips_the_alias_reads_that_only_details_states() {
    let aliased = || {
        let other = json!({"id": "PYSEC-1", "affected": [{"package": {"name": "lodash", "ecosystem": "npm"}}]});
        let mut listed = fixed("GHSA-HIGHER", "HIGH", &["4.17.21"]);
        listed["aliases"] = json!(["PYSEC-1"]);
        registry()
            .json("/v1/vulns/PYSEC-1", other)
            .osv_pages(vec![(200, json!({"vulns": [listed]})), (200, json!({}))])
    };
    let alias_reads = |http: &RouteHttp| {
        http.targets()
            .iter()
            .filter(|target| target.ends_with("/v1/vulns/PYSEC-1"))
            .count()
    };
    let http = aliased();
    let summary = answer(&http, package_query("summary"))
        .await
        .expect("summary");
    assert_eq!(alias_reads(&http), 0, "{:?}", http.targets());
    let http = aliased();
    let details = answer(&http, package_query("details"))
        .await
        .expect("details");
    assert_eq!(alias_reads(&http), 1, "{:?}", http.targets());
    assert_eq!(
        details["advisories"][0]["disputedBy"],
        json!(["PYSEC-1"]),
        "{details}"
    );
    // The same verdict, count and upgrade either way.
    assert_eq!(summary["affected"], details["affected"]);
    assert_eq!(summary["upgradeTo"], details["upgradeTo"]);
    assert_eq!(
        summary["advisoryCount"],
        details["advisories"]
            .as_array()
            .map(Vec::len)
            .expect("list")
    );
}

/// A URL states no coordinates as fields: the summary names the package and
/// version it resolved to.
#[tokio::test]
async fn a_url_summary_is_the_package_summary() {
    let query = |view: &str| json!({"operation": "url", "url": "https://www.npmjs.com/package/lodash", "view": view});
    let http = || registry_with_latest().osv_pages(vec![(200, json!({}))]);
    let summary = answer(&http(), query("summary")).await.expect("summary");
    let details = answer(&http(), query("details")).await.expect("details");
    assert_eq!(summary["affected"], false, "{summary}");
    assert_eq!(summary["advisoryCount"], 0, "{summary}");
    assert_eq!(
        summary["version"], "4.18.1",
        "the version it resolved: {summary}"
    );
    assert_eq!(details["advisories"], json!([]), "{details}");
    assert_details_holds_the_summary(&summary, &details);
    // The lead names the package the URL resolved to.
    let lead = details_call(&summary);
    assert_eq!(lead["operation"], "package", "{lead}");
    assert_eq!(lead["packageName"], "lodash", "{lead}");
    assert_row_contract(ToolId::SecurityCheck, &summary);
}

#[tokio::test]
async fn a_commit_summary_counts_and_links_and_details_lists() {
    let http = || {
        RouteHttp::new().osv_pages(vec![(
            200,
            json!({"vulns": [fixed("OSV-2020-1", "MEDIUM", &[]), fixed("CVE-2023-1", "HIGH", &[])]}),
        )])
    };
    let query = |view: &str| json!({"operation": "commit", "commit": COMMIT, "view": view});
    let summary = answer(&http(), query("summary")).await.expect("summary");
    let details = answer(&http(), query("details")).await.expect("details");
    assert_eq!(
        summary,
        json!({
            "affected": true,
            "advisoryCount": 2,
            "highestSeverity": "high",
            "link": "https://osv.dev/vulnerability/CVE-2023-1",
            "related": [{"kind": "related", "tool": "securityCheck",
                "input": {"queries": [{"operation": "commit", "commit": COMMIT, "view": "details"}]}}],
        })
    );
    assert_eq!(
        details["advisories"].as_array().map(Vec::len),
        Some(2),
        "{details}"
    );
    assert!(details["answer"].is_string(), "{details}");
    assert_details_holds_the_summary(&summary, &details);
    // None known for a commit: a count of 0 is not "clean", so the sentence
    // that says what it does not prove stays.
    let none = RouteHttp::new().osv_pages(vec![(200, json!({}))]);
    let summary = answer(&none, query("summary")).await.expect("summary");
    assert!(summary.get("status").is_none(), "{summary}");
    assert_eq!(summary["advisoryCount"], 0, "{summary}");
    assert!(summary["answer"].is_string(), "{summary}");
    assert!(summary.get("affected").is_none(), "{summary}");
    assert_row_contract(ToolId::SecurityCheck, &summary);
}

/// The walk that stops keeps its view: the next page is the same view's.
#[tokio::test]
async fn a_stopped_walk_reads_on_in_the_view_it_began_with() {
    for (view, carried) in [("summary", Value::Null), ("details", json!("details"))] {
        let http = RouteHttp::new().osv_pages(vec![
            (
                200,
                json!({"vulns": [fixed("OSV-2020-1", "MEDIUM", &[])], "next_page_token": "t2"}),
            ),
            (503, json!({})),
        ]);
        let stopped = answer(
            &http,
            json!({"operation": "commit", "commit": COMMIT, "view": view}),
        )
        .await
        .expect("stopped");
        assert_eq!(stopped["isPartial"], true, "{stopped}");
        assert!(stopped.get("advisoryCount").is_none(), "{stopped}");
        // No verdict yet: the sentence says to read on, in either view.
        assert!(stopped["answer"].is_string(), "{stopped}");
        let next = &stopped["next"]["nextPage"]["input"]["queries"][0];
        assert!(next["resume"].is_string(), "{stopped}");
        // The default view is left unstated, as every emitted default is.
        assert_eq!(next["view"], carried, "{view}: {stopped}");
    }
}

/// One advisory OSV and GitHub both keep: `details` states each source's
/// record whole; the summary states the advisory once, with the ranges and
/// fixes in the answer.
#[tokio::test]
async fn an_advisory_summary_is_one_row_per_advisory() {
    use crate::tools::security_check::GhsaRead;
    use crate::tools::security_check::github_double::Double;
    use crate::tools::security_check::repository::RepoRead;
    let id = "GHSA-aaaa-bbbb-cccc";
    let osv = json!({"id": id, "summary": "Command injection", "details": "Long text.",
        "aliases": ["CVE-2021-23337"],
        "database_specific": {"severity": "HIGH", "cwe_ids": ["CWE-77"]},
        "references": [{"type": "WEB", "url": "https://example.test/ref"}],
        "affected": [{"package": {"name": "x", "ecosystem": "npm"},
            "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "2.0.0"}]}]}]});
    let github = || {
        Double::repo(RepoRead::Missing).ghsa(
            None,
            id,
            GhsaRead::Found(Box::new(github_advisory(id, json!({"severity": "high"})))),
        )
    };
    let http = || RouteHttp::new().json(&format!("/v1/vulns/{id}"), osv.clone());
    let query = |view: &str| json!({"operation": "advisory", "id": id, "view": view});
    let summary = answer_on_github(&http(), &github(), query("summary"))
        .await
        .expect("summary");
    let details = answer_on_github(&http(), &github(), query("details"))
        .await
        .expect("details");
    let whole = details["advisories"].as_array().expect("records");
    assert_eq!(whole.len(), 2, "a record per source: {details}");
    assert_eq!(whole[0]["details"], "Long text.", "{details}");
    assert!(whole[0]["references"].is_array(), "{details}");
    assert_eq!(
        summary["advisories"],
        json!([{
            "id": id,
            "cves": ["CVE-2021-23337"],
            "severity": "high",
            "fixedIn": ["2.0.0"],
            "summary": "Command injection",
            "link": format!("https://osv.dev/vulnerability/{id}"),
        }]),
        "{summary}"
    );
    // The ranges and fixes are the answer, the same sentence in both views.
    assert_eq!(summary["answer"], details["answer"]);
    assert!(
        summary["answer"]
            .as_str()
            .is_some_and(|said| said.contains("x <2.0.0 (fixed 2.0.0)")),
        "{summary}"
    );
    assert!(summary.get("sources").is_none(), "{summary}");
    assert_eq!(
        summary["related"],
        json!([{"kind": "related", "tool": "securityCheck",
            "input": {"queries": [{"operation": "advisory", "id": id, "view": "details"}]}}])
    );
    assert_row_contract(ToolId::SecurityCheck, &summary);
    assert_row_contract(ToolId::SecurityCheck, &details);
}

/// A source that failed is named in the summary too: the row is partial.
#[tokio::test]
async fn an_advisory_summary_names_the_source_that_failed() {
    use crate::tools::security_check::GhsaRead;
    use crate::tools::security_check::github_double::Double;
    use crate::tools::security_check::repository::{RepoFailure, RepoRead};
    let id = "GHSA-29mw-wpgm-hmr9";
    let http = RouteHttp::new().json(
        &format!("/v1/vulns/{id}"),
        json!({"id": id, "affected": [{"package": {"name": "lodash", "ecosystem": "npm"}}]}),
    );
    let github = Double::repo(RepoRead::Missing).ghsa(
        None,
        id,
        GhsaRead::Failed(RepoFailure {
            code: ErrorCode::RateLimited,
            why: "GitHub rate limit reached.".into(),
        }),
    );
    let summary = answer_on_github(
        &http,
        &github,
        json!({"operation": "advisory", "id": id, "view": "summary"}),
    )
    .await
    .expect("summary");
    assert_eq!(summary["isPartial"], true, "{summary}");
    assert_eq!(summary["sources"][0]["source"], "github", "{summary}");
    assert_eq!(summary["sources"][0]["status"], "failed", "{summary}");
    assert!(summary["sources"][0]["why"].is_string(), "{summary}");
    assert_eq!(summary["sources"].as_array().map(Vec::len), Some(1));
}
