//! What a row says to upgrade to, and when it names nothing.
use super::tests::{answer, record, registry};
use crate::security_check::osv_double::OsvPages;
use octocode_contract_types::ErrorCode;
use octocode_test_support::json_fixture;
use serde_json::json;

/// No fix named by a record: said in words, never a guess.
#[tokio::test]
async fn no_upgrade_is_named_when_the_data_does_not_support_one() {
    let http = registry().osv_pages(vec![(200, json!({"vulns": [record("GHSA-1")]}))]);
    let data = answer(&http, Some("4.17.20"), None).await.expect("row");
    assert!(data.get("upgradeTo").is_none(), "{data}");
    assert!(
        data["upgradeNote"]
            .as_str()
            .unwrap_or("")
            .contains("GHSA-1 states no fixed version"),
        "{data}"
    );
    assert_eq!(http.osv_queries().len(), 1, "nothing to confirm");
    // OSV still listing an advisory at the candidate names none.
    let still = |version: &str| {
        json!({"id": "GHSA-1", "affected": [{
        "package": {"name": "lodash", "ecosystem": "npm"},
        "ranges": [{"type": "SEMVER", "events": [{"fixed": version}]}]}]})
    };
    let http = registry().osv_pages(vec![
        (200, json!({"vulns": [still("4.17.21")]})),
        (200, json!({"vulns": [still("4.17.21")]})),
    ]);
    let data = answer(&http, Some("4.17.20"), None).await.expect("row");
    assert!(data.get("upgradeTo").is_none(), "{data}");
    assert!(
        data["upgradeNote"]
            .as_str()
            .unwrap_or("")
            .contains("still lists GHSA-1"),
        "{data}"
    );
    // Others appearing at the candidate are said beside it.
    let http = registry().osv_pages(vec![
        (200, json!({"vulns": [still("4.17.21")]})),
        (200, json_fixture(r#"{"vulns": [{"id": "GHSA-9"}]}"#)),
    ]);
    let data = answer(&http, Some("4.17.20"), None).await.expect("row");
    assert_eq!(data["upgradeTo"], "4.17.21");
    assert!(
        data["upgradeNote"]
            .as_str()
            .unwrap_or("")
            .contains("GHSA-9"),
        "{data}"
    );
    // A failed confirmation names none.
    let http = registry()
        .osv_pages(vec![(200, json!({"vulns": [still("4.17.21")]}))])
        .post_fails_from(1, ErrorCode::ProviderError);
    let data = answer(&http, Some("4.17.20"), None).await.expect("row");
    assert!(data.get("upgradeTo").is_none(), "{data}");
    assert!(
        data["upgradeNote"]
            .as_str()
            .unwrap_or("")
            .contains("could not confirm"),
        "{data}"
    );
}

/// Another advisory at the candidate moves it past that one's fix, so the version
/// named is one OSV knows none at.
#[tokio::test]
async fn the_upgrade_moves_past_advisories_found_at_the_candidate() {
    let fixed = |id: &str, version: &str| {
        json!({"id": id, "affected": [{
            "package": {"name": "lodash", "ecosystem": "npm"},
            "ranges": [{"type": "SEMVER", "events": [{"fixed": version}]}]
        }]})
    };
    let http = registry().osv_pages(vec![
        (200, json!({"vulns": [fixed("GHSA-1", "4.18.0")]})),
        (200, json!({"vulns": [fixed("GHSA-2", "4.18.1")]})),
        (200, json!({})),
    ]);
    let data = answer(&http, Some("4.17.20"), None).await.expect("row");
    assert_eq!(data["upgradeTo"], "4.18.1");
    // The skipped candidate is said apart; GHSA-2 does not affect 4.17.20.
    assert_eq!(
        data["upgradeNote"], "Skipped 4.18.0 (OSV lists GHSA-2 at it).",
        "{data}"
    );
    let asked = http
        .osv_queries()
        .iter()
        .map(|body| body["version"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(asked, ["4.17.20", "4.18.0", "4.18.1"]);
}

/// A later-page failure at the proposed version is unfinished verification,
/// even when that source withholds its findings until the walk completes.
#[tokio::test]
async fn an_incomplete_candidate_check_never_names_an_upgrade() {
    let fixed = json_fixture(
        r#"{
            "id": "GHSA-1",
            "affected": [
                {
                    "package": {"name": "lodash", "ecosystem": "npm"},
                    "ranges": [
                        {
                            "type": "SEMVER",
                            "events": [{"introduced": "0"}, {"fixed": "4.18.1"}]
                        }
                    ]
                }
            ]
        }"#,
    );
    for first_candidate_page in [
        json_fixture(r#"{"next_page_token": "remaining"}"#),
        json!({"vulns": [record("GHSA-OTHER")], "next_page_token": "remaining"}),
    ] {
        let http = registry().osv_pages(vec![
            (200, json!({"vulns": [fixed.clone()]})),
            (200, first_candidate_page),
            (503, json!({})),
        ]);
        let data = answer(&http, Some("4.17.20"), None).await.expect("row");
        assert!(data.get("upgradeTo").is_none(), "{data}");
        assert_eq!(data["affected"], true, "{data}");
        assert_eq!(data["advisories"][0]["id"], "GHSA-1", "{data}");
        assert!(
            data["upgradeNote"]
                .as_str()
                .expect("upgrade note")
                .contains("could not confirm"),
            "{data}"
        );
        assert_eq!(http.osv_queries()[1]["version"], "4.18.1");
        assert_eq!(http.osv_queries()[2]["page_token"], "remaining");
    }
}

/// Every confirmed row says where it stands against the registry's latest.
#[tokio::test]
async fn a_row_reports_the_registry_s_latest_and_how_far_behind_it_is() {
    let http = registry().osv_pages(vec![(200, json!({}))]);
    let data = answer(&http, Some("4.17.20"), None).await.expect("row");
    assert_eq!(data["latestVersion"], "4.18.1", "{data}");
    assert!(data.get("isLatest").is_none(), "{data}");
    assert_eq!(data["outdated"], true);
    assert_eq!(data["upgradeKind"], "minor");
    let http = registry().osv_pages(vec![(200, json!({}))]);
    let data = answer(&http, None, None).await.expect("row");
    assert_eq!(data["outdated"], false);
    assert!(data.get("upgradeKind").is_none(), "{data}");
}

/// A fix OSV clears but the registry never published is no upgrade: the row
/// names the next published release (OSV is asked again there).
#[tokio::test]
async fn an_upgrade_the_registry_does_not_publish_steps_to_the_next_release() {
    let packument = json_fixture(
        r#"{
            "name": "lodash",
            "dist-tags": {"latest": "4.17.22"},
            "versions": {
                "4.17.20": {"name": "lodash", "version": "4.17.20"},
                "4.17.22": {"name": "lodash", "version": "4.17.22"}
            }
        }"#,
    );
    let fixed = json_fixture(
        r#"{
            "id": "GHSA-1",
            "affected": [
                {
                    "package": {"name": "lodash", "ecosystem": "npm"},
                    "ranges": [{"type": "SEMVER", "events": [{"fixed": "4.17.21"}]}]
                }
            ]
        }"#,
    );
    let http = crate::artifact::test_http::RouteHttp::new()
        .json("/lodash", packument)
        .json(
            "/lodash/4.17.20",
            json_fixture(r#"{"name": "lodash", "version": "4.17.20"}"#),
        )
        .json(
            "/lodash/4.17.22",
            json_fixture(r#"{"name": "lodash", "version": "4.17.22"}"#),
        )
        .osv_pages(vec![
            (200, json!({"vulns": [fixed]})),
            (200, json!({})),
            (200, json!({})),
        ]);
    let data = answer(&http, Some("4.17.20"), None).await.expect("row");
    assert_eq!(data["upgradeTo"], "4.17.22", "{data}");
    let asked = http
        .osv_queries()
        .iter()
        .map(|body| body["version"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(asked, ["4.17.20", "4.17.21", "4.17.22"]);
}

/// A fix that is the highest the registry lists and is not published names no
/// upgrade and says why.
#[tokio::test]
async fn an_unpublished_highest_fix_names_no_upgrade() {
    let packument = json_fixture(
        r#"{
            "name": "lodash",
            "dist-tags": {"latest": "4.17.20"},
            "versions": {"4.17.20": {"name": "lodash", "version": "4.17.20"}}
        }"#,
    );
    let fixed = json_fixture(
        r#"{
            "id": "GHSA-1",
            "affected": [
                {
                    "package": {"name": "lodash", "ecosystem": "npm"},
                    "ranges": [{"type": "SEMVER", "events": [{"fixed": "4.17.21"}]}]
                }
            ]
        }"#,
    );
    let http = crate::artifact::test_http::RouteHttp::new()
        .json("/lodash", packument)
        .json(
            "/lodash/4.17.20",
            json_fixture(r#"{"name": "lodash", "version": "4.17.20"}"#),
        )
        .osv_pages(vec![(200, json!({"vulns": [fixed]})), (200, json!({}))]);
    let data = answer(&http, Some("4.17.20"), None).await.expect("row");
    assert!(data.get("upgradeTo").is_none(), "{data}");
    assert!(
        data["upgradeNote"]
            .as_str()
            .is_some_and(|note| note.contains("registry")),
        "{data}"
    );
}

/// A lodash registry publishing `versions`, each readable by version.
fn lodash_publishing(versions: &[&str], latest: &str) -> crate::artifact::test_http::RouteHttp {
    let listed = versions
        .iter()
        .map(|version| {
            (
                (*version).to_owned(),
                json!({"name": "lodash", "version": version}),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let packument = json!({
        "name": "lodash",
        "dist-tags": {"latest": latest},
        "versions": listed,
    });
    versions.iter().fold(
        crate::artifact::test_http::RouteHttp::new().json("/lodash", packument),
        |http, version| {
            http.json(
                &format!("/lodash/{version}"),
                json!({"name": "lodash", "version": version}),
            )
        },
    )
}

/// An advisory whose only fix is `version`.
fn fixed_only_in(version: &str) -> serde_json::Value {
    json!({"id": "GHSA-1", "affected": [{
        "package": {"name": "lodash", "ecosystem": "npm"},
        "ranges": [{"type": "SEMVER", "events": [{"fixed": version}]}]}]})
}

fn asked_versions(http: &crate::artifact::test_http::RouteHttp) -> Vec<String> {
    http.osv_queries()
        .iter()
        .map(|body| body["version"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// A fix listed only as a prerelease (`13.4.20-canary.13`) is a fix: a
/// stable version steps to the lowest stable release the registry publishes
/// above it, which OSV is asked about.
#[tokio::test]
async fn a_prerelease_only_fix_steps_to_the_next_stable_release() {
    let http = lodash_publishing(
        &["4.17.20", "4.17.21-canary.13", "4.17.21", "4.18.0"],
        "4.18.0",
    )
    .osv_pages(vec![
        (200, json!({"vulns": [fixed_only_in("4.17.21-canary.13")]})),
        (200, json!({})),
    ]);
    let data = answer(&http, Some("4.17.20"), None).await.expect("row");
    assert_eq!(data["upgradeTo"], "4.17.21", "{data}");
    assert_eq!(asked_versions(&http), ["4.17.20", "4.17.21"], "{data}");
    assert!(
        data["upgradeNote"]
            .as_str()
            .is_some_and(|note| note.contains("4.17.21-canary.13 (a prerelease")),
        "{data}"
    );
}

/// With no stable release above the prerelease fix, the prerelease is
/// named, and the note says only a prerelease fix is listed; the summary
/// keeps that note.
#[tokio::test]
async fn only_a_prerelease_fix_is_named_with_its_note() {
    let http = lodash_publishing(&["4.17.20", "4.17.21-canary.13"], "4.17.20").osv_pages(vec![
        (200, json!({"vulns": [fixed_only_in("4.17.21-canary.13")]})),
        (200, json!({})),
    ]);
    let data = super::tests::answer_in(
        &http,
        Some("4.17.20"),
        octocode_contract_types::SecurityCheckView::Summary,
    )
    .await
    .expect("row");
    assert_eq!(data["upgradeTo"], "4.17.21-canary.13", "{data}");
    assert!(
        data["upgradeNote"]
            .as_str()
            .is_some_and(|note| note.contains("Only a prerelease fix is listed: 4.17.21-canary.13")),
        "{data}"
    );
    assert_eq!(asked_versions(&http), ["4.17.20", "4.17.21-canary.13"]);
}

/// A yanked fix steps once to the next live release: the registry's list of
/// live releases is read once and kept for the walk, so OSV is asked only at
/// the version, the yanked fix and the live release, and no yanked release
/// past the fix is read (futures-util 0.3.6: 0.3.7 through 0.3.30 yanked).
#[tokio::test]
async fn a_yanked_fix_steps_once_to_the_next_live_release() {
    let crate_row = json_fixture(
        r#"{
            "id": "futures-util",
            "name": "futures-util",
            "max_stable_version": "0.3.31",
            "max_version": "0.3.31"
        }"#,
    );
    let version =
        |patch: u32| json!({"num": format!("0.3.{patch}"), "yanked": (7..=30).contains(&patch)});
    let fixed = json_fixture(
        r#"{
            "id": "RUSTSEC-2020-0059",
            "affected": [
                {
                    "package": {"name": "futures-util", "ecosystem": "crates.io"},
                    "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "0.3.7"}]}]
                }
            ]
        }"#,
    );
    let http = crate::artifact::test_http::RouteHttp::routes(vec![
        (
            "/api/v1/crates/futures-util?include=",
            json!({"crate": crate_row}),
        ),
        (
            "/api/v1/crates/futures-util/0.3.6",
            json!({"version": version(6)}),
        ),
        (
            "/api/v1/crates/futures-util/0.3.7",
            json!({"version": version(7)}),
        ),
        (
            "/api/v1/crates/futures-util",
            json!({"crate": crate_row, "versions": (6..=31).rev().map(version).collect::<Vec<_>>()}),
        ),
    ])
    .osv_pages(vec![
        (200, json!({"vulns": [fixed]})),
        (200, json!({})),
        (200, json!({})),
    ]);
    let data = super::tests::answer_for(
        &http,
        octocode_contract_types::ArtifactType::Crates,
        "futures-util",
        Some("0.3.6"),
        None,
    )
    .await
    .expect("row");
    assert_eq!(data["upgradeTo"], "0.3.31", "{data}");
    assert_eq!(
        asked_versions(&http),
        ["0.3.6", "0.3.7", "0.3.31"],
        "{data}"
    );
    let releases_read = http
        .targets()
        .into_iter()
        .filter(|path| path.starts_with("/api/v1/crates/futures-util/0.3."))
        .filter_map(|path| {
            path.trim_start_matches("/api/v1/crates/futures-util/")
                .split(['/', '?'])
                .next()
                .map(str::to_owned)
        })
        .collect::<std::collections::BTreeSet<_>>();
    // The version asked, the yanked fix, and the latest release the row
    // states (`latestVersion`, read beside OSV): no other yanked release.
    assert!(
        releases_read
            .iter()
            .all(|read| ["0.3.6", "0.3.31"].contains(&read.as_str()) || read.starts_with("0.3.7")),
        "no yanked release past the fix is read: {releases_read:?}"
    );
    assert_eq!(data["latestVersion"], "0.3.31", "{data}");
}
