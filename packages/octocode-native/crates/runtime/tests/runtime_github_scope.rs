// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used)]
//! GitHub tools keep the requested scope and filters, and say when evidence
//! is bounded or a provider path fell back.

mod support;

use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn api(server: &MockServer) -> [(&'static str, String); 1] {
    [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]
}

async fn requested_paths(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect()
}

async fn mount_pull_request(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Fix parser", "state": "open", "merged_at": null,
            "draft": false, "body": "Fixes the parser.", "user": {"login": "bob"},
            "head": {"sha": "def456", "ref": "feat/parser"}, "base": {"ref": "main"},
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z",
            "labels": []
        })))
        .mount(server)
        .await;
}

fn reviews_query(debug: bool) -> Value {
    json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 7,
        "content": {"body": true, "reviews": true}, "debug": debug})
}

/// A GraphQL document GitHub rejects falls back to REST with the cause in
/// debug output; the served GraphQL path returns the same reviews with
/// stable identities and skips the REST collections it replaces.
#[tokio::test]
async fn pull_request_graphql_fallback_is_observable_and_equivalent() {
    let rejected = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"errors": [{
            "path": ["query", "repository", "pullRequest", "isMerged"],
            "extensions": {"code": "undefinedField", "typeName": "PullRequest"},
            "message": "Field 'isMerged' doesn't exist on type 'PullRequest'"
        }]})))
        .expect(2)
        .mount(&rejected)
        .await;
    mount_pull_request(&rejected).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7/reviews"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 11, "user": {"login": "ann"}, "state": "APPROVED", "body": "",
             "submitted_at": "2026-01-03T00:00:00Z", "commit_id": "def456"},
            {"id": 12, "user": {"login": "cy"}, "state": "COMMENTED", "body": "nit",
             "submitted_at": "2026-01-04T00:00:00Z", "commit_id": "def456"}
        ])))
        .mount(&rejected)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&api(&rejected));
    let outcome = call(&runtime, "ghGetHistoryItem", reviews_query(true))
        .await
        .expect("REST fallback");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "{data}");
    let reason = data["graphqlFallback"].as_str().expect("fallback reason");
    assert!(reason.starts_with("undefinedField:"), "{data}");
    let rest_reviews = data["pullRequests"][0]["reviews"].clone();
    assert_eq!(rest_reviews[0]["id"], "11", "{data}");
    assert_eq!(rest_reviews[1]["id"], "12", "{data}");
    let quiet = call(&runtime, "ghGetHistoryItem", reviews_query(false))
        .await
        .expect("REST fallback without debug");
    assert!(
        row_data(&quiet).get("graphqlFallback").is_none(),
        "{}",
        row_data(&quiet)
    );
    runtime.close().await;

    let served = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"pullRequest": {
                "number": 7, "title": "Fix parser", "url": "https://x", "state": "OPEN",
                "body": "Fixes the parser.", "isDraft": false, "author": {"login": "bob"},
                "labels": {"pageInfo": {"hasNextPage": false}, "nodes": []},
                "baseRefName": "main", "headRefName": "feat/parser", "headRefOid": "def456",
                "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z",
                "closedAt": null, "mergedAt": null, "mergeCommit": null,
                "comments": {"totalCount": 0}, "changedFiles": 1, "additions": 1, "deletions": 0,
                "reviews": {"pageInfo": {"hasNextPage": false}, "nodes": [
                    {"databaseId": 11, "author": {"login": "ann"}, "state": "APPROVED",
                     "body": "", "submittedAt": "2026-01-03T00:00:00Z", "commit": {"oid": "def456"}},
                    {"databaseId": 12, "author": {"login": "cy"}, "state": "COMMENTED",
                     "body": "nit", "submittedAt": "2026-01-04T00:00:00Z", "commit": {"oid": "def456"}}
                ]}
            }}
        }})))
        .expect(1)
        .mount(&served)
        .await;
    let runtime = workspace.runtime(&api(&served));
    let outcome = call(&runtime, "ghGetHistoryItem", reviews_query(true))
        .await
        .expect("GraphQL read");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "{data}");
    assert!(data.get("graphqlFallback").is_none(), "{data}");
    assert_eq!(
        data["pullRequests"][0]["reviews"], rest_reviews,
        "GraphQL and REST reviews agree: {data}"
    );
    assert_eq!(
        requested_paths(&served).await,
        ["/api/graphql"],
        "GraphQL replaces the REST metadata and review reads"
    );
    runtime.close().await;
}

/// More closing references than one read lists: the set says it is
/// bounded and the fix it names is a candidate.
#[tokio::test]
async fn issue_closing_references_past_the_read_limit_are_disclosed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42, "title": "t", "state": "closed", "state_reason": "completed",
            "body": "", "user": {"login": "alice"}, "labels": [], "comments": 0,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let nodes = (1..=25)
        .map(|n| json!({"number": 100 + n, "state": "CLOSED", "mergedAt": null}))
        .collect::<Vec<_>>();
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"issue": {"closedByPullRequestsReferences": {
                "totalCount": 31, "pageInfo": {"hasNextPage": true}, "nodes": nodes
            }}}
        }})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&api(&server));
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42, "debug": false}),
    )
    .await
    .expect("issue read");
    let data = row_data(&outcome);
    assert_eq!(
        data["issues"][0]["closedBy"].as_array().map(Vec::len),
        Some(25),
        "{data}"
    );
    assert_eq!(data["isPartial"], true, "{data}");
    assert_eq!(data["terminalLimit"], true, "{data}");
    assert_eq!(
        data["partialReasons"],
        json!(["closingReferenceLimit"]),
        "{data}"
    );
    assert!(
        data["warnings"][0]
            .as_str()
            .is_some_and(|w| w.contains("25 of 31")),
        "{data}"
    );
    assert_eq!(data["hints"]["readFixPr"]["confidence"], "medium", "{data}");
    runtime.close().await;
}

/// `archived` filters a scoped pull-request listing through search, with
/// or without keywords; the pulls list would silently ignore it.
#[tokio::test]
async fn archived_pull_request_listing_enforces_the_filter() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"full_name": "a/b", "default_branch": "main"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count": 0, "incomplete_results": false, "items": []})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&api(&server));
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "pullRequest", "owner": "a", "repo": "b", "archived": true, "pageSize": 1}),
    )
    .await
    .expect("archived listing");
    assert_ne!(row_status(&outcome), "error", "{}", row_data(&outcome));
    let requests = server.received_requests().await.unwrap_or_default();
    let search = requests
        .iter()
        .find(|request| request.url.path() == "/api/v3/search/issues")
        .expect("search request");
    let q = search
        .url
        .query_pairs()
        .find(|(key, _)| key == "q")
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default();
    assert!(q.contains("archived:true"), "{q}");
    runtime.close().await;
}

/// A non-empty code-search page GitHub marks incomplete keeps its partial
/// coverage in default (non-debug) output.
#[tokio::test]
async fn incomplete_code_search_page_is_partial_without_debug() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1, "incomplete_results": true, "items": [
                {"name": "app.rs", "path": "app.rs", "sha": "1", "html_url": "https://x",
                 "repository": {"full_name": "a/b", "html_url": "https://x", "url": "https://x"}}
            ]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&api(&server));
    let outcome = call(
        &runtime,
        "ghSearchCode",
        json!({"owner": "a", "keywords": ["app"], "match": "path", "debug": false}),
    )
    .await
    .expect("code search");
    let data = row_data(&outcome);
    assert_eq!(data["isPartial"], true, "{data}");
    assert_eq!(
        data["partialReasons"],
        json!(["providerIncompleteResults"]),
        "{data}"
    );
    assert_eq!(data["next"]["retry"]["tool"], "ghSearchCode", "{data}");
    runtime.close().await;
}

/// Records when each commit-detail request arrives, then answers late.
#[derive(Clone)]
struct SlowCommitDetail {
    arrivals: Arc<Mutex<Vec<Instant>>>,
}

impl Respond for SlowCommitDetail {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.arrivals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Instant::now());
        let sha = request.url.path().rsplit('/').next().unwrap_or_default();
        ResponseTemplate::new(200)
            .set_delay(Duration::from_millis(400))
            .set_body_json(json!({"sha": sha, "files": [
                {"filename": format!("{sha}.rs"), "status": "modified",
                 "additions": 1, "deletions": 0, "changes": 1}
            ]}))
    }
}

/// `commits.includeFiles` fetches commit details a few at a time and keeps
/// the page order.
#[tokio::test]
async fn pull_request_commit_details_load_concurrently_in_order() {
    let server = MockServer::start().await;
    mount_pull_request(&server).await;
    let shas = ["c1", "c2", "c3", "c4"].map(|s| s.repeat(20));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(
            shas.iter()
                .map(|sha| json!({"sha": sha, "commit": {"message": format!("m {sha}"),
                    "author": {"name": "bob", "date": "2026-01-01T00:00:00Z"}}}))
                .collect::<Vec<_>>()
        )))
        .mount(&server)
        .await;
    let arrivals = Arc::new(Mutex::new(Vec::new()));
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/commits/c[0-9a-f]+$"))
        .respond_with(SlowCommitDetail {
            arrivals: arrivals.clone(),
        })
        .expect(4)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&api(&server));
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 7,
            "content": {"commits": {"includeFiles": true}}}),
    )
    .await
    .expect("commit details");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "{data}");
    let listed = data["pullRequests"][0]["commits"]
        .as_array()
        .expect("commits")
        .iter()
        .map(|commit| commit["sha"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(listed, shas, "page order: {data}");
    assert!(
        data["pullRequests"][0]["commits"][2]["files"]
            .to_string()
            .contains(&format!("{}.rs", shas[2])),
        "each commit keeps its own files: {data}"
    );
    // All four reads are in flight before the first 400 ms answer lands;
    // one at a time they would arrive 400 ms apart.
    let arrivals = arrivals
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let spread = arrivals[arrivals.len() - 1].duration_since(arrivals[0]);
    assert!(spread < Duration::from_millis(300), "{spread:?}");
    runtime.close().await;
}
