//! REST fallbacks: a collection GraphQL cannot read and a failed GraphQL fast path.
use super::*;
use octocode_test_support::json_fixture;

/// A connection GraphQL could not read (`null`) is read by REST from its
/// first batch, beside the GraphQL header (no REST read of the PR itself);
/// a complete one is not read again.
#[tokio::test]
async fn an_unreadable_graphql_collection_is_read_by_rest() {
    let server = MockServer::builder().start().await;
    mount_graphql(
        &server,
        graphql_pr(json_fixture(
            r#"{
                "files": null,
                "commentsConn": {"pageInfo": {"hasNextPage": false}, "nodes": []}
            }"#,
        )),
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
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 7,
                "sections": ["files", "comments"]
            }"#,
        ),
    )
    .await;
    assert_eq!(
        out.data.pointer("/pullRequests/0/files"),
        Some(&json_fixture(
            r#"["A +1 -0 a.rs", "A +1 -0 b.rs", "A +1 -0 c.rs"]"#
        )),
        "{}",
        out.data
    );
}

/// A GraphQL answer without the pull request (an `errors[]` reply) falls
/// back to REST. REST returns the same rows, so the transport is not part of
/// the answer: the row holds only fields the output contract declares.
#[tokio::test]
async fn a_failed_graphql_read_falls_back_to_rest() {
    let server = MockServer::builder().start().await;
    mount_graphql(
        &server,
        json_fixture(
            r#"{
                "data": {"repository": {"pullRequest": null}},
                "errors": [
                    {
                        "type": "FORBIDDEN",
                        "message": "Resource not accessible by integration"
                    }
                ]
            }"#,
        ),
    )
    .await;
    mount_rest_pr(&server, 1).await;
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
        json_fixture(
            r#"[
                {
                    "id": 11,
                    "user": {"login": "amy"},
                    "body": "Looks good.",
                    "created_at": "2026-01-01T00:00:00Z"
                }
            ]"#,
        ),
    )
    .await;
    let out = read(
        &server,
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 7,
                "sections": ["files", "comments"]
            }"#,
        ),
    )
    .await;
    assert_eq!(
        out.data.pointer("/pullRequests/0/comments/0/body"),
        Some(&json!("Looks good.")),
        "{}",
        out.data
    );
    assert_declared(&out.data);
}

/// The row as published meets the canonical output contract, which
/// rejects any key the core schema does not declare.
fn assert_declared(data: &Value) {
    let shaped = octocode_runtime_base::response::rows::result_row(0, data.clone(), None);
    if let Err(error) = octocode_runtime_base::test_support::validate_sealed(
        "ghGetHistoryItem",
        &json!({"results": [shaped]}),
    ) {
        panic!("{error}\n{data}");
    }
}

/// The row of the same read when GraphQL could not read `files` at all
/// (`null`): REST lists them, beside the header `header` overrides.
async fn files_read_by_rest(header: Value) -> Value {
    let server = MockServer::builder().start().await;
    let mut connections = header;
    connections["files"] = Value::Null;
    connections["commentsConn"] =
        json_fixture(r#"{"pageInfo": {"hasNextPage": false}, "nodes": []}"#);
    mount_graphql(&server, graphql_pr(connections)).await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/pulls/7/files",
        200,
        json!([changed("a.rs"), changed("b.rs"), changed("c.rs")]),
    )
    .await;
    read(
        &server,
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 7,
                "sections": ["files", "comments"]
            }"#,
        ),
    )
    .await
    .data
}

/// A GraphQL `errors[]` entry inside a connection beside non-null data (a
/// node field GitHub nulled) makes that connection untrusted: REST reads it
/// whole and an untouched connection is not read again. REST's rows are the
/// evidence, so the row is the one GraphQL's `null` connection gives: the
/// transport's errors are not part of the answer.
#[tokio::test]
async fn graphql_errors_beside_a_connection_send_it_to_rest() {
    let server = MockServer::builder().start().await;
    let mut answer = graphql_pr(json_fixture(
        r#"{
            "files": {
                "pageInfo": {"hasNextPage": false},
                "nodes": [
                    {
                        "path": "a.rs",
                        "additions": 1,
                        "deletions": 0,
                        "changeType": "ADDED"
                    },
                    {
                        "path": null,
                        "additions": 1,
                        "deletions": 0,
                        "changeType": "ADDED"
                    }
                ]
            },
            "commentsConn": {"pageInfo": {"hasNextPage": false}, "nodes": []}
        }"#,
    ));
    answer["errors"] = json_fixture(
        r#"[
            {
                "type": "FORBIDDEN",
                "message": "Path withheld",
                "path": ["repository", "pullRequest", "files", "nodes", 1, "path"]
            }
        ]"#,
    );
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
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 7,
                "sections": ["files", "comments"]
            }"#,
        ),
    )
    .await;
    assert_eq!(
        out.data.pointer("/pullRequests/0/files"),
        Some(&json_fixture(
            r#"["A +1 -0 a.rs", "A +1 -0 b.rs", "A +1 -0 c.rs"]"#
        )),
        "{}",
        out.data
    );
    assert_eq!(out.data, files_read_by_rest(json!({})).await);
    assert_declared(&out.data);
}

/// A GraphQL error outside every collection (a nulled PR field) leaves the
/// GraphQL header untrusted: REST serves the whole read, its connections
/// included.
#[tokio::test]
async fn graphql_errors_beside_the_pr_fields_fall_back_to_rest() {
    let server = MockServer::builder().start().await;
    let mut answer = graphql_pr(json_fixture(
        r#"{
            "body": null,
            "files": {
                "pageInfo": {"hasNextPage": false},
                "nodes": [
                    {
                        "path": "a.rs",
                        "additions": 1,
                        "deletions": 0,
                        "changeType": "ADDED"
                    }
                ]
            },
            "commentsConn": {"pageInfo": {"hasNextPage": false}, "nodes": []}
        }"#,
    ));
    answer["errors"] = json_fixture(
        r#"[
            {
                "message": "Body withheld",
                "path": ["repository", "pullRequest", "body"]
            }
        ]"#,
    );
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
        json_fixture(
            r#"[
                {
                    "id": 11,
                    "user": {"login": "amy"},
                    "body": "Looks good.",
                    "created_at": "2026-01-01T00:00:00Z"
                }
            ]"#,
        ),
    )
    .await;
    let out = read(
        &server,
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 7,
                "sections": ["files", "comments"]
            }"#,
        ),
    )
    .await;
    assert_eq!(
        out.data.pointer("/pullRequests/0/comments/0/body"),
        Some(&json!("Looks good.")),
        "{}",
        out.data
    );
    // GraphQL listed one file; REST lists all three.
    assert_eq!(
        out.data.pointer("/pullRequests/0/files"),
        Some(&json_fixture(
            r#"["A +1 -0 a.rs", "A +1 -0 b.rs", "A +1 -0 c.rs"]"#
        )),
        "{}",
        out.data
    );
    assert_declared(&out.data);
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
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 7,
                "sections": ["files", "comments"]
            }"#,
        ),
    )
    .await;
    assert_eq!(
        out.data.pointer("/pullRequests/0/files"),
        Some(&json_fixture(
            r#"["A +1 -0 a.rs", "A +1 -0 b.rs", "A +1 -0 c.rs"]"#
        )),
        "{}",
        out.data
    );
    assert_eq!(
        out.data,
        files_read_by_rest(json_fixture(r#"{"changedFiles": null}"#)).await
    );
    assert_declared(&out.data);
}

/// A connection GraphQL answered with errors is REST's to read. When REST's
/// list then ends short of the count the pull request reports, the row
/// says so as partial evidence with its provider limit.
#[tokio::test]
async fn a_short_rest_list_after_a_graphql_connection_error_is_partial() {
    let server = MockServer::builder().start().await;
    let mut answer = graphql_pr(json_fixture(
        r#"{
            "commitsCount": {"totalCount": 5},
            "commits": {
                "pageInfo": {"hasNextPage": false},
                "nodes": [{"commit": null}]
            }
        }"#,
    ));
    answer["errors"] = json_fixture(
        r#"[
            {
                "message": "Commit withheld",
                "path": [
                    "repository",
                    "pullRequest",
                    "commits",
                    "nodes",
                    0,
                    "commit"
                ]
            }
        ]"#,
    );
    mount_graphql(&server, answer).await;
    let commit = |n: usize| {
        json!({"sha": format!("{n:040x}"), "commit": {"message": format!("commit {n}"),
            "author": {"name": "Ada", "email": "a@x", "date": "2026-01-01T00:00:00Z"}},
            "author": {"login": "ada"}})
    };
    mount_json(
        &server,
        "/api/v3/repos/a/b/pulls/7/commits",
        200,
        json!((0..3).map(commit).collect::<Vec<_>>()),
    )
    .await;
    let out = read(
        &server,
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 7,
                "sections": ["commits"]
            }"#,
        ),
    )
    .await;
    let data = &out.data;
    assert_eq!(
        data["pullRequests"][0]["commits"].as_array().map(Vec::len),
        Some(3),
        "{data}"
    );
    assert_eq!(data["isPartial"], true, "{data}");
    assert_eq!(data["pullRequests"][0]["commitCount"], 5, "{data}");
    assert_eq!(
        data["pullRequests"][0]["sectionPagination"]["commits"]["countScope"], "partial",
        "{data}"
    );
    assert!(data["providerLimit"]["reason"].is_string(), "{data}");
    assert_declared(data);
}
