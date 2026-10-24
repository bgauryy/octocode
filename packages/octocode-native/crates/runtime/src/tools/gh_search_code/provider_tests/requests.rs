//! Requests a search sends or refuses: invalid refs and globs before any request, one request for identical searches.
use super::*;

/// Identical searches in one batch share one request: the second waits for
/// the first and reads the page it stored, so the duplicate spends no
/// code-search quota.
#[tokio::test]
async fn identical_concurrent_searches_send_one_request() {
    let server = MockServer::builder().start().await;
    let search = Delayed::new(
        Duration::from_millis(200),
        ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"app.rs","path":"app.rs","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}}
            ]
        })),
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(search.clone())
        .mount(&server)
        .await;
    let mut provider = one_shot_provider(&server);
    provider.transport.cache = std::sync::Arc::new(MemoryCache::default());
    let query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"a","repo":"b","keywords":["app"],"matchIn":["path"]
    }))
    .expect("query");
    let context = default_context();
    let (first, second) = futures_util::future::join(
        execute(&provider, &query, &context, &Passthrough),
        execute(&provider, &query, &context, &Passthrough),
    )
    .await;
    assert_eq!(
        first.expect("first").data["files"],
        second.expect("second").data["files"]
    );
    assert_eq!(search.arrivals.count(), 1, "one GET /search/code");
}

#[test]
fn canonical_code_scope_is_validated_before_provider_work() {
    for fields in [
        json!({"ref":"dev"}),
        json!({"nameContains":"*.rs"}),
        json!({"pathPrefix":"src/**"}),
    ] {
        let mut row = json!({"owner":"o","keywords":["needle"]});
        for (key, value) in fields.as_object().expect("fields") {
            row[key] = value.clone();
        }
        assert!(crate::contracts::validate("ghSearchCode", json!({"queries":[row]})).is_err());
    }
    crate::contracts::validate("ghSearchCode", json!({"queries":[{"owner":"o","repo":"r","ref":"dev","nameContains":".rs","pathPrefix":"src"}]})).expect("canonical bounded scope");
}
