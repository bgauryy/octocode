// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod support;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, path_regex, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

#[derive(Clone)]
struct DelayedContentResponse {
    arrivals: Arc<Mutex<Vec<Instant>>>,
    path: &'static str,
}

impl Respond for DelayedContentResponse {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.arrivals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Instant::now());
        ResponseTemplate::new(200)
            .set_delay(Duration::from_millis(200))
            .set_body_json(json!({
                "type": "file",
                "encoding": "base64",
                "content": STANDARD.encode("fn placeholder(){}"),
                "size": 18,
                "sha": "a".repeat(40),
                "path": self.path
            }))
    }
}

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
            "chunkSize": 2
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
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"one.rs","path":"one.rs","type":"file","size":4,"sha":"1".repeat(40)},
            {"name":"two.rs","path":"two.rs","type":"file","size":4,"sha":"2".repeat(40)}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
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
        "ghStructure",
        json!({
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
async fn github_clone_is_cli_only_even_when_mcp_enables_clone() {
    let workspace = Workspace::new();
    let cli = workspace.runtime(&[("ENABLE_CLONE", "false".into())]);
    assert!(cli.is_available("ghCloneRepo"));
    let mcp_call = cli
        .execute_mcp(
            "mcp-clone".into(),
            "ghCloneRepo".into(),
            json!({"queries":[{"owner":"a","repo":"b","goal": "test", "reasoning":"check MCP gate"}]}),
        )
        .await
        .expect_err("MCP channel cannot clone through a CLI runtime");
    assert_eq!(mcp_call.code, "toolUnavailable");
    cli.close().await;

    let mut input = workspace.config(&[("ENABLE_CLONE", "true".into())]);
    input.runtime_surface = octocode_native::config::RuntimeSurface::Mcp;
    let mcp = octocode_native::runtime::ToolRuntime::new(input).expect("MCP runtime");
    assert!(!mcp.is_available("ghCloneRepo"));
    let error = call(&mcp, "ghCloneRepo", json!({"owner":"a","repo":"b"}))
        .await
        .expect_err("clone disabled for MCP");
    assert_eq!(error.code, "toolUnavailable");
    mcp.close().await;
}

/// Verifies that independent GitHub API calls run concurrently while output
/// rows retain their input order.
#[tokio::test]
async fn three_github_bulk_queries_are_concurrent_and_preserve_order() {
    let server = MockServer::start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let arrivals = Arc::new(Mutex::new(Vec::new()));

    for name in ["alpha.rs", "beta.rs", "gamma.rs"] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", sha))
            .respond_with(DelayedContentResponse {
                arrivals: arrivals.clone(),
                path: name,
            })
            .mount(&server)
            .await;
    }

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);

    let queries = ["alpha.rs", "beta.rs", "gamma.rs"].map(|name| {
        json!({
            "owner": "a",
            "repo": "b",
            "path": name,
            "branch": sha,
            "forceRefresh": true,
            "goal": "test", "reasoning": format!("Read {name} through the GitHub bulk path."),
            "debug": true
        })
    });
    let outcome = runtime
        .execute(
            "github-bulk".into(),
            "ghGetFileContent".into(),
            json!({"queries": queries}),
        )
        .await
        .expect("GitHub bulk result");
    let rows = outcome.structured_content["results"]
        .as_array()
        .expect("GitHub result rows");
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| row["index"].as_u64())
            .collect::<Vec<_>>(),
        [Some(0), Some(1), Some(2)]
    );
    assert!(
        rows.iter().all(|row| row.get("status").is_none()),
        "{rows:?}"
    );
    {
        let arrivals = arrivals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(arrivals.len(), 3);
        assert!(
            arrivals.last().unwrap().duration_since(arrivals[0]) < Duration::from_millis(150),
            "GitHub API calls were dispatched serially: {arrivals:?}"
        );
    }

    runtime.close().await;
}

/// Verifies that three sequential single-query calls each produce exactly one
/// server-side HTTP request — `.expect(1)` on each mock endpoint is the
/// observable proof.
#[tokio::test]
async fn three_sequential_queries_each_hit_the_server() {
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

    for name in ["p.rs", "q.rs", "r.rs"] {
        let outcome = call(
            &runtime,
            "ghGetFileContent",
            json!({"owner": "a", "repo": "b", "path": name,
                   "branch": "main", "forceRefresh": true}),
        )
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
        assert_ne!(
            row_status(&outcome),
            "error",
            "{name} failed: {}",
            outcome.structured_content
        );
    }
    // WireMock verifies `.expect(1)` on drop: each content endpoint hit exactly once.

    runtime.close().await;
}

// ── ghSearchHistory integration tests ──────────────────────────────────

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
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        rendered.contains("stabilise parser"),
        "expected commit in {rendered}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_history_issues_list_via_issue_search() {
    // Plain issue listings page `is:issue` search results: GitHub's REST
    // /issues list interleaves pull requests, which left pages short.
    let server = MockServer::start().await;
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
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        rendered.contains("Memory leak"),
        "expected issue in {rendered}"
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
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(
        rendered.contains("concurrency buffering"),
        "expected PR in {rendered}"
    );
    runtime.close().await;
}

// ── ghGetHistoryItem integration tests ───────────────────────────────────

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
async fn gh_get_history_item_preserves_github_permission_reason() {
    let server = MockServer::start().await;
    let leaked_token = format!("ghp_{}", "a".repeat(37));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "message": format!("Resource protected by organization SAML SSO authorization; token {leaked_token}")
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
    .expect("permission error row");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "error");
    assert_eq!(data["httpStatus"], 403);
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|message| message.contains("SAML SSO authorization")),
        "{data}"
    );
    assert!(!data.to_string().contains(&leaked_token), "{data}");
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

/// Cloning a repository that does not exist must report the missing repo by
/// name, not the internal-sounding clone.defaultBranchUnavailable failure
/// that used to surface when default-branch resolution silently failed.
#[tokio::test]
async fn gh_clone_repo_missing_repository_reports_repo_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/ghost/nope"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("ENABLE_CLONE", "true".into()),
    ]);
    let outcome = call(
        &runtime,
        "ghCloneRepo",
        json!({"owner":"ghost","repo":"nope"}),
    )
    .await
    .expect("clone error row");

    assert_eq!(
        row_status(&outcome),
        "error",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "clone.repositoryNotFound", "{data}");
    assert_eq!(
        data["error"].as_str(),
        Some("Repository not found: ghost/nope"),
        "{data}"
    );
    let hint = data["hints"][0].as_str().expect("repo hint");
    assert!(hint.contains("ghost/nope"), "{hint}");
    runtime.close().await;
}

#[tokio::test]
async fn gh_clone_repo_preserves_metadata_auth_and_rate_limit_failures() {
    for (status, code, retryable) in [
        (401, "authentication", false),
        (429, "rateLimited", true),
        (503, "server", true),
    ] {
        let server = MockServer::start().await;
        let response =
            ResponseTemplate::new(status).set_body_json(json!({"message": "metadata unavailable"}));
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b"))
            .respond_with(response)
            .mount(&server)
            .await;
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
        let outcome = call(&runtime, "ghCloneRepo", json!({"owner":"a","repo":"b"}))
            .await
            .expect("clone metadata error row");
        let data = row_data(&outcome);
        assert_eq!(row_status(&outcome), "error", "{data}");
        assert_eq!(data["errorCode"], code, "{data}");
        assert_eq!(data["httpStatus"], status, "{data}");
        assert_eq!(data["retryable"], retryable, "{data}");
        assert!(data["hints"][0].is_string(), "{data}");
        assert!(
            !data.to_string().contains("defaultBranchUnavailable"),
            "{data}"
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        assert!(
            !workspace.home.join("tmp/clone").exists(),
            "metadata failure must return before starting Git"
        );
        runtime.close().await;
    }
}

#[tokio::test]
async fn gh_clone_repo_preserves_metadata_timeout() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(6))
                .set_body_json(json!({"default_branch":"main"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("REQUEST_TIMEOUT", "5000".into()),
    ]);
    let outcome = call(&runtime, "ghCloneRepo", json!({"owner":"a","repo":"b"}))
        .await
        .expect("clone timeout row");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "error", "{data}");
    assert_eq!(data["errorCode"], "timeout", "{data}");
    assert!(
        data["hints"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("Retry")),
        "{data}"
    );
    assert!(
        !data.to_string().contains("defaultBranchUnavailable"),
        "{data}"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    assert!(
        !workspace.home.join("tmp/clone").exists(),
        "metadata timeout must return before starting Git"
    );
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
    // The mock registry binds to loopback; opt into the SSRF escape hatch so the
    // happy-path wiring is exercised (default-block is covered by npm.rs unit tests).
    let runtime = workspace.runtime(&[("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".to_string())]);
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

// ── Audit regressions: ghGetHistoryItem pullRequest ─────────────────────────

#[tokio::test]
async fn gh_get_history_item_pull_request_without_content_passes_output_contract() {
    // Regression: the per-row `next` menu omitted required pageSize, so every
    // plain PR fetch tripped outputContractViolation. A >500-char multibyte
    // body also exercises the char-boundary-safe bodyPreview.
    let server = MockServer::start().await;
    let body = "修复并发缓冲区的内存泄漏问题。".repeat(60);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7,
            "title": "Add concurrency buffering",
            "state": "closed",
            "merged_at": null,
            "draft": false,
            "body": body,
            "user": {"login": "bob"},
            "head": {"sha": "def456", "ref": "feat/buf"},
            "base": {"ref": "main"},
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-02T00:00:00Z"
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 7}),
    )
    .await
    .expect("PR fetch must satisfy the output contract");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let pr = &row_data(&outcome)["pullRequests"][0];
    let preview = pr["bodyPreview"].as_str().expect("bodyPreview");
    assert!(preview.ends_with("..."), "{preview}");
    assert!(preview.chars().count() <= 500, "{preview}");
    let get_body = &pr["next"]["getBody"]["query"];
    // Continuations omit defaulted fields; validation restores them on replay.
    assert!(get_body.get("pageSize").is_none(), "{get_body}");
    assert!(get_body.get("minify").is_none(), "{get_body}");
    assert_eq!(get_body["content"], json!({"body": true}), "{get_body}");
    let replayed = octocode_native::contracts::validate_query("ghGetHistoryItem", get_body.clone())
        .expect("compact continuation validates");
    // pageSize has no contract default: each surface sizes its own page.
    assert!(replayed.get("pageSize").is_none(), "{replayed}");
    assert_eq!(replayed["minify"], "standard", "{replayed}");
    runtime.close().await;
}

// ── Audit regressions: ghSearchHistory ──────────────────────────────────────

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
    let server = MockServer::start().await;
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
    let server = MockServer::start().await;
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
    let server = MockServer::start().await;
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
        json!({"operation": "commit", "owner": "a", "repo": "b", "committer": "web-flow"}),
    )
    .await
    .expect("commit list");
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(rendered.contains("merged via UI"), "{rendered}");
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_file_content_on_directory_returns_tree_recovery() {
    let server = MockServer::start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"lib.rs","path":"src/lib.rs","type":"file","size":3,"sha":"1"}
        ])))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"src","branch":"main","forceRefresh":true}),
    )
    .await
    .expect("directory read is a row error, not a contract violation");
    assert_eq!(
        row_status(&outcome),
        "error",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert!(
        data["error"]
            .as_str()
            .unwrap_or_default()
            .contains("is a directory"),
        "{data}"
    );
    assert_eq!(data["next"]["viewTree"]["query"]["path"], "src", "{data}");
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_concise_repositories_are_contract_valid() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1,
            "incomplete_results": false,
            "items": [{"full_name":"o/r","name":"r","html_url":"https://x","default_branch":"main"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchRepo",
        json!({"keywords":["x"],"concise":true}),
    )
    .await
    .expect("concise repositories");
    assert_eq!(row_data(&outcome)["repositories"], json!(["o/r"]));
    runtime.close().await;
}

/// A primary rate limit surfaces as a contract-valid error row carrying
/// rate-limit metadata (incl. resource), and the blocking fact is mirrored to
/// `<home>/tmp/ratelimit/` for other processes.
#[tokio::test]
async fn gh_primary_rate_limit_is_contract_valid_and_persisted_for_other_processes() {
    let server = MockServer::start().await;
    let reset = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .insert_header("x-ratelimit-resource", "core")
                .set_body_json(json!({"message": "API rate limit exceeded"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_STORAGE_MODE", "persistent".into()),
    ]);
    let sha = "0123456789abcdef0123456789abcdef01234567";
    for name in ["one.rs", "two.rs"] {
        let outcome = call(
            &runtime,
            "ghGetFileContent",
            json!({"owner": "a", "repo": "b", "path": name, "branch": sha, "forceRefresh": true}),
        )
        .await
        .expect("row-level error");
        assert_eq!(
            row_status(&outcome),
            "error",
            "{}",
            outcome.structured_content
        );
        let data = row_data(&outcome);
        assert!(
            !outcome
                .structured_content
                .to_string()
                .contains("outputContractViolation"),
            "{}",
            outcome.structured_content
        );
        assert_eq!(data["errorCode"], "rateLimited", "{data}");
        assert_eq!(data["rateLimit"]["resetEpochSeconds"], reset, "{data}");
        assert_eq!(data["rateLimit"]["resource"], "core", "{data}");
    }
    // The second read was refused before sending (mock expects one hit).
    let dir = workspace.home.join("tmp").join("ratelimit");
    let files: Vec<_> = std::fs::read_dir(&dir)
        .expect("ratelimit dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".json"))
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
    let state: serde_json::Value =
        serde_json::from_slice(&std::fs::read(files[0].path()).unwrap()).unwrap();
    assert_eq!(state["buckets"]["core"]["remaining"], 0, "{state}");
    assert_eq!(state["buckets"]["core"]["reset"], reset, "{state}");
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_capped_file_scan_is_not_a_complete_count() {
    // A path-scoped scan that stops at the file-batch cap must not
    // report its file count as complete.
    let server = MockServer::start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    let commit_path = format!("/api/v3/repos/a/b/commits/{sha}");
    Mock::given(method("GET"))
        .and(path(commit_path.clone()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header(
                    "link",
                    format!("<{}{commit_path}?page=999>; rel=\"next\"", server.uri()).as_str(),
                )
                .set_body_json(json!({
                    "sha": sha,
                    "commit": {
                        "message": "big change",
                        "author": {"name": "Alice", "date": "2024-01-01T00:00:00Z"}
                    },
                    "files": [{"filename": "other/file.rs", "status": "modified",
                               "additions": 1, "deletions": 0}]
                })),
        )
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "commit", "owner": "a", "repo": "b", "ref": sha, "path": "wanted/"}),
    )
    .await
    .expect("capped commit scan");
    let data = row_data(&outcome);
    assert_eq!(
        data["changedFilesCountScope"], "partial",
        "{}",
        outcome.structured_content
    );
    let rendered = serde_json::to_string(data).expect("json");
    assert!(rendered.contains("terminalLimit"), "{rendered}");
    assert!(!rendered.contains("\"complete\""), "{rendered}");
    runtime.close().await;
}

// ── Validation-bench regressions (2026-09-30) ───────────────────────────────

/// Every hint string anywhere in a row.
fn all_hints(value: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if key == "hints"
                    && let Some(hints) = child.as_array()
                {
                    out.extend(hints.iter().filter_map(|h| h.as_str().map(str::to_owned)));
                }
                out.extend(all_hints(child));
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| out.extend(all_hints(v))),
        _ => {}
    }
    out
}

/// D7: authentication and binary-file hints arrive whole, never cut
/// mid-sentence with an ellipsis.
#[tokio::test]
async fn github_recovery_hints_are_never_cut_mid_sentence() {
    let server = MockServer::start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/locked"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message":"Bad credentials"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/contents/img.png")))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file", "encoding": "base64",
            "content": STANDARD.encode([0x89u8, b'P', b'N', b'G', 0, 0, 1]),
            "size": 7, "sha": "f".repeat(40), "path": "img.png"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for query in [
        json!({"owner":"a","repo":"b","path":"README.md","branch":"locked","debug":false}),
        json!({"owner":"a","repo":"b","path":"img.png","branch":sha,"debug":false}),
    ] {
        let outcome = call(&runtime, "ghGetFileContent", query)
            .await
            .expect("error row");
        let data = row_data(&outcome);
        let hints = all_hints(data);
        assert!(!hints.is_empty(), "{data}");
        for hint in hints {
            assert!(!hint.contains('…'), "truncated hint {hint:?} in {data}");
        }
    }
    runtime.close().await;
}

/// D8: a repository-level 404 (the ref resolution itself 404s) says the
/// repository is missing, private, or inaccessible to the token; it does not
/// blame path case, offer a viewTree of the same missing repository, or echo
/// GitHub's unrelated commits documentation link.
#[tokio::test]
async fn gh_file_read_on_a_missing_repository_reports_repository_access() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/ghost/nope/commits/HEAD"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "message":"Not Found",
            "documentation_url":"https://docs.github.com/rest/commits/commits#get-a-commit"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"ghost","repo":"nope","path":"README.md","debug":false}),
    )
    .await
    .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    let rendered = data.to_string();
    assert!(rendered.contains("private"), "{rendered}");
    assert!(!rendered.contains("exact case"), "{rendered}");
    assert!(data.pointer("/next/viewTree").is_none(), "{rendered}");
    assert!(!rendered.contains("commits#get-a-commit"), "{rendered}");
    runtime.close().await;
}

/// D11: a pull-request read of a number that is an issue offers the issue
/// read instead of a generic not-found.
#[tokio::test]
async fn gh_pull_request_read_of_an_issue_number_offers_read_issue() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 9, "title": "A bug", "state": "open", "user": {"login": "x"}
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,"debug":false}),
    )
    .await
    .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "error", "{data}");
    let read = &data["next"]["readIssue"]["query"];
    assert_eq!(read["operation"], "issue", "{data}");
    assert_eq!(read["number"], 9, "{data}");
    assert!(
        data["error"].as_str().unwrap_or("").contains("issue"),
        "{data}"
    );
    runtime.close().await;
}

const BASE_SHA: &str = "1111111aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEAD_SHA: &str = "2222222bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// A 150-commit comparison at GitHub's 300-file cap: commit pages of 100.
async fn mount_capped_compare(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/v2"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HEAD_SHA))
        .mount(server)
        .await;
    let files = (0..300)
        .map(|n| json!({"filename":format!("src/f{n:03}.rs"),"status":"modified","additions":1,"deletions":1,"sha":"3".repeat(40)}))
        .collect::<Vec<_>>();
    for (page, count) in [(1, 100), (2, 50)] {
        let commits = (0..count)
            .map(|n| json!({"sha":format!("{:040x}", page * 1000 + n),"commit":{"message":format!("c{n}"),"author":{"name":"a","date":"2024-01-01T00:00:00Z"}}}))
            .collect::<Vec<_>>();
        Mock::given(method("GET"))
            .and(path_regex(format!(
                "^/api/v3/repos/a/b/compare/(v1|{BASE_SHA})\\.\\.\\.{HEAD_SHA}$"
            )))
            .and(query_param("page", page.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status":"ahead","ahead_by":150,"behind_by":0,"total_commits":150,
                "permalink_url":"https://github.com/a/b/compare/a:1111111...a:2222222",
                "base_commit":{"sha":BASE_SHA},"merge_base_commit":{"sha":BASE_SHA},
                "commits":commits,"files":files
            })))
            .mount(server)
            .await;
    }
}

/// D4/D5: a comparison resolves `head` to the commit it read and pins its
/// continuations to it; its 300-file list is not a complete count; a file
/// page carries only files, and a commit page only commits.
#[tokio::test]
async fn compare_pages_carry_one_collection_each_and_pin_both_refs() {
    let server = MockServer::start().await;
    mount_capped_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let first = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2","pageSize":100,"debug":false}),
    )
    .await
    .expect("compare");
    let data = row_data(&first).clone();
    assert_eq!(data["head"], HEAD_SHA, "{data}");
    assert_eq!(data["base"], BASE_SHA, "{data}");
    assert_ne!(data["filesPagination"]["countScope"], "complete", "{data}");
    let commit_page = data["next"]["nextPage"]["query"].clone();
    assert!(commit_page.get("filePage").is_none(), "{commit_page}");
    assert_eq!(commit_page["head"], HEAD_SHA, "{commit_page}");
    let file_page = data["next"]["nextFilePage"]["query"].clone();
    assert_eq!(file_page["head"], HEAD_SHA, "{file_page}");

    let files = call(&runtime, "ghGetHistoryItem", file_page)
        .await
        .expect("file page");
    let files = row_data(&files);
    assert!(
        files.get("commits").is_none(),
        "file page repeats commits: {files}"
    );
    assert_eq!(
        files["files"].as_array().map(Vec::len),
        Some(100),
        "{files}"
    );

    let commits = call(&runtime, "ghGetHistoryItem", commit_page)
        .await
        .expect("commit page");
    let commits = row_data(&commits);
    assert!(commits.get("files").is_none(), "{commits}");
    assert_eq!(
        commits["commits"].as_array().map(Vec::len),
        Some(50),
        "{commits}"
    );
    runtime.close().await;
}

/// D6: a path-scoped commit read counts only the files in scope and labels
/// the whole-commit line totals as such.
#[tokio::test]
async fn path_scoped_commit_labels_whole_commit_totals() {
    let server = MockServer::start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "commit": {"message": "two files", "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}},
            "stats": {"additions": 30, "deletions": 5, "total": 35},
            "files": [
                {"filename": "src/a.rs", "status": "modified", "additions": 10, "deletions": 5},
                {"filename": "docs/b.md", "status": "modified", "additions": 20, "deletions": 0}
            ]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"path":"src/","debug":false}),
    )
    .await
    .expect("commit");
    let data = row_data(&outcome);
    assert_eq!(data["changedFiles"], 1, "{data}");
    assert!(
        data.get("additions").is_none(),
        "unlabeled whole-commit total: {data}"
    );
    assert_eq!(data["commitTotals"]["additions"], 30, "{data}");
    assert_eq!(data["commitTotals"]["changedFiles"], 2, "{data}");
    runtime.close().await;
}

/// orangu: a whole-file read of a large file stays under the host output
/// cap (about 40k chars) and continues instead of returning 74–82k.
#[tokio::test]
async fn gh_full_content_first_page_stays_under_the_host_output_cap() {
    let server = MockServer::start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let text = (0..6000)
        .map(|n| format!("fn function_number_{n}(value: usize) -> usize {{ value * {n} }}\n"))
        .collect::<String>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fbig.rs"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file", "encoding": "base64", "content": STANDARD.encode(&text),
            "size": text.len(), "sha": "f".repeat(40), "path": "src/big.rs"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"src/big.rs","branch":sha,"fullContent":true,"debug":false}),
    )
    .await
    .expect("full content");
    let rendered = outcome.structured_content.to_string();
    assert!(
        rendered.chars().count() <= 40_000,
        "first page is {} chars",
        rendered.chars().count()
    );
    assert!(
        rendered.contains("\"continue\"") || rendered.contains("responsePagination"),
        "no continuation: {}",
        &rendered[..rendered.len().min(2000)]
    );
    runtime.close().await;
}
