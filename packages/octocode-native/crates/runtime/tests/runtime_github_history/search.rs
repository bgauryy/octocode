//! ghSearchHistory lists: commits, issues and pull requests via REST and search, defaults, filters and page ceilings.

use crate::support::{self};
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn gh_search_history_commits_lists_via_rest() {
    let server = MockServer::builder().start().await;
    // Repository metadata (rename) pre-flight
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"full_name":"a/b","default_branch":"main"})),
        )
        .mount(&server)
        .await;
    // commit list (no keywords → REST list, not search API)
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "sha": "abc123",
                "commit": {
                    "message": "fix: stabilise parser",
                    "author": {"name": "Alice", "date": "2024-01-01T00:00:00Z"}
                },
                "author": {"login": "alice"}
            }
        ])))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "commit", "owner": "a", "repo": "b"}),
    )
    .await
    .expect("gh_search_history commits");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        data["commits"][0]["messageHeadline"], "fix: stabilise parser",
        "{data}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_history_issues_list_via_issue_search() {
    // Plain issue listings page `is:issue` search results: GitHub's REST
    // /issues list interleaves pull requests, which left pages short.
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1,
            "incomplete_results": false,
            "items": [{"number": 42, "title": "Memory leak in parser", "state": "open",
                       "user": {"login": "alice"}, "labels": []}]
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "issue", "owner": "a", "repo": "b"}),
    )
    .await
    .expect("gh_search_history issues");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        data["issues"][0]["title"], "Memory leak in parser",
        "{data}"
    );
    let requests = server.received_requests().await.expect("recorded");
    let q = requests
        .iter()
        .find(|r| r.url.path() == "/api/v3/search/issues")
        .and_then(|r| {
            r.url
                .query_pairs()
                .find(|(k, _)| k == "q")
                .map(|(_, v)| v.into_owned())
        })
        .expect("issue search request");
    assert!(q.contains("repo:a/b") && q.contains("is:issue"), "{q}");
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_history_pull_requests_lists_via_rest() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"full_name":"a/b","default_branch":"main"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "number": 7,
                "title": "Add concurrency buffering",
                "state": "open",
                "user": {"login": "bob"},
                "head": {"sha": "def456", "ref": "feat/buf"},
                "base": {"sha": "main", "ref": "main"}
            }
        ])))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "pullRequest", "owner": "a", "repo": "b"}),
    )
    .await
    .expect("gh_search_history pullRequests");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        data["pullRequests"][0]["title"], "Add concurrency buffering",
        "{data}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn history_rest_page_ceiling_retains_current_items_without_invalid_continuation() {
    for operation in ["pullRequest", "commit"] {
        let server = MockServer::builder().start().await;
        mount_repo_metadata(&server).await;
        let (endpoint, rows, field) = if operation == "pullRequest" {
            (
                "pulls",
                json!([{"number": 9, "title": "Current PR", "state": "open", "user": {"login": "bob"}}]),
                "pullRequests",
            )
        } else {
            (
                "commits",
                json!([{"sha": "abc123", "commit": {"message": "Current commit", "author": {"name": "Bob", "date": "2024-01-01T00:00:00Z"}}, "author": {"login": "bob"}}]),
                "commits",
            )
        };
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/{endpoint}")))
            .and(query_param("page", "1000"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        "Link",
                        format!(
                            "<{}/api/v3/repos/a/b/{endpoint}?page=1001>; rel=\"next\"",
                            server.uri()
                        ),
                    )
                    .set_body_json(rows),
            )
            .mount(&server)
            .await;
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
        let outcome = call(
            &runtime,
            "ghSearchHistory",
            json!({"operation": operation, "owner": "a", "repo": "b", "page": 1000, "pageSize": 1}),
        )
        .await
        .expect("current page");
        let data = row_data(&outcome);
        assert_eq!(
            data[field].as_array().expect("current rows").len(),
            1,
            "{data}"
        );
        assert_eq!(data["terminalLimit"], true, "{data}");
        assert!(data.pointer("/next/nextPage").is_none(), "{data}");
        runtime.close().await;
    }
}

async fn mount_repo_metadata(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"full_name":"a/b","default_branch":"main"})),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn gh_search_history_pull_request_list_defaults_to_all_states_newest_first() {
    let server = MockServer::builder().start().await;
    mount_repo_metadata(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls"))
        .and(query_param("state", "all"))
        .and(query_param("direction", "desc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"number": 9, "title": "Newest", "state": "closed", "user": {"login": "bob"}}
        ])))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "pullRequest", "owner": "a", "repo": "b"}),
    )
    .await
    .expect("PR list");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(data["pullRequests"][0]["number"], 9, "{data}");
    assert!(data.get("effectiveQuery").is_none(), "{data}");
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_history_pull_request_search_works_across_repositories() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1,
            "incomplete_results": false,
            "items": [{"number": 3, "title": "Cross repo fix", "state": "open", "user": {"login": "eve"}}]
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "pullRequest", "keywords": ["buffer"]}),
    )
    .await
    .expect("cross-repo PR search");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let requests = server.received_requests().await.expect("recorded");
    let q = requests
        .iter()
        .find(|r| r.url.path() == "/api/v3/search/issues")
        .and_then(|r| {
            r.url
                .query_pairs()
                .find(|(k, _)| k == "q")
                .map(|(_, v)| v.into_owned())
        })
        .expect("search q");
    assert!(q.contains("is:pr") && !q.contains("repo:"), "{q}");
    assert!(!q.contains("archived:"), "{q}");
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_history_commit_list_forwards_committer() {
    let server = MockServer::builder().start().await;
    mount_repo_metadata(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .and(query_param("committer", "web-flow"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"sha": "abc", "commit": {"message": "merged via UI", "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}}}
        ])))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "commit", "owner": "a", "repo": "b", "qualifiers": "committer:web-flow"}),
    )
    .await
    .expect("commit list");
    let data = row_data(&outcome);
    assert_eq!(
        data["commits"][0]["messageHeadline"], "merged via UI",
        "{data}"
    );
    runtime.close().await;
}
