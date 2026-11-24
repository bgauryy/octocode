use super::*;
use crate::security_check::osv_double::OsvPages;
use octocode_test_support::json_fixture;

/// A gem that bundles its parts (rails over actionpack) carries their
/// advisories under their own names: its own row says none known, so it
/// states the exactly pinned dependencies and leads to a check of each at its
/// pinned version. A requirement that allows other versions is no pin.
#[tokio::test]
async fn an_umbrella_package_leads_to_its_exactly_pinned_dependencies() {
    let http = RouteHttp::new()
        .json(
            "/api/v2/rubygems/rails/versions/8.1.4.1.json",
            json_fixture(
                r#"{
                    "name": "rails",
                    "version": "8.1.4.1",
                    "dependencies": {
                        "runtime": [
                            {"name": "actionpack", "requirements": "= 8.1.4.1"},
                            {"name": "activerecord", "requirements": "= 8.1.4.1"},
                            {"name": "bundler", "requirements": ">= 1.15.0"}
                        ]
                    }
                }"#,
            ),
        )
        .osv_pages(vec![(200, json!({}))]);
    let data = answer_for(
        &http,
        ArtifactType::Rubygems,
        "rails",
        Some("8.1.4.1"),
        None,
    )
    .await
    .expect("row");
    // None known is an answer, not an empty row.
    assert!(data.get("status").is_none(), "{data}");
    assert_eq!(data["affected"], false, "{data}");
    let sentence = data["answer"].as_str().expect("answer");
    assert!(
        sentence.contains("pins 2 runtime dependencies"),
        "{sentence}"
    );
    let call = serde_json::to_string(&data["related"]).expect("call");
    for pinned in ["actionpack", "activerecord"] {
        assert!(
            call.contains(&format!("\"packageName\":\"{pinned}\"")),
            "{call}"
        );
    }
    assert!(call.contains("\"version\":\"8.1.4.1\""), "{call}");
    assert!(!call.contains("bundler"), "{call}");
    // A package that pins nothing says nothing of the kind.
    let http = registry().osv_pages(vec![(200, json!({}))]);
    let data = answer(&http, Some("4.17.20"), None).await.expect("row");
    assert!(data.get("related").is_none(), "{data}");
    assert!(!data["answer"].as_str().unwrap_or_default().contains("pins"));
}

/// The summary's `answer` says only what no field of the row does: a
/// verdict, a count, CVEs and an upgrade are fields, so a row that says
/// nothing more has no `answer` (none known is `affected:false`, whose
/// meaning the field states); the clause about pinned dependencies is no
/// field's and stays alone. The details answer keeps the whole sentence.
#[tokio::test]
async fn the_summary_answer_holds_only_what_no_field_states() {
    let summary = octocode_contract_types::SecurityCheckView::Summary;
    let rails = || {
        RouteHttp::new()
            .json(
                "/api/v2/rubygems/rails/versions/8.1.4.1.json",
                json_fixture(
                    r#"{
                        "name": "rails",
                        "version": "8.1.4.1",
                        "dependencies": {"runtime": [{"name": "actionpack", "requirements": "= 8.1.4.1"}]}
                    }"#,
                ),
            )
            .osv_pages(vec![(200, json!({}))])
    };
    let pinned = answer_viewed(
        &rails(),
        ArtifactType::Rubygems,
        "rails",
        Some("8.1.4.1"),
        None,
        summary,
    )
    .await
    .expect("row");
    assert_eq!(pinned["affected"], false, "{pinned}");
    let sentence = pinned["answer"].as_str().expect("the pins clause stays");
    assert!(
        sentence.starts_with("It pins 1 runtime dependency"),
        "{sentence}"
    );
    assert!(!sentence.contains("none known"), "{sentence}");
    // None known and nothing else: the fields say it all.
    let clear = answer_in(
        &registry().osv_pages(vec![(200, json!({}))]),
        Some("4.17.20"),
        summary,
    )
    .await
    .expect("row");
    assert_eq!(clear["affected"], false, "{clear}");
    assert!(clear.get("answer").is_none(), "{clear}");
    // Affected, with its CVE and upgrade: every clause is a field.
    let fixed = json!({"vulns": [{"id": "GHSA-1", "aliases": ["CVE-2021-23337"], "affected": [{
        "package": {"name": "lodash", "ecosystem": "npm"},
        "ranges": [{"type": "SEMVER", "events": [{"introduced": "0"}, {"fixed": "4.18.1"}]}]}]}]});
    let http = registry().osv_pages(vec![(200, fixed.clone()), (200, json!({"vulns": []}))]);
    let affected = answer_in(&http, Some("4.17.20"), summary)
        .await
        .expect("row");
    assert_eq!(affected["upgradeTo"], "4.18.1", "{affected}");
    assert!(affected.get("answer").is_none(), "{affected}");
    let http = registry().osv_pages(vec![(200, fixed), (200, json!({"vulns": []}))]);
    let details = answer(&http, Some("4.17.20"), None).await.expect("row");
    let whole = details["answer"].as_str().expect("details answer");
    assert!(
        whole.contains("CVE-2021-23337") && whole.contains("4.18.1"),
        "{whole}"
    );
}
