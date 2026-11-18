use crate::{
    CredentialSource, GitHubEndpoint, GitHubTransport, RequestContext, ResolvedCredential,
    RetryPolicy,
};
use octocode_io::budget::RequestBudget;
use octocode_test_support::json_fixture;
use std::time::Duration;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// A list's `Link` header names its last page: the read reports it, and
/// reports none for a page that links no `last`.
#[tokio::test]
async fn a_history_read_reports_the_last_page_its_link_header_names() {
    let server = MockServer::builder().start().await;
    let uri = server.uri();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits/abc"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    format!(
                        "<{uri}/api/v3/repos/o/r/commits/abc?per_page=300&page=2>; rel=\"next\", \
                 <{uri}/api/v3/repos/o/r/commits/abc?per_page=300&page=10>; rel=\"last\""
                    ),
                )
                .set_body_json(serde_json::json!({})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits/def"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;
    let endpoint = GitHubEndpoint::new(url::Url::parse(&format!("{uri}/api/v3")).unwrap()).unwrap();
    let transport = GitHubTransport::new(
        endpoint,
        RetryPolicy {
            max_attempts: 1,
            base_delay: Duration::from_millis(1),
        },
        crate::GitHubBudget::relaxed(),
    )
    .unwrap();
    let context = || {
        RequestContext::new(
            RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
            Some(ResolvedCredential::new(
                "last",
                CredentialSource::Environment,
            )),
        )
    };
    let linked = transport
        .history_item(&["repos", "o", "r", "commits", "abc"], &[], &context())
        .await
        .expect("linked page");
    assert!(linked.has_more);
    assert_eq!(linked.last_page, Some(10));
    let single = transport
        .history_item(&["repos", "o", "r", "commits", "def"], &[], &context())
        .await
        .expect("single page");
    assert!(!single.has_more);
    assert_eq!(single.last_page, None);
}

/// Rows of one batch each read the current pull request.
#[tokio::test]
async fn concurrent_reads_of_one_item_send_independent_requests() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/pulls/9"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_delay(Duration::from_millis(150))
                .set_body_json(json_fixture(r#"{"number": 9}"#)),
        )
        .expect(3)
        .mount(&server)
        .await;
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).unwrap()).unwrap();
    let transport = GitHubTransport::new(
        endpoint,
        RetryPolicy {
            max_attempts: 1,
            base_delay: Duration::from_millis(1),
        },
        crate::GitHubBudget::relaxed(),
    )
    .unwrap();
    let context = || {
        RequestContext::new(
            RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
            Some(ResolvedCredential::new(
                "flight",
                CredentialSource::Environment,
            )),
        )
    };
    let read = || async {
        transport
            .history_item(&["repos", "o", "r", "pulls", "9"], &[], &context())
            .await
            .expect("pull request")
    };
    let (first, second, third) = tokio::join!(read(), read(), read());
    for item in [first, second, third] {
        assert_eq!(item.value["number"], 9);
    }
}
