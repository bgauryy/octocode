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
    let row = || {
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,
            "sections":["files","comments"]})
    };
    let quiet = read(&server, row()).await;
    assert_eq!(
        quiet.data.pointer("/pullRequests/0/comments/0/body"),
        Some(&json!("Looks good.")),
        "{}",
        quiet.data
    );
    assert!(quiet.data["graphqlFallback"].is_string());
    let shown = read(&server, row()).await;
    let reason = shown.data["graphqlFallback"].as_str().unwrap_or_default();
    assert!(reason.contains("not accessible"), "{}", shown.data);
}

/// The response's warnings, joined.
fn warnings(data: &Value) -> String {
    data["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("\n")
}

/// A GraphQL `errors[]` entry inside a connection beside non-null data (a
/// node field GitHub nulled) makes that connection untrusted: REST reads it
/// whole, an untouched connection is not read again, and a warning names the
/// error's path and text.
#[tokio::test]
async fn graphql_errors_beside_a_connection_send_it_to_rest_and_are_warned() {
    let server = MockServer::builder().start().await;
    let mut answer = graphql_pr(json!({
        "files": {"pageInfo": {"hasNextPage": false}, "nodes": [
            {"path": "a.rs", "additions": 1, "deletions": 0, "changeType": "ADDED"},
            {"path": null, "additions": 1, "deletions": 0, "changeType": "ADDED"}]},
        "commentsConn": {"pageInfo": {"hasNextPage": false}, "nodes": []}
    }));
    answer["errors"] = json!([{"type": "FORBIDDEN", "message": "Path withheld",
        "path": ["repository", "pullRequest", "files", "nodes", 1, "path"]}]);
    mount_graphql(&server, answer).await;
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
    let warned = warnings(&out.data);
    assert!(
        warned.contains("repository.pullRequest.files.nodes.1.path")
            && warned.contains("Path withheld"),
        "{}",
        out.data
    );
}

/// A GraphQL error outside every collection (a nulled PR field) leaves the
/// GraphQL header untrusted: REST serves the whole read, and the fallback
/// reason names every error's path and text.
#[tokio::test]
async fn graphql_errors_beside_the_pr_fields_fall_back_to_rest() {
    let server = MockServer::builder().start().await;
    let mut answer = graphql_pr(json!({
        "body": null,
        "files": {"pageInfo": {"hasNextPage": false}, "nodes": [
            {"path": "a.rs", "additions": 1, "deletions": 0, "changeType": "ADDED"}]},
        "commentsConn": {"pageInfo": {"hasNextPage": false}, "nodes": []}
    }));
    answer["errors"] = json!([{"message": "Body withheld",
        "path": ["repository", "pullRequest", "body"]}]);
    mount_graphql(&server, answer).await;
    mount_rest_pr(&server, 1).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/pulls/7/files",
        200,
        json!([changed("a.rs"), changed("b.rs"), changed("c.rs")]),
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
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,
            "sections":["files","comments"]}),
    )
    .await;
    assert_eq!(
        out.data.pointer("/pullRequests/0/comments/0/body"),
        Some(&json!("Looks good.")),
        "{}",
        out.data
    );
    let reason = out.data["graphqlFallback"].as_str().unwrap_or_default();
    assert!(
        reason.contains("repository.pullRequest.body") && reason.contains("Body withheld"),
        "{}",
        out.data
    );
}

/// A later GraphQL page that carries `errors[]` is not appended: the
/// connection is left to REST, and a warning names the error.
#[tokio::test]
async fn graphql_errors_on_a_later_page_leave_the_connection_to_rest() {
    let server = MockServer::builder().start().await;
    let nodes = |range: std::ops::Range<usize>| {
        range
            .map(|n| json!({"path": format!("f{n:03}.rs"), "additions": 1, "deletions": 0, "changeType": "ADDED"}))
            .collect::<Vec<_>>()
    };
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("after:$after"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"repository": {"pullRequest": {"files": {
                "pageInfo": {"hasNextPage": false, "endCursor": "c2"},
                "nodes": nodes(100..150)}}}},
            "errors": [{"message": "Path withheld",
                "path": ["repository", "pullRequest", "files", "nodes", 3, "path"]}]
        })))
        .expect(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(graphql_pr(json!({
            "changedFiles": null,
            "files": {"pageInfo": {"hasNextPage": true, "endCursor": "c1"},
                "nodes": nodes(0..100)},
            "commentsConn": {"pageInfo": {"hasNextPage": false}, "nodes": []}
        }))))
        .expect(1)
        .with_priority(2)
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/pulls/7/files",
        200,
        json!([changed("a.rs"), changed("b.rs"), changed("c.rs")]),
    )
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
    let warned = warnings(&out.data);
    assert!(
        warned.contains("repository.pullRequest.files.nodes.3.path")
            && warned.contains("Path withheld"),
        "{}",
        out.data
    );
}
