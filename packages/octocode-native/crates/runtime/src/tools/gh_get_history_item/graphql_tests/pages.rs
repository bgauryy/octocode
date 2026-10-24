//! GraphQL first pages and connection cursors: what one GraphQL read serves and how a connection reads on.
use super::*;

/// A first page of two collections is one GraphQL read: the header and
/// both complete collections come from it, with no REST request at all.
#[tokio::test]
async fn a_first_page_of_two_collections_is_one_graphql_read() {
    let server = MockServer::builder().start().await;
    mount_graphql(
        &server,
        graphql_pr(json!({
            "files": {"pageInfo": {"hasNextPage": false}, "nodes": [
                {"path": "src/a.rs", "additions": 1, "deletions": 0, "changeType": "ADDED"}]},
            "commentsConn": {"pageInfo": {"hasNextPage": false}, "nodes": [
                {"databaseId": 11, "author": {"login": "amy", "__typename": "User"},
                 "body": "Looks good.", "createdAt": "2026-01-01T00:00:00Z", "url": "https://x"}]}
        })),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,
            "sections":["files","comments"]}),
    )
    .await;
    let pr = &out.data["pullRequests"][0];
    assert_eq!(
        pr.pointer("/comments/0/body"),
        Some(&json!("Looks good.")),
        "{pr}"
    );
    assert_eq!(pr.pointer("/headSha"), Some(&json!(HEAD)), "{pr}");
    let rest = server
        .received_requests()
        .await
        .expect("requests")
        .into_iter()
        .filter(|request| request.url.path().starts_with("/api/v3/"))
        .count();
    assert_eq!(rest, 0, "GraphQL served the whole page");
}

fn file_nodes(range: std::ops::Range<usize>) -> Vec<Value> {
    range
        .map(|n| json!({"path": format!("f{n:03}.rs"), "additions": 1, "deletions": 0, "changeType": "ADDED"}))
        .collect()
}

/// A connection GraphQL returned in part, with no count to size REST
/// batches, reads on from its end cursor until the first page is held:
/// every fetched node stays, and REST is never asked for what GraphQL
/// already sent.
#[tokio::test]
async fn an_uncounted_graphql_connection_reads_on_from_its_cursor() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("after:$after"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"pullRequest": {"files": {
                "pageInfo": {"hasNextPage": false, "endCursor": "c2"},
                "nodes": file_nodes(100..150)}}}
        }})))
        .expect(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(graphql_pr(json!({
            "changedFiles": null,
            "files": {"pageInfo": {"hasNextPage": true, "endCursor": "c1"},
                "nodes": file_nodes(0..100)},
            "commentsConn": {"pageInfo": {"hasNextPage": false}, "nodes": []}
        }))))
        .expect(1)
        .with_priority(2)
        .mount(&server)
        .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,
            "sections":["files","comments"]}),
    )
    .await;
    let files = out.data["pullRequests"][0]["files"]
        .as_array()
        .expect("files");
    assert_eq!(files.len(), 150, "{}", out.data);
    assert_eq!(files[0], "A +1 -0 f000.rs", "{}", out.data);
    assert_eq!(files[149], "A +1 -0 f149.rs", "{}", out.data);
    let continuation = server
        .received_requests()
        .await
        .expect("requests")
        .into_iter()
        .find(|request| String::from_utf8_lossy(&request.body).contains("after:$after"))
        .expect("continuation");
    let body: Value = serde_json::from_slice(&continuation.body).expect("json");
    assert_eq!(body["variables"]["after"], "c1", "{body}");
    let rest = server
        .received_requests()
        .await
        .expect("requests")
        .into_iter()
        .filter(|request| request.url.path().starts_with("/api/v3/"))
        .count();
    assert_eq!(rest, 0, "GraphQL served every file");
}

/// A counted connection whose page needs more than its first GraphQL page
/// (250 of 250 changed files) is read by REST: one GraphQL request, then every REST batch the
/// count sizes in one concurrent wave, never the 1 s-spaced cursor walk.
#[tokio::test]
async fn a_counted_graphql_connection_with_more_pages_is_read_by_rest_in_one_wave() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(graphql_pr(json!({
            "changedFiles": 250,
            "files": {"pageInfo": {"hasNextPage": true, "endCursor": "c1"},
                "nodes": file_nodes(0..100)}
        }))))
        .expect(1)
        .mount(&server)
        .await;
    mount_rest_pr(&server, 0).await;
    // Each batch answers `delay` after it is asked, and all three log their
    // arrivals in one place.
    let delay = std::time::Duration::from_millis(500);
    let mut arrivals = None;
    for (page, files) in [(1, 0..100), (2, 100..200), (3, 200..250)] {
        let mut answer = ResponseTemplate::new(200).set_body_json(
            files
                .map(|n| changed(&format!("f{n:03}.rs")))
                .collect::<Vec<_>>(),
        );
        if page < 3 {
            answer = answer.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/pulls/7/files?page={}>; rel=\"next\"",
                    server.uri(),
                    page + 1
                ),
            );
        }
        let mut batch = Delayed::new(delay, answer);
        match &arrivals {
            Some(shared) => batch.arrivals = Clone::clone(shared),
            None => arrivals = Some(batch.arrivals.clone()),
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/pulls/7/files"))
            .and(wiremock::matchers::query_param("page", page.to_string()))
            .respond_with(batch)
            .expect(1)
            .mount(&server)
            .await;
    }
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,
            "sections":["body","files"],"pageSize":250}),
    )
    .await;
    let files = out.data["pullRequests"][0]["files"]
        .as_array()
        .expect("files");
    assert_eq!(files.len(), 250, "{}", out.data);
    assert_eq!(files[0], "A +1 -0 f000.rs", "{}", out.data);
    assert_eq!(files[249], "A +1 -0 f249.rs", "{}", out.data);
    // One wave: every batch was asked before the first could answer. A
    // walk asks each batch only after the previous one answered, so its
    // asks span at least two delays.
    let asked = arrivals.expect("batches mounted").times();
    assert_eq!(asked.len(), 3, "three REST batches");
    let span = asked[2].duration_since(asked[0]);
    assert!(span < delay, "batches asked {span:?} apart");
}

/// A first page GraphQL already holds (more comments remain) is served
/// from it: no continuation, no REST read, and the page says more follow.
#[tokio::test]
async fn a_graphql_first_page_is_served_though_more_remain() {
    let server = MockServer::builder().start().await;
    let comments = (0..100)
        .map(|n| json!({"databaseId": n, "author": {"login": "amy", "__typename": "User"},
            "body": format!("comment {n}"), "createdAt": "2026-01-01T00:00:00Z", "url": "https://x"}))
        .collect::<Vec<_>>();
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(graphql_pr(json!({
            "comments": {"totalCount": 150},
            "commentsConn": {"pageInfo": {"hasNextPage": true, "endCursor": "c1"}, "nodes": comments}
        }))))
        .expect(1)
        .mount(&server)
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
            "sections":["body","comments"],"pageSize":40}),
    )
    .await;
    let shown = out.data["pullRequests"][0]["comments"]
        .as_array()
        .expect("comments");
    assert_eq!(shown.len(), 40, "{}", out.data);
    assert_eq!(shown[0]["body"], "comment 0", "{}", out.data);
    assert_eq!(
        out.data
            .pointer("/next/nextCommentPage/input/queries/0/page"),
        Some(&json!(2)),
        "{}",
        out.data
    );
}
