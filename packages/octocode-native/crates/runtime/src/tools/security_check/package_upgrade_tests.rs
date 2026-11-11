//! What a row says to upgrade to, and when it names nothing.
use super::tests::{answer, record, registry};
use crate::tools::security_check::osv_double::OsvPages;
use octocode_contract_types::ErrorCode;
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
        (200, json!({"vulns": [{"id": "GHSA-9"}]})),
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
    let fixed = json!({"id": "GHSA-1", "affected": [{
        "package": {"name": "lodash", "ecosystem": "npm"},
        "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "4.18.1"}]}]
    }]});
    for first_candidate_page in [
        json!({"next_page_token": "remaining"}),
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
    assert_eq!(data["isLatest"], false);
    assert_eq!(data["outdated"], true);
    assert_eq!(data["upgradeKind"], "minor");
    let http = registry().osv_pages(vec![(200, json!({}))]);
    let data = answer(&http, None, None).await.expect("row");
    assert_eq!(data["isLatest"], true, "{data}");
    assert_eq!(data["outdated"], false);
    assert!(data.get("upgradeKind").is_none(), "{data}");
}
