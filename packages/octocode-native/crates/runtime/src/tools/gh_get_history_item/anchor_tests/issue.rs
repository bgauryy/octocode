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
    // A summary previews the body, as a pull request's does; the body read
    // is one call away.
    assert_eq!(issue["bodyPreview"], "It crashes.", "{out}");
    assert!(issue.get("body").is_none(), "{out}");
    let body = checks::related_call(&out, "ghGetHistoryItem", |input| {
        input["queries"][0]["sections"] == json!(["body"])
    });
    assert_eq!(body["input"]["queries"][0]["number"], 7);
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

/// A saved `pageSize` (the cursor's grid; no published field sets one) only
/// places a comment page: page 2 of a 4-comment grid starts at comment 4, and
/// the page then fills the response budget instead of stopping at 4, so a
/// walk resumed on a small grid never shrinks to it. Every comment from there
/// arrives once, in order, and the block states no resume size.
#[tokio::test]
async fn a_saved_grid_places_issue_comment_pages_without_capping_them() {
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
    let data = read(
        &server,
        json!({"operation":"issue","owner":"o","repo":"r","number":7,
            "sections":["comments"],"page":2,"pageSize":4}),
    )
    .await;
    let issue = &data["issues"][0];
    let ids = issue["comments"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|comment| comment["id"].to_string())
        .collect::<Vec<_>>();
    let expected = (4..11).map(|n| json!(n.to_string()).to_string());
    assert_eq!(ids, expected.collect::<Vec<_>>(), "{data}");
    let block = &issue["sectionPagination"]["comments"];
    assert!(block.get("pageSize").is_none(), "{data}");
    assert_eq!(block["hasMore"], false, "{data}");
}

/// A summary's preview is the body's opening, whitespace collapsed and cut
/// with `…`, never the whole body; a section read of the body holds it
/// verbatim.
#[tokio::test]
async fn an_issue_summary_previews_a_long_body_and_the_body_read_holds_it_whole() {
    let server = MockServer::builder().start().await;
    let body = format!("First line.\n\n{}", "word ".repeat(400));
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/7",
        200,
        json!({"number":7,"title":"Long","state":"open","user":{"login":"bob"},
            "body":body,"comments":0,"created_at":"2024-01-01T00:00:00Z",
            "updated_at":"2024-01-02T00:00:00Z"}),
    )
    .await;
    let summary = read(
        &server,
        json!({"operation":"issue","owner":"o","repo":"r","number":7}),
    )
    .await;
    let preview = summary["issues"][0]["bodyPreview"]
        .as_str()
        .expect("preview");
    assert!(preview.starts_with("First line. word"), "{summary}");
    assert!(preview.ends_with('…'), "{summary}");
    assert!(preview.chars().count() < body.chars().count(), "{summary}");
    let whole = read(
        &server,
        json!({"operation":"issue","owner":"o","repo":"r","number":7,"sections":["body"]}),
    )
    .await;
    assert_eq!(whole["issues"][0]["body"], json!(body), "{whole}");
    assert!(whole["issues"][0].get("bodyPreview").is_none(), "{whole}");
}

/// A discussion the caller asked for is stated even when it holds no
/// comment: an empty list says zero, where an absent one reads as not
/// fetched.
#[tokio::test]
async fn a_requested_discussion_with_no_comment_is_an_empty_list() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/8",
        200,
        json!({"number":8,"title":"Quiet","state":"closed","user":{"login":"bob"},
            "body":"b","comments":0,"created_at":"2024-01-01T00:00:00Z",
            "closed_at":"2024-01-03T00:00:00Z"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/8/comments",
        200,
        json!([]),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"issue","owner":"o","repo":"r","number":8,"sections":["comments"]}),
    )
    .await;
    assert_eq!(out["issues"][0]["comments"], json!([]), "{out}");
}
