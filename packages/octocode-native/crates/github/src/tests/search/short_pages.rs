//! A caller page that spans two provider pages: a short provider page ends
//! the walk only when GitHub says its results are complete. A short page
//! GitHub marks incomplete (it timed out) is not the end, so the next provider
//! page's rows inside the caller page are still read.
use super::*;
use wiremock::matchers::query_param;

/// A search whose items are numbered by their position: provider page 2 (of
/// 100) answers only its first 40 rows, and page 3 all 100 of its rows. An
/// `incomplete` page 2 is GitHub's timed-out answer; a complete one is the
/// last page of 140 results.
async fn short_second_page_server(incomplete: bool) -> MockServer {
    let server = MockServer::builder().start().await;
    for (page, answered) in [(2_usize, 40_usize), (3, 100)] {
        let start = (page - 1) * 100;
        let items = (start..start + answered)
            .map(|position| serde_json::json!({"number": position + 1}))
            .collect::<Vec<_>>();
        Mock::given(method("GET"))
            .and(path("/api/v3/search/issues"))
            .and(query_param("page", page.to_string()))
            .and(query_param("per_page", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "total_count": if incomplete { 900 } else { 140 },
                "incomplete_results": page == 2 && incomplete,
                "items": items
            })))
            .mount(&server)
            .await;
    }
    server
}

/// Caller page 7 of 30 (rows 181-210), read from provider pages 2 and 3.
async fn seventh_page_of_thirty(server: &MockServer) -> crate::HistoryPage {
    let (transport, _) = executor_transport(server, GitHubBudget::relaxed(), short_retry());
    transport
        .search_issues(
            &HistoryRequest {
                query: "repo:o/r".into(),
                page: 7,
                per_page: 30,
                sort: None,
                order: None,
            },
            &with_token("t", Duration::from_secs(5), 1 << 20),
        )
        .await
        .expect("page 7")
}

fn numbers(page: &crate::HistoryPage) -> Vec<u64> {
    page.items
        .iter()
        .map(|item| item["number"].as_u64().expect("number"))
        .collect()
}

#[tokio::test]
async fn a_short_incomplete_provider_page_does_not_end_the_walk() {
    let server = short_second_page_server(true).await;
    let page = seventh_page_of_thirty(&server).await;
    assert_eq!(
        numbers(&page),
        (201..=210).collect::<Vec<_>>(),
        "the caller page's rows on provider page 3"
    );
    assert!(page.incomplete_results, "the walk read an incomplete page");
    assert_eq!(server.received_requests().await.expect("requests").len(), 2);
}

#[tokio::test]
async fn a_short_complete_provider_page_ends_the_walk() {
    let server = short_second_page_server(false).await;
    let page = seventh_page_of_thirty(&server).await;
    assert_eq!(numbers(&page), Vec::<u64>::new());
    assert!(!page.incomplete_results);
    assert_eq!(
        server.received_requests().await.expect("requests").len(),
        1,
        "a complete short page is the end of the results"
    );
}
