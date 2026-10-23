//! Results SAML enforcement withheld: the organizations named, and no cached copy.
use super::*;

/// `partial-results` names the organizations that withheld results.
#[tokio::test]
async fn partial_sso_results_name_the_withheld_organizations() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "x-github-sso",
                    "partial-results; organizations=21955855,20582480",
                )
                .set_body_json(serde_json::json!({"items": []})),
        )
        .mount(&server)
        .await;
    let (transport, endpoint) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let page = get_status(&transport, &endpoint, &["search", "issues"])
        .await
        .expect("ok");
    assert_eq!(page.sso_partial_organizations(), vec![21955855, 20582480]);
    let plain = ResponsePage {
        status: 200,
        headers: reqwest::header::HeaderMap::new(),
        body: bytes::Bytes::new(),
        next: None,
    };
    assert!(plain.sso_partial_organizations().is_empty());
}

/// A search the token only partly covered says so, and is not
/// cached (a cached copy would hide the authorization that fixes it).
#[tokio::test]
async fn a_search_partly_withheld_by_saml_names_the_organizations_and_is_not_cached() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "x-github-sso",
                    "partial-results; organizations=21955855,20582480",
                )
                .set_body_json(serde_json::json!({
                    "total_count": 0, "incomplete_results": false, "items": []
                })),
        )
        .expect(4)
        .mount(&server)
        .await;
    let (mut transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    transport.cache = Arc::new(PartitionedCache::default());
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    for _ in 0..2 {
        let page = transport
            .search_code(&code_search_request(), &context)
            .await
            .expect("code search");
        assert_eq!(page.sso_partial_organizations, vec![21955855, 20582480]);
    }
    for _ in 0..2 {
        let history = transport
            .search_issues(
                &HistoryRequest {
                    query: "repo:o/r".into(),
                    page: 1,
                    per_page: 10,
                    sort: None,
                    order: None,
                },
                &context,
            )
            .await
            .expect("issues");
        assert!(
            history
                .warnings
                .iter()
                .any(|warning| warning.contains("SAML")),
            "{:?}",
            history.warnings
        );
    }
}
