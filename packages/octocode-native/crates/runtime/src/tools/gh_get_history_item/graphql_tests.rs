//! The GraphQL branches of an item read against a mock GitHub (GraphQL beside
//! REST): a pull request's first page served by GraphQL, a collection GraphQL
//! left incomplete read by REST, a failed fast path falling back to REST, and
//! an issue's closing references.
use super::{HistoryItemRequest, execute};
use crate::security::scan::Passthrough;
use crate::tools::gh_get_history_item::gh_get_history_item_query;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use octocode_contract_types::ExtraKind;
use octocode_github::mock_http::{Delayed, mount_json};
use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BASE: &str = "1111111111111111111111111111111111111111";
const HEAD: &str = "2222222222222222222222222222222222222222";

/// The GraphQL pull request `a/b#7` with the given connections.
fn graphql_pr(connections: Value) -> Value {
    let mut pr = json!({
        "number": 7, "title": "Fix parser", "url": "https://x", "state": "OPEN",
        "body": "Fixes the parser.", "isDraft": false, "author": {"login": "bob"},
        "labels": {"pageInfo": {"hasNextPage": false}, "nodes": []},
        "baseRefName": "main", "baseRefOid": BASE, "headRefName": "feat", "headRefOid": HEAD,
        "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z",
        "closedAt": null, "mergedAt": null, "mergeCommit": null,
        "comments": {"totalCount": 1}, "changedFiles": 3, "additions": 3, "deletions": 0,
        "commitsCount": {"totalCount": 1}, "reviewThreads": {"totalCount": 0}
    });
    for (key, value) in connections.as_object().into_iter().flatten() {
        pr[key] = value.clone();
    }
    json!({"data": {"repository": {"pullRequest": pr}}})
}

async fn mount_graphql(server: &MockServer, answer: Value) {
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .mount(server)
        .await;
}

/// The REST pull request itself; `reads` is how often a read may ask.
async fn mount_rest_pr(server: &MockServer, reads: u64) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Fix parser", "state": "open", "body": "Fixes the parser.",
            "user": {"login": "bob"}, "labels": [], "draft": false,
            "base": {"ref": "main", "sha": BASE}, "head": {"ref": "feat", "sha": HEAD},
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z",
            "comments": 1, "review_comments": 0, "commits": 1, "changed_files": 3,
            "additions": 3, "deletions": 0
        })))
        .expect(reads)
        .mount(server)
        .await;
}

async fn read(server: &MockServer, row: Value) -> crate::response::result::ToolData {
    let provider = one_shot_provider(server);
    let request = HistoryItemRequest::from_row(row).expect("valid row");
    execute(
        &provider.transport,
        &request,
        &default_context(),
        &Passthrough,
    )
    .await
    .expect("history item")
}

fn changed(name: &str) -> Value {
    json!({"filename": name, "status": "added", "additions": 1, "deletions": 0, "changes": 1})
}

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
            .pointer("/next/nextCommentPage/query/queries/0/page"),
        Some(&json!(2)),
        "{}",
        out.data
    );
}

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
        json!({"number": 42, "title": "t", "state": "closed", "body": "x".repeat(30),
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
    let issue = |offset: u64| json!({"operation":"issue","owner":"a","repo":"b","number":42,"offset":offset,"length":10});
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
    assert_eq!(
        first
            .data
            .pointer("/next/readPullRequest/query/queries/0/number"),
        Some(&json!(6)),
        "{}",
        first.data
    );
    let later = read(&server, issue(10)).await;
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
