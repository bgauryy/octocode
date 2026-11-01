//! Issue closing references and review comments read beside the GraphQL read.
use super::*;

/// An issue's first window lists the pull requests that close it (merged
/// first) from the GraphQL closing references; a later window asks only for
/// their count and lists none.
#[tokio::test]
async fn an_issue_lists_its_closing_pull_requests_from_graphql() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/issues/42",
        200,
        json!({"number": 42, "title": "t", "state": "closed", "body": "x".repeat(30_000),
            "user": {"login": "amy"}, "labels": [], "comments": 0,
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"}),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("nodes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"issue": {"closedByPullRequestsReferences": {
                "totalCount": 2, "pageInfo": {"hasNextPage": false}, "nodes": [
                    {"number": 5, "state": "CLOSED", "mergedAt": null},
                    {"number": 6, "state": "MERGED", "mergedAt": "2026-01-02T00:00:00Z"}]}}}
        }})))
        .expect(1)
        .mount(&server)
        .await;
    // A later window lists no `closedBy`: it spends no GraphQL request.
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let issue = |offset: u64| json!({"operation":"issue","owner":"a","repo":"b","number":42,"offset":offset});
    let first = read(&server, issue(0)).await;
    assert_eq!(
        first.data.pointer("/issues/0/closedBy"),
        Some(
            &json!([{"number": 6, "state": "merged", "mergedAt": "2026-01-02T00:00:00Z"},
            {"number": 5, "state": "closed"}])
        ),
        "{}",
        first.data
    );
    let linked = crate::tools::test_support::checks::related_call(
        &first.data,
        "ghGetHistoryItem",
        |input| input["queries"][0]["operation"] == "pullRequest",
    );
    assert_eq!(linked["input"]["queries"][0]["number"], 6);
    let later = read(&server, issue(12_000)).await;
    assert_eq!(
        later.data.pointer("/issues/0/closedBy"),
        None,
        "{}",
        later.data
    );
}

/// Review comments are REST-only: their first batch is requested beside
/// the GraphQL read, not after its answer.
#[tokio::test]
async fn review_comments_load_beside_the_graphql_read() {
    let server = MockServer::builder().start().await;
    let delay = std::time::Duration::from_millis(400);
    let graphql = Delayed::new(
        delay,
        ResponseTemplate::new(200).set_body_json(graphql_pr(json!({
            "commentsConn": {"pageInfo": {"hasNextPage": false}, "nodes": []},
            "commits": {"pageInfo": {"hasNextPage": false}, "nodes": []}
        }))),
    );
    let graphql_arrivals = graphql.arrivals.clone();
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(graphql)
        .mount(&server)
        .await;
    let inline = Delayed::new(
        std::time::Duration::ZERO,
        ResponseTemplate::new(200).set_body_json(json!([{
            "id": 5, "user": {"login": "amy"}, "body": "nit", "path": "src/a.rs",
            "created_at": "2026-01-01T00:00:00Z", "html_url": "https://x"
        }])),
    );
    let inline_arrivals = inline.arrivals.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7/comments"))
        .respond_with(inline)
        .mount(&server)
        .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,
            "sections":["comments","commits","reviewComments"]}),
    )
    .await;
    let pr = &out.data["pullRequests"][0];
    assert!(pr.get("reviewComments").is_some(), "{pr}");
    let (graphql_at, inline_at) = (graphql_arrivals.times(), inline_arrivals.times());
    assert_eq!((graphql_at.len(), inline_at.len()), (1, 1), "{pr}");
    assert!(
        inline_at[0] < graphql_at[0] + delay,
        "review comments waited {:?} for the GraphQL answer",
        inline_at[0].duration_since(graphql_at[0])
    );
}

/// Every closing reference is read, page by page from its cursor: the
/// issue lists all of them and reports no bound.
#[tokio::test]
async fn every_closing_reference_is_read_on_from_its_cursor() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/issues/42",
        200,
        json!({"number": 42, "title": "t", "state": "closed", "body": "b",
            "user": {"login": "amy"}, "labels": [], "comments": 0,
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z"}),
    )
    .await;
    let nodes = |range: std::ops::Range<u64>| {
        range
            .map(|number| json!({"number": number, "state": "CLOSED", "mergedAt": null}))
            .collect::<Vec<_>>()
    };
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("\"after\":\"r1\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"issue": {"closedByPullRequestsReferences": {
                "totalCount": 130, "pageInfo": {"hasNextPage": false, "endCursor": "r2"},
                "nodes": nodes(100..130)}}}
        }})))
        .expect(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"issue": {"closedByPullRequestsReferences": {
                "totalCount": 130, "pageInfo": {"hasNextPage": true, "endCursor": "r1"},
                "nodes": nodes(0..100)}}}
        }})))
        .expect(1)
        .with_priority(2)
        .mount(&server)
        .await;
    let out = read(
        &server,
        json!({"operation":"issue","owner":"a","repo":"b","number":42}),
    )
    .await;
    let closed_by = out
        .data
        .pointer("/issues/0/closedBy")
        .and_then(Value::as_array);
    assert_eq!(closed_by.map(Vec::len), Some(130), "{}", out.data);
    assert!(out.data.get("terminalLimit").is_none(), "{}", out.data);
}
