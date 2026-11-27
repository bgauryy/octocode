//! GitHub serves an issue or repository search page while its first
//! result lies inside the 1,000-result cap, so the last page runs past it
//! (`per_page=30&page=34` answers results 991-1020, `per_page=100&page=11`
//! is refused). A caller page that runs past the cap is GitHub's own answer,
//! read as asked: a provider page of 100 cannot reach its last rows.
use super::*;
use wiremock::matchers::query_param;

const CAP_REFUSAL: &str = "Only the first 1000 search results are available";

/// A search of 2,000 matches whose items are numbered by their position:
/// GitHub's answer at `page`/`per_page`, refused for a page that starts past
/// the cap.
async fn capped_search_server() -> MockServer {
    let server = MockServer::builder().start().await;
    for (page, per_page) in [(34_usize, 30_usize), (10, 100)] {
        let start = (page - 1) * per_page;
        let items = (start..start + per_page)
            .map(|position| serde_json::json!({"number": position + 1}))
            .collect::<Vec<_>>();
        Mock::given(method("GET"))
            .and(path("/api/v3/search/issues"))
            .and(query_param("page", page.to_string()))
            .and(query_param("per_page", per_page.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": 2000, "incomplete_results": false, "items": items
            })))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(422).set_body_json(serde_json::json!({"message": CAP_REFUSAL})),
        )
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn the_last_page_of_a_search_serves_the_rows_past_the_cap() {
    let server = capped_search_server().await;
    let (transport, _) = executor_transport(&server, GitHubBudget::relaxed(), short_retry());
    let context = with_token("t", Duration::from_secs(5), 1 << 20);
    let page = transport
        .search_issues(
            &HistoryRequest {
                query: "repo:o/r".into(),
                page: 34,
                per_page: 30,
                sort: None,
                order: None,
            },
            &context,
        )
        .await
        .expect("the last page");
    let numbers = page
        .items
        .iter()
        .map(|item| item["number"].as_u64().expect("number"))
        .collect::<Vec<_>>();
    assert_eq!(
        numbers,
        (991..=1020).collect::<Vec<_>>(),
        "the page GitHub serves at page 34 of 30"
    );
}
