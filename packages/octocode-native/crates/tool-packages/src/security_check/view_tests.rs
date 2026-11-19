//! Each operation in both views, against recorded doubles: the summary states
//! what decides, `details` states the whole row, and the summary's lead is the
//! call that returns it.
use super::*;
use crate::security_check::summary::DETAIL_FIELDS;
use octocode_runtime_base::test_support::assert_row_contract;
use octocode_test_support::json_fixture;

/// The fields a summary computes from the list it leaves to `details`.
const SUMMARY_ONLY: [&str; 5] = [
    "advisoryCount",
    "advisoryIds",
    "highestSeverity",
    "unfixedInMajorCount",
    "link",
];
/// Fields a summary narrows or states only when they say something new.
const NARROWED: [&str; 7] = [
    "answer",
    "upgradeNote",
    "packageName",
    "version",
    "sources",
    "related",
    "outdated",
];

/// The tool call's result for `query` as a caller receives it: the row after
/// the response stage, not the executor's internal row.
async fn serialized(http: &RouteHttp, query: Value) -> Value {
    let row = answer(http, query).await.expect("row");
    octocode_runtime_base::test_support::published(ToolId::SecurityCheck, &row)["results"][0]["data"].clone()
}

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
            details.get(field).is_some() || SUMMARY_ONLY.contains(&field),
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

/// A summary offers no lead that restates its query as `view:"details"`:
/// the view is named in the tool description.
fn assert_no_details_lead(summary: &Value) {
    assert!(
        summary["related"].as_array().is_none_or(|calls| calls
            .iter()
            .all(|call| call["input"]["queries"][0]["view"] != "details")),
        "{summary}"
    );
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
    let latest = json_fixture(r#"{"name": "lodash", "version": "4.18.1"}"#);
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
    // `outdated:true` is derived from the release the row names: the
    // `latestVersion` stays only when it is not the `upgradeTo` already stated.
    assert!(summary.get("outdated").is_none(), "{summary}");
    assert_eq!(details["outdated"], true, "{details}");
    assert_ne!(summary["latestVersion"], summary["upgradeTo"], "{summary}");
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
    assert_no_details_lead(&summary);
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
                json_fixture(
                    r#"{
                        "name": "lodash",
                        "dist-tags": {"latest": "6.4.5"},
                        "versions": {
                            "5.4.0": {"name": "lodash", "version": "5.4.0"},
                            "6.4.5": {"name": "lodash", "version": "6.4.5"}
                        }
                    }"#,
                ),
            )
            .json(
                "/lodash/5.4.0",
                json_fixture(r#"{"name": "lodash", "version": "5.4.0"}"#),
            )
            .json(
                "/lodash/6.4.5",
                json_fixture(r#"{"name": "lodash", "version": "6.4.5"}"#),
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
        let mut listed = fixed("GHSA-HIGHER", "HIGH", &["4.17.21"]);
        listed["aliases"] = json_fixture(r#"["PYSEC-1"]"#);
        // The details view alone lists the package's records by name, a third
        // request, to tell which alias OSV keeps for it.
        registry().osv_pages(vec![
            (200, json!({"vulns": [listed]})),
            (200, json!({})),
            (200, json_fixture(r#"{"vulns": [{"id": "PYSEC-1"}]}"#)),
        ])
    };
    let alias_reads = |http: &RouteHttp| http.post_bodies().len().saturating_sub(2);
    let http = aliased();
    let summary = answer(&http, package_query("summary"))
        .await
        .expect("summary");
    assert_eq!(alias_reads(&http), 0, "{:?}", http.post_bodies());
    let http = aliased();
    let details = answer(&http, package_query("details"))
        .await
        .expect("details");
    assert_eq!(alias_reads(&http), 1, "{:?}", http.post_bodies());
    assert!(
        http.targets()
            .iter()
            .all(|target| !target.contains("PYSEC-1")),
        "no alias is read by id: {:?}",
        http.targets()
    );
    assert_eq!(
        details["advisories"][0]["disputedBy"],
        json_fixture(r#"["PYSEC-1"]"#),
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
    assert_no_details_lead(&summary);
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
    let query = |view: &str| json!({"operation": "commit", "commitSha": COMMIT, "view": view});
    let summary = answer(&http(), query("summary")).await.expect("summary");
    let details = answer(&http(), query("details")).await.expect("details");
    assert_eq!(
        summary,
        json_fixture(
            r#"{
                "affected": true,
                "advisoryCount": 2,
                "advisoryIds": ["CVE-2023-1", "OSV-2020-1"],
                "highestSeverity": "high",
                "link": "https://osv.dev/vulnerability/CVE-2023-1"
            }"#
        )
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
            json!({"operation": "commit", "commitSha": COMMIT, "view": view}),
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
    use crate::security_check::GhsaRead;
    use crate::security_check::github_double::Double;
    use crate::security_check::repository::RepoRead;
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
            GhsaRead::Found(Box::new(github_advisory(
                id,
                json_fixture(r#"{"severity": "high"}"#),
            ))),
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
    assert_no_details_lead(&summary);
    assert_row_contract(ToolId::SecurityCheck, &summary);
    assert_row_contract(ToolId::SecurityCheck, &details);
}

/// A source that failed is named in the summary too: the row is partial.
#[tokio::test]
async fn an_advisory_summary_names_the_source_that_failed() {
    use crate::security_check::GhsaRead;
    use crate::security_check::github_double::Double;
    use crate::security_check::repository::{RepoFailure, RepoRead};
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

/// A purl naming `ubuntu-22.04` reads OSV records stated for `Ubuntu:22.04:LTS`
/// as the asked release's own: their fix and ranges are served, and a record
/// for the Pro track of the same release is another release's.
#[tokio::test]
async fn a_distro_release_matches_the_lts_track_osv_states() {
    let http = RouteHttp::new().osv_pages(vec![(
        200,
        json_fixture(
            r#"{
                "vulns": [
                    {
                        "id": "UBUNTU-CVE-2022-3602",
                        "affected": [
                            {
                                "package": {"name": "openssl", "ecosystem": "Ubuntu:22.04:LTS"},
                                "ranges": [
                                    {
                                        "type": "ECOSYSTEM",
                                        "events": [{"introduced": "0"}, {"fixed": "3.0.2-0ubuntu1.7"}]
                                    }
                                ]
                            },
                            {
                                "package": {"name": "openssl", "ecosystem": "Ubuntu:Pro:22.04:LTS"},
                                "ranges": [
                                    {
                                        "type": "ECOSYSTEM",
                                        "events": [{"introduced": "0"}, {"fixed": "3.0.2-0ubuntu1.99"}]
                                    }
                                ]
                            }
                        ]
                    }
                ]
            }"#,
        ),
    )]);
    let data = serialized(
        &http,
        json_fixture(
            r#"{
                "operation": "url",
                "view": "details",
                "url": "pkg:deb/ubuntu/openssl@3.0.2-0ubuntu1?distro=ubuntu-22.04"
            }"#,
        ),
    )
    .await;
    // The record is stated whole: its LTS track's range with that fix (the
    // range states it, so no `fixedIn` repeats it), and not the Pro track's.
    let row = &data["advisories"][0];
    assert_eq!(
        row["affected"][0]["ecosystem"], "Ubuntu:22.04:LTS",
        "{data}"
    );
    assert_eq!(
        row["affected"][0]["ranges"][0]["events"][1]["fixed"], "3.0.2-0ubuntu1.7",
        "{data}"
    );
    assert!(
        !data.to_string().contains("1.99") || row["alsoAffects"].is_array(),
        "{data}"
    );
}

/// An advisory that affects two packages with different fixes states no
/// `fixedIn` on its row, since one list would mix them; the answer names each
/// package's fix. A record about one package keeps `fixedIn`.
#[tokio::test]
async fn an_advisory_of_two_packages_states_no_mixed_fixed_in() {
    let id = "GHSA-jfh8-c2jp-5v3q";
    let affects = |name: &str, fixed: &str| {
        json!({"package": {"name": name, "ecosystem": "Maven"},
            "ranges": [{"type": "ECOSYSTEM", "events": [{"introduced": "0"}, {"fixed": fixed}]}]})
    };
    let record = |entries: Vec<Value>| json!({"id": id, "summary": "s", "affected": entries});
    let http = RouteHttp::new().json(
        &format!("/v1/vulns/{id}"),
        record(vec![
            affects("org.apache.logging.log4j:log4j-core", "2.15.0"),
            affects("org.ops4j.pax.logging:pax-logging-log4j2", "1.11.10"),
        ]),
    );
    let served = serialized(
        &http,
        json!({"operation": "advisory", "id": id, "view": "summary"}),
    )
    .await;
    assert!(served["advisories"][0].get("fixedIn").is_none(), "{served}");
    let said = served["answer"].as_str().expect("answer");
    assert!(
        said.contains("2.15.0") && said.contains("1.11.10"),
        "{said}"
    );
    let one = RouteHttp::new().json(
        &format!("/v1/vulns/{id}"),
        record(vec![affects("org.a:b", "2.15.0")]),
    );
    let served = serialized(
        &one,
        json!({"operation": "advisory", "id": id, "view": "summary"}),
    )
    .await;
    assert_eq!(
        served["advisories"][0]["fixedIn"],
        json_fixture(r#"["2.15.0"]"#),
        "{served}"
    );
}

/// The summary keeps the note only where it is the one reason an affected row
/// names no upgrade; a row that names its `upgradeTo` leaves the note to
/// `details`.
#[tokio::test]
async fn the_summary_states_why_an_affected_row_names_no_upgrade() {
    let http = RouteHttp::new().osv_pages(vec![(
        200,
        json_fixture(
            r#"{
                "vulns": [
                    {
                        "id": "UBUNTU-CVE-1",
                        "affected": [
                            {
                                "package": {"name": "openssl", "ecosystem": "Ubuntu:22.04:LTS"},
                                "ranges": [
                                    {
                                        "type": "ECOSYSTEM",
                                        "events": [{"introduced": "0"}, {"fixed": "3.0.2-0ubuntu1.7"}]
                                    }
                                ]
                            }
                        ]
                    }
                ]
            }"#,
        ),
    )]);
    let served = serialized(
        &http,
        json_fixture(
            r#"{
                "operation": "url",
                "view": "summary",
                "url": "pkg:deb/ubuntu/openssl@3.0.2-0ubuntu1?distro=ubuntu-22.04"
            }"#,
        ),
    )
    .await;
    assert_eq!(served["affected"], true, "{served}");
    assert!(served.get("upgradeTo").is_none(), "{served}");
    assert!(served["upgradeNote"].is_string(), "{served}");
    let with_upgrade = serialized(&affected_package(), package_query("summary")).await;
    assert!(with_upgrade["upgradeTo"].is_string(), "{with_upgrade}");
    assert!(with_upgrade.get("upgradeNote").is_none(), "{with_upgrade}");
}

/// `details` states each fact once: no advisory link that is the OSV page its
/// `id` names, no `isLatest` beside `outdated`, no repository `url`; every id,
/// severity and fix stays.
#[tokio::test]
async fn details_states_nothing_its_other_fields_derive() {
    let summary = serialized(&affected_package(), package_query("summary")).await;
    let details = serialized(&affected_package(), package_query("details")).await;
    let listed = details["advisories"].as_array().expect("advisories");
    assert_eq!(listed.len(), 3, "{details}");
    for row in listed {
        assert!(row["id"].is_string() && row.get("link").is_none(), "{row}");
    }
    assert!(details.get("isLatest").is_none(), "{details}");
    assert_eq!(details["outdated"], true, "{details}");
    assert!(
        listed.iter().any(|row| row["severity"] == "high"),
        "{details}"
    );
    // The summary's one reference is the OSV page of the worst advisory.
    assert_eq!(
        summary["link"], "https://osv.dev/vulnerability/GHSA-HIGHER",
        "{summary}"
    );
}

/// Advisory text is third-party text: after the summary's ids, the distro
/// clause stated once and the call share, every served leaf still passes the
/// output gate. A secret-shaped string in an OSV record's id, summary,
/// details and reference is redacted in both views of a served batch, and
/// in a distribution row with its once-stated warning.
#[tokio::test]
async fn a_secret_in_advisory_text_is_redacted_in_both_views() {
    let secret = format!("ghp_{}", "a1B2c3D4e5".repeat(4));
    let leaky = || {
        let mut record = fixed(&format!("GHSA-{secret}"), "HIGH", &["4.17.21"]);
        record["summary"] = json!(format!("token {secret} in one line"));
        record["details"] = json!(format!("Set GITHUB_TOKEN={secret} to reproduce."));
        record["references"] =
            json!([{"type": "WEB", "url": format!("https://example.test/{secret}")}]);
        registry_with_latest().osv_pages(vec![(200, json!({"vulns": [record]})), (200, json!({}))])
    };
    let summary = answer(&leaky(), package_query("summary"))
        .await
        .expect("summary");
    let details = answer(&leaky(), package_query("details"))
        .await
        .expect("details");
    assert!(
        summary.to_string().contains(&secret) && summary.get("advisoryIds").is_some(),
        "the executor row holds the record as OSV gave it: {summary}"
    );
    let distro = answer(
        &RouteHttp::new().osv_pages(vec![(
            200,
            json!({"vulns": [{"id": "DSA-1", "summary": format!("key {secret}"), "affected": [
                {"package": {"name": "zlib", "ecosystem": "Debian:12"}}]}]}),
        )]),
        json!({"operation": "package", "ecosystem": "Debian", "packageName": "zlib", "release": "12"}),
    )
    .await
    .expect("distro");
    let served = octocode_runtime_base::test_support::staged_response(
        ToolId::SecurityCheck,
        vec![summary, details, distro],
    );
    let text = served.to_string();
    assert!(!text.contains(&secret), "{served}");
    assert!(text.contains("REDACTED"), "{served}");
    assert!(
        served["results"][0]["data"]["advisoryIds"].is_array(),
        "{served}"
    );
}
