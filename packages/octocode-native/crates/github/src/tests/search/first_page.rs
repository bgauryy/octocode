//! GSC-4 and GSH-4: a search's first page asks GitHub for exactly the
//! requested window in one request (`page=1`, `per_page` = the window), not
//! a larger page sliced afterwards.
use super::*;
use octocode_test_support::json_fixture;

async fn one_request_for(server: &MockServer, route: &str, per_page: usize) {
    let requests = server.received_requests().await.expect("recorded");
    assert_eq!(requests.len(), 1, "{route}: {requests:?}");
    let query = requests[0].url.query_pairs().collect::<Vec<_>>();
    let param = |name: &str| {
        query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.to_string())
    };
    assert_eq!(requests[0].url.path(), route);
    assert_eq!(param("page").as_deref(), Some("1"), "{route}");
    assert_eq!(
        param("per_page"),
        Some(per_page.to_string()),
        "{route}: the first page's size is the requested window"
    );
}

async fn empty_search_server() -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"total_count": 0, "incomplete_results": false, "items": []}"#,
        )))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_code_search_first_page_asks_for_the_requested_window() {
    let server = empty_search_server().await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    let mut request = code_search_request();
    request.per_page = 7;
    transport
        .search_code(&request, &context)
        .await
        .expect("code search");
    one_request_for(&server, "/api/v3/search/code", 7).await;
}

#[tokio::test]
async fn a_history_search_first_page_asks_for_the_requested_window() {
    let server = empty_search_server().await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    transport
        .search_issues(
            &HistoryRequest {
                query: "repo:o/r".into(),
                page: 1,
                per_page: 7,
                sort: None,
                order: None,
            },
            &context,
        )
        .await
        .expect("issues");
    one_request_for(&server, "/api/v3/search/issues", 7).await;
}
