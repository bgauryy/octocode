//! REST fallbacks: a collection GraphQL cannot read and a failed GraphQL fast path.
use super::*;

/// A connection GraphQL could not read (`null`) is read by REST from its
/// first batch, beside the GraphQL header (no REST read of the PR itself);
/// a complete one is not read again.
#[tokio::test]
async fn an_unreadable_graphql_collection_is_read_by_rest() {
    let server = MockServer::builder().start().await;
    mount_graphql(
        &server,
        graphql_pr(json!({
            "files": null,
            "commentsConn": {"pageInfo": {"hasNextPage": false}, "nodes": []}
        })),
    )
    .await;
    mount_rest_pr(&server, 0).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/pulls/7/files",
        200,
        json!([changed("a.rs"), changed("b.rs"), changed("c.rs")]),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(0)
        .mount(&server)
        .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,
            "sections":["files","comments"]}),
    )
    .await;
    assert_eq!(
        out.data.pointer("/pullRequests/0/files"),
        Some(&json!(["A +1 -0 a.rs", "A +1 -0 b.rs", "A +1 -0 c.rs"])),
        "{}",
        out.data
    );
}

/// A GraphQL answer without the pull request (an `errors[]` reply) falls
/// back to REST; the reason is kept and shown only with the diagnostics
/// extra.
#[tokio::test]
async fn a_failed_graphql_read_falls_back_to_rest() {
    let server = MockServer::builder().start().await;
    mount_graphql(
        &server,
        json!({"data": {"repository": {"pullRequest": null}},
            "errors": [{"type": "FORBIDDEN", "message": "Resource not accessible by integration"}]}),
    )
    .await;
    mount_rest_pr(&server, 2).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/pulls/7/files",
        200,
        json!([changed("a.rs")]),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/issues/7/comments",
        200,
        json!([{"id": 11, "user": {"login": "amy"}, "body": "Looks good.",
            "created_at": "2026-01-01T00:00:00Z"}]),
    )
    .await;
    let row = |extras: Value| {
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,
            "sections":["files","comments"],"extras":extras})
    };
    let quiet = read(&server, row(json!([]))).await;
    assert_eq!(
        quiet.data.pointer("/pullRequests/0/comments/0/body"),
        Some(&json!("Looks good.")),
        "{}",
        quiet.data
    );
    assert_eq!(quiet.data.get("graphqlFallback"), None, "{}", quiet.data);
    let request = HistoryItemRequest::from_row(row(json!(["diagnostics"]))).expect("row");
    assert!(gh_get_history_item_query::wants(
        &request,
        ExtraKind::Diagnostics
    ));
    let shown = read(&server, row(json!(["diagnostics"]))).await;
    let reason = shown.data["graphqlFallback"].as_str().unwrap_or_default();
    assert!(reason.contains("not accessible"), "{}", shown.data);
}
