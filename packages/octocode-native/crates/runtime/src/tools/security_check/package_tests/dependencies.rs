use super::*;
use crate::tools::security_check::osv_double::OsvPages;

/// A gem that bundles its parts (rails over actionpack) carries their
/// advisories under their own names: its own row says none known, so it
/// states the exactly pinned dependencies and leads to a check of each at its
/// pinned version. A requirement that allows other versions is no pin.
#[tokio::test]
async fn an_umbrella_package_leads_to_its_exactly_pinned_dependencies() {
    let http = RouteHttp::new()
        .json(
            "/api/v2/rubygems/rails/versions/8.1.4.1.json",
            json!({
                "name": "rails", "version": "8.1.4.1",
                "dependencies": {"runtime": [
                    {"name": "actionpack", "requirements": "= 8.1.4.1"},
                    {"name": "activerecord", "requirements": "= 8.1.4.1"},
                    {"name": "bundler", "requirements": ">= 1.15.0"}
                ]}
            }),
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
    assert_eq!(data["status"], "empty");
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
