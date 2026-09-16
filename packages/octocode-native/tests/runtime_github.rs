mod support;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn github_file_read_goes_through_execute_and_redacts() {
    let server = MockServer::start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Flib.rs"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file",
            "encoding": "base64",
            "content": STANDARD.encode(format!("one\nneedle ghp_{}\nthree\n", "a".repeat(37))),
            "size": 22,
            "sha": "f".repeat(40),
            "path": "src/lib.rs"
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({
            "owner": "a",
            "repo": "b",
            "path": "src/lib.rs",
            "branch": "main",
            "forceRefresh": true,
            "chunkType": "lines",
            "limit": 2
        }),
    )
    .await
    .expect("github read");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let file = &row_data(&outcome)["files"][0];
    let content = file["content"].as_str().unwrap_or("");
    assert!(content.contains("one\n"), "{content}");
    assert!(
        content.contains("[REDACTED"),
        "expected secret redaction, got {content}"
    );
    assert!(file["next"]["continue"]["query"].is_object());
    runtime.close().await;
}

#[tokio::test]
async fn github_missing_identity_is_a_contract_error() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let error = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b"}),
    )
    .await
    .expect_err("missing sha");
    assert_eq!(error.code, "invalidInput");
    runtime.close().await;
}

#[tokio::test]
async fn github_tree_materialize_is_accepted_and_emits_location() {
    let server = MockServer::start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", "main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"one.rs","path":"one.rs","type":"file","size":4,"sha":"1".repeat(40)},
            {"name":"two.rs","path":"two.rs","type":"file","size":4,"sha":"2".repeat(40)}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    for name in ["one.rs", "two.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", sha))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type": "file",
                "encoding": "base64",
                "content": STANDARD.encode("fn x(){}\n"),
                "size": 8,
                "sha": "a".repeat(40),
                "path": name
            })))
            .mount(&server)
            .await;
    }

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearch",
        json!({
            "operation": "tree",
            "owner": "a",
            "repo": "b",
            "materialize": true
        }),
    )
    .await
    .expect("tree materialize");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert!(data["location"]["localPath"].as_str().is_some(), "{}", data);
    runtime.close().await;
}

#[tokio::test]
async fn github_clone_is_unavailable_when_disabled() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("ENABLE_CLONE", "false".into())]);
    let error = call(&runtime, "ghCloneRepo", json!({"owner":"a","repo":"b"}))
        .await
        .expect_err("clone disabled");
    assert_eq!(error.code, "toolUnavailable");
    runtime.close().await;
}

/// Returns the `status` field for every result row in a multi-query outcome.
fn all_row_statuses(outcome: &octocode_native::runtime::ToolOutcome) -> Vec<&str> {
    outcome
        .structured_content
        .get("results")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    r.get("status")
                        .and_then(Value::as_str)
                        .unwrap_or(if r.get("data").is_some() {
                            "success"
                        } else {
                            ""
                        })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Exercises the batch `queries` array path and verifies all 3 results succeed.
/// This proves that `execute_queries` / `.buffered(3)` handles multi-query input
/// correctly end-to-end through the full tool dispatch stack.
#[tokio::test]
async fn batch_of_three_github_queries_all_succeed() {
    let server = MockServer::start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";

    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;

    for name in ["alpha.rs", "beta.rs", "gamma.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", sha))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type": "file",
                "encoding": "base64",
                "content": STANDARD.encode("fn placeholder(){}"),
                "size": 18,
                "sha": "a".repeat(40),
                "path": name
            })))
            .mount(&server)
            .await;
    }

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);

    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({
            "queries": [
                {"owner": "a", "repo": "b", "path": "alpha.rs",
                 "branch": "main", "forceRefresh": true},
                {"owner": "a", "repo": "b", "path": "beta.rs",
                 "branch": "main", "forceRefresh": true},
                {"owner": "a", "repo": "b", "path": "gamma.rs",
                 "branch": "main", "forceRefresh": true},
            ]
        }),
    )
    .await
    .expect("three-query batch");

    let statuses = all_row_statuses(&outcome);
    assert_eq!(
        statuses.len(),
        3,
        "expected 3 results: {}",
        outcome.structured_content
    );
    for (i, status) in statuses.iter().enumerate() {
        assert_ne!(
            *status, "error",
            "result[{i}] failed: {}",
            outcome.structured_content
        );
    }

    runtime.close().await;
}

/// Verifies that a batch of 3 `ghGetFileContent` queries dispatches all 3
/// HTTP content requests to the server — the server-side receipt count is the
/// observable proof that `execute_queries` / `.buffered(3)` actually issues
/// one HTTP request per query rather than short-circuiting.
///
/// Why not a wall-time assertion?  `handle.block_on` drives all 3 futures on
/// a single blocking thread: the HTTP requests are sent in parallel, but
/// response processing (JSON parsing, content scanning, minification) runs
/// sequentially on that one thread.  In a debug build each response takes
/// ~300–400 ms of CPU time, which swamps any network-delay signal.  A
/// server-side request count is deterministic and environment-independent.
#[tokio::test]
async fn three_github_queries_each_produce_a_server_side_http_request() {
    let server = MockServer::start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";

    // One SHA endpoint shared by all three queries.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;

    // Three distinct content endpoints — each MUST be hit exactly once.
    for name in ["p.rs", "q.rs", "r.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", sha))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type": "file",
                "encoding": "base64",
                "content": STANDARD.encode("fn ok(){}"),
                "size": 9,
                "sha": "b".repeat(40),
                "path": name
            })))
            .expect(1) // exactly 1 request per file endpoint
            .mount(&server)
            .await;
    }

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);

    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({
            "queries": [
                {"owner": "a", "repo": "b", "path": "p.rs",
                 "branch": "main", "forceRefresh": true},
                {"owner": "a", "repo": "b", "path": "q.rs",
                 "branch": "main", "forceRefresh": true},
                {"owner": "a", "repo": "b", "path": "r.rs",
                 "branch": "main", "forceRefresh": true},
            ]
        }),
    )
    .await
    .expect("three-query dispatch");

    // All three results succeed.
    let statuses = all_row_statuses(&outcome);
    assert_eq!(
        statuses.len(),
        3,
        "expected 3 results: {}",
        outcome.structured_content
    );
    for (i, status) in statuses.iter().enumerate() {
        assert_ne!(
            *status, "error",
            "result[{i}] failed: {}",
            outcome.structured_content
        );
    }
    // WireMock verifies `.expect(1)` on drop: each content endpoint was hit exactly once.

    runtime.close().await;
}

// ── S19: ghSearchHistory integration tests ──────────────────────────────────

#[tokio::test]
async fn gh_search_history_commits_lists_via_rest() {
    let server = MockServer::start().await;
    // canonical_owner_repo pre-flight
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
        json!({"operation": "commits", "owner": "a", "repo": "b"}),
    )
    .await
    .expect("gh_search_history commits");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        rendered.contains("stabilise parser"),
        "expected commit in {rendered}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_history_issues_lists_via_rest() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"full_name":"a/b","default_branch":"main"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"number": 42, "title": "Memory leak in parser", "state": "open",
             "user": {"login": "alice"}, "labels": [], "pull_request": null}
        ])))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "issues", "owner": "a", "repo": "b"}),
    )
    .await
    .expect("gh_search_history issues");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        rendered.contains("Memory leak"),
        "expected issue in {rendered}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_history_pull_requests_lists_via_rest() {
    let server = MockServer::start().await;
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
        json!({"operation": "pullRequests", "owner": "a", "repo": "b"}),
    )
    .await
    .expect("gh_search_history pullRequests");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        rendered.contains("concurrency buffering"),
        "expected PR in {rendered}"
    );
    runtime.close().await;
}

// ── S20: ghGetHistoryItem integration tests ───────────────────────────────────

#[tokio::test]
async fn gh_get_history_item_commit_fetches_via_rest() {
    let server = MockServer::start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "commit": {
                "message": "fix: critical regression",
                "author": {"name": "Alice", "date": "2024-01-01T00:00:00Z"}
            },
            "author": {"login": "alice"},
            "stats": {"additions": 5, "deletions": 2},
            "files": []
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "commit", "owner": "a", "repo": "b", "ref": sha}),
    )
    .await
    .expect("gh_get_history_item commit");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        rendered.contains("critical regression"),
        "expected commit message in {rendered}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_issue_fetches_via_rest() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42,
            "title": "Parser OOM on large inputs",
            "state": "open",
            "body": "Detailed reproduction steps here",
            "user": {"login": "alice"},
            "labels": [],
            "comments": 3
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42}),
    )
    .await
    .expect("gh_get_history_item issue");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        rendered.contains("Parser OOM"),
        "expected issue title in {rendered}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_commit_not_found_surfaces_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/deadbeef1234567890"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    // Not-found is surfaced as a row-level error (non-panic)
    let result = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "commit", "owner": "a", "repo": "b",
               "ref": "deadbeef1234567890"}),
    )
    .await;
    // Either a top-level RuntimeError or a row-level error status
    let is_error = match &result {
        Err(_) => true,
        Ok(outcome) => {
            let status = row_status(outcome);
            status == "error" || status.is_empty()
        }
    };
    assert!(is_error, "expected not-found to surface as error");
    runtime.close().await;
}

#[tokio::test]
async fn artifact_search_lookup_goes_through_execute() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/left-pad/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "left-pad",
            "version": "1.3.0",
            "description": "pad strings",
            "repository": {"type": "git", "url": "https://github.com/stevemao/left-pad"}
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[]);
    let outcome = call(
        &runtime,
        "artifactSearch",
        json!({
            "type": "npm",
            "packageName": "left-pad",
            "registry": server.uri()
        }),
    )
    .await
    .expect("artifactSearch");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        rendered.contains("left-pad"),
        "expected package in {rendered}"
    );
    runtime.close().await;
}
