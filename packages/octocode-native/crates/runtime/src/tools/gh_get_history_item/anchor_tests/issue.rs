//! Issue rows: number, state, author and discussion size; a pull-request number is not an issue; comment pages keep the caller's page size.
use super::*;

#[tokio::test]
async fn issue_row_carries_number_state_author_and_discussion_size() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/7",
        200,
        json!({"number":7,"title":"Crash","state":"open","user":{"login":"bob"},
            "body":"It crashes.","comments":3,"created_at":"2024-01-01T00:00:00Z",
            "updated_at":"2024-01-02T00:00:00Z"}),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"issue","owner":"o","repo":"r","number":7}),
    )
    .await;
    let issue = &out["issues"][0];
    assert_eq!(issue["number"], 7, "{out}");
    assert_eq!(issue["state"], "open", "{out}");
    assert_eq!(issue["author"], "bob", "{out}");
    assert_eq!(issue["commentsCount"], 3, "{out}");
    assert_eq!(issue["body"], "It crashes.", "{out}");
    let discussion = checks::related_call(&out, "ghGetHistoryItem", |input| {
        input["queries"][0]["sections"] == json!(["comments"])
    });
    assert_eq!(discussion["input"]["queries"][0]["number"], 7);
    assert!(out.get("next").is_none(), "{out}");
}

/// The wrong operation for an existing number is one failure class both
/// ways: not found as the kind asked for, typed by its reason.
#[tokio::test]
async fn an_issue_read_of_a_pull_request_number_is_not_found() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/9",
        200,
        json!({"number":9,"title":"PR","state":"open","user":{"login":"bob"},
            "pull_request":{"url":"https://x"},"comments":0}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = fixture_context(Duration::from_secs(10), 4 << 20);
    let request = HistoryItemRequest::from_row(
        json!({"operation":"issue","owner":"o","repo":"r","number":9}),
    )
    .expect("valid row");
    let Err(error) = execute(&provider, &request, &context, &Passthrough).await else {
        panic!("an issue read of a pull request number fails");
    };
    assert_eq!(error.kind, octocode_github::ProviderErrorKind::NotFound);
    assert_eq!(
        error.reason,
        Some(octocode_github::ProviderErrorReason::IssueIsPullRequest)
    );
    assert_eq!(&*error.message, "#9 is a pull request, not an issue");
}

/// An explicit `pageSize` bounds every issue comment page, not only the
/// first: the walk keeps the caller's grid, the page block states no
/// resume size, and every comment arrives once, in order.
#[tokio::test]
async fn issue_comment_pages_keep_the_callers_page_size() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/7",
        200,
        json!({"number":7,"title":"Thread","state":"open","user":{"login":"bob"},
            "body":"repro","comments":11,"created_at":"2024-01-01T00:00:00Z",
            "updated_at":"2024-01-02T00:00:00Z"}),
    )
    .await;
    let comments = (0..11)
        .map(|n| {
            json!({"id": n, "user": {"login": "amy"}, "body": format!("comment {n}"),
            "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-01T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/issues/7/comments"))
        .respond_with(paged_like_github(server.uri(), comments, Value::Array))
        .mount(&server)
        .await;
    let mut query = json!({"operation":"issue","owner":"o","repo":"r","number":7,
        "sections":["comments"],"pageSize":4});
    let mut ids = Vec::new();
    for call in 1.. {
        assert!(call <= 4, "walk did not finish");
        let data = read(&server, query.clone()).await;
        let issue = &data["issues"][0];
        let shown = issue["comments"].as_array().cloned().unwrap_or_default();
        assert!(shown.len() <= 4, "call {call}: {data}");
        let block = &issue["contentPagination"]["comments"];
        assert!(block.get("pageSize").is_none(), "call {call}: {data}");
        ids.extend(shown.iter().map(|comment| comment["id"].to_string()));
        let Some(next) = data.pointer("/next/nextCommentPage/input/queries/0") else {
            break;
        };
        assert_eq!(next["pageSize"], 4, "call {call}: {data}");
        assert_eq!(next["page"], call + 1, "call {call}: {data}");
        query = next.clone();
    }
    let expected = (0..11).map(|n| json!(n.to_string()).to_string());
    assert_eq!(ids, expected.collect::<Vec<_>>());
}
