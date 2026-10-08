// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::panic)]

use crate::support;

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
async fn malformed_remote_regex_keeps_original_error_and_required_file_path() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/README.md"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("example source\n"),"size":15,"sha":"f".repeat(40),"path":"README.md"
        })))
        .mount(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for debug in [false, true] {
        let out = call(&runtime, "ghGetFileContent", json!({"owner":"a","repo":"b","path":"README.md","ref":sha,"matchString":"(","regex":"rust","debug":debug})).await.expect("error row");
        assert_eq!(row_status(&out), "error", "{}", out.structured_content);
        let data = row_data(&out);
        assert_ne!(data["errorCode"], "outputContractViolation", "{data}");
        assert_eq!(data["path"], "README.md", "{data}");
        assert!(data.to_string().contains("Invalid regex pattern"), "{data}");
    }
    runtime.close().await;
}

#[tokio::test]
async fn github_file_read_goes_through_execute_and_redacts() {
    let server = MockServer::builder().start().await;
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
            "ref": "main",
            "forceRefresh": true,
            "unit": "lines",
            "length": 2
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
    let file = row_data(&outcome);
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
    let server = MockServer::builder().start().await;
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
    // Only the path and coverage: kind/source/cached are constants, hasMore
    // restates `complete`, and the ref is the snapshot the path names.
    let mut keys: Vec<&str> = data["location"]
        .as_object()
        .expect("location")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["complete", "localPath"], "{data}");
    let exhausted = call(&runtime, "ghStructure", json!({"owner":"a","repo":"b","ref":sha,"pageSize":1,"materialize":true,"materializeOffset":1})).await.expect("boundary offset");
    let boundary = row_data(&exhausted);
    assert_eq!(boundary["pagination"]["hasMore"], true, "{boundary}");
    // GS6b: a materialize resumed inside its page does not re-send the
    // entries its first call listed; `location` anchors it.
    let resumed = call(
        &runtime,
        "ghStructure",
        json!({"owner":"a","repo":"b","ref":sha,"materialize":true,"materializeOffset":1}),
    )
    .await
    .expect("mid-page offset");
    let resumed = row_data(&resumed);
    assert!(resumed.get("entries").is_none(), "{resumed}");
    assert!(
        resumed["location"]["localPath"].as_str().is_some(),
        "{resumed}"
    );
    assert_eq!(resumed["location"]["complete"], true, "{resumed}");
    let next = &boundary["next"]["continueMaterialize"]["query"]["queries"][0];
    assert_eq!(next["ref"], sha, "{boundary}");
    assert_eq!(next["page"], 2, "{boundary}");
    assert_eq!(next["materializeOffset"], 0, "{boundary}");
    let last = call(&runtime, "ghStructure", next.clone())
        .await
        .expect("replay boundary continuation");
    let completed = row_data(&last);
    assert_eq!(completed["location"]["complete"], true, "{completed}");
    runtime.close().await;
}

/// E8: a materialize that lists folders below its depth without writing
/// them is not complete, and `next.expandDepth` writes them.
#[tokio::test]
async fn github_tree_materialize_with_unwritten_folders_is_not_complete() {
    let server = MockServer::builder().start().await;
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
            {"name":"router","path":"router","type":"dir","sha":"2".repeat(40)}
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("/api/v3/repos/a/b/commits/(main|HEAD)$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/one.rs"))
        .and(query_param("ref", sha))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file", "encoding": "base64", "content": STANDARD.encode("fn x(){}\n"),
            "size": 8, "sha": "a".repeat(40), "path": "one.rs"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"owner": "a", "repo": "b", "materialize": true}),
    )
    .await
    .expect("tree materialize");
    let data = row_data(&outcome);
    assert_eq!(data["location"]["complete"], false, "{data}");
    assert_eq!(data["isPartial"], true, "{data}");
    assert!(
        data["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&json!("materializeDepth"))),
        "{data}"
    );
    let deeper = &data["next"]["expandDepth"];
    assert_eq!(deeper["tool"], "ghStructure", "{data}");
    let row = &deeper["query"]["queries"][0];
    assert_eq!(row["maxDepth"], 20, "{data}");
    assert_eq!(row["materialize"], true, "{data}");
    assert_eq!(row["ref"], sha, "{data}");
    runtime.close().await;
}

/// E21/D4: ghSearchCode on a repository that does not exist is notFound
/// (exit 3), not an empty partial search.
#[tokio::test]
async fn gh_search_code_on_a_missing_repository_is_not_found() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count":0,"incomplete_results":true,"items":[]})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/psf/zz-nope-repo"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchCode",
        json!({"owner":"psf","repo":"zz-nope-repo","keywords":["needle"]}),
    )
    .await
    .expect("search");
    let row = &outcome.structured_content["results"][0];
    assert_eq!(
        row_status(&outcome),
        "error",
        "{}",
        outcome.structured_content
    );
    assert_eq!(row["data"]["errorCode"], "notFound", "{row}");
    runtime.close().await;
}

#[tokio::test]
async fn github_clone_is_cli_only() {
    let workspace = Workspace::new();
    let cli = workspace.runtime(&[]);
    assert!(cli.is_available("ghCloneRepo"));
    let mcp_call = cli
        .execute_mcp(
            "mcp-clone".into(),
            "ghCloneRepo".into(),
            json!({"queries":[{"owner":"a","repo":"b","mainGoal": "test", "reasoning":"check MCP gate"}]}),
        )
        .await
        .expect_err("MCP channel cannot clone through a CLI runtime");
    assert_eq!(mcp_call.code, "toolUnavailable");
    cli.close().await;

    let mut input = workspace.config(&[]);
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
    let server = MockServer::builder().start().await;
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
            "ref": sha,
            "forceRefresh": true,
            "mainGoal": "test", "reasoning": format!("Read {name} through the GitHub bulk path."),
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
    let server = MockServer::builder().start().await;
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
                   "ref": "main", "forceRefresh": true}),
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
    let server = MockServer::builder().start().await;
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
    let server = MockServer::builder().start().await;
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
    // D9: a SHA ref is not restated beside the same sha.
    let data = row_data(&outcome);
    assert_eq!(data["sha"], sha, "{data}");
    assert!(data.get("ref").is_none(), "{data}");
    assert_eq!(
        data["hints"]["findPullRequest"]["query"]["queries"][0]["keywords"],
        json!([sha]),
        "{data}"
    );
    runtime.close().await;
}

/// D9: a squash-merge headline names its pull request: read it directly
/// instead of searching by SHA.
#[tokio::test]
async fn squash_merge_commit_reads_its_pull_request_directly() {
    let server = MockServer::builder().start().await;
    let sha = "30df32a13f9cf5f129b913b967ff6f137c5511d6";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "commit": {
                "message": "io: use `spawn_mandatory_blocking` (#8506)\n\nCo-authored-by: x",
                "author": {"name": "Alice", "date": "2024-01-01T00:00:00Z"}
            },
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
    .expect("commit read");
    let data = row_data(&outcome);
    let read = &data["hints"]["readPullRequest"];
    assert_eq!(read["tool"], "ghGetHistoryItem", "{data}");
    assert_eq!(read["query"]["queries"][0]["number"], 8506, "{data}");
    assert!(data["hints"].get("findPullRequest").is_none(), "{data}");
    assert!(
        !outcome
            .structured_content
            .to_string()
            .contains("outputContractViolation"),
        "{}",
        outcome.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_issue_fetches_via_rest() {
    let server = MockServer::builder().start().await;
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
    let server = MockServer::builder().start().await;
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
    let server = MockServer::builder().start().await;
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

/// ghCloneRepo: no GitHub API call precedes git. A missing or
/// inaccessible repository is classified from the metadata API only after
/// git fails (unit-tested in `runtime::github`); here the endpoint cannot
/// be cloned at all, and the API is never asked.
#[tokio::test]
async fn gh_clone_repo_makes_no_metadata_call_before_git() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/ghost/nope"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": "Not Found"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghCloneRepo",
        json!({"owner":"ghost","repo":"nope"}),
    )
    .await
    .expect("clone error row");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "error", "{data}");
    assert_eq!(data["errorCode"], "configuration", "{data}");
    assert!(
        !data.to_string().contains("defaultBranchUnavailable"),
        "{data}"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 0);
    runtime.close().await;
}

#[tokio::test]
async fn artifact_search_lookup_goes_through_execute() {
    let server = MockServer::builder().start().await;
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
            "ecosystem": "npm",
            "packageName": "left-pad",
            "registryUrl": server.uri()
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

/// npm credentials come from the runtime's resolved environment, never the
/// process's: each runtime's own userconfig (or its `HOME/.npmrc`) and its
/// own `${VAR}` values decide the Authorization header a registry receives.
#[tokio::test]
async fn artifact_search_npm_credentials_follow_the_runtime_env() {
    let workspace = Workspace::new();
    let empty = workspace.home.join("empty.npmrc");
    std::fs::write(&empty, "").expect("empty npmrc");
    for (case, expected) in [
        ("userconfig", Some("Bearer from-runtime-env")),
        ("home", Some("Bearer from-runtime-home")),
        ("empty", None),
    ] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/runtime-env-auth/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "name": "runtime-env-auth",
                "version": "1.0.0"
            })))
            .mount(&server)
            .await;
        let port = server.address().port();
        let injected = workspace.home.join("injected.npmrc");
        std::fs::write(
            &injected,
            format!("//127.0.0.1:{port}/:_authToken=${{RUNTIME_NPM_TOKEN}}\n"),
        )
        .expect("injected npmrc");
        std::fs::write(
            workspace.home.join(".npmrc"),
            format!("//127.0.0.1:{port}/:_authToken=from-runtime-home\n"),
        )
        .expect("home npmrc");
        let mut settings = vec![
            ("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".to_owned()),
            ("OCTOCODE_STORAGE_MODE", "memory".to_owned()),
            ("RUNTIME_NPM_TOKEN", "from-runtime-env".to_owned()),
            ("HOME", workspace.home.to_string_lossy().into_owned()),
        ];
        match case {
            "userconfig" => settings.push((
                "NPM_CONFIG_USERCONFIG",
                injected.to_string_lossy().into_owned(),
            )),
            "empty" => settings.push((
                "npm_config_userconfig",
                empty.to_string_lossy().into_owned(),
            )),
            _ => {}
        }
        let runtime = workspace.runtime(&settings);
        let outcome = call(
            &runtime,
            "artifactSearch",
            json!({"ecosystem": "npm", "packageName": "runtime-env-auth", "registryUrl": server.uri()}),
        )
        .await
        .expect("artifactSearch");
        assert_eq!(
            row_status(&outcome),
            "success",
            "{case}: {}",
            outcome.structured_content
        );
        runtime.close().await;
        let seen: Vec<Option<String>> = server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .map(|request| {
                request
                    .headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_owned)
            })
            .collect();
        assert!(!seen.is_empty(), "{case}");
        assert!(
            seen.iter().all(|header| header.as_deref() == expected),
            "{case}: {seen:?}"
        );
    }
}

/// A workspace `.octocode/.env` is repository-controlled: it never chooses
/// the npm userconfig, so a repository cannot point credential discovery at
/// its own npmrc and route a user's token to a registry it names.
#[tokio::test]
async fn artifact_search_npm_userconfig_never_comes_from_the_workspace_env() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/runtime-env-auth/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": "runtime-env-auth",
            "version": "1.0.0"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let port = server.address().port();
    let planted = workspace.write(
        "planted.npmrc",
        format!("//127.0.0.1:{port}/:_authToken=${{GITHUB_TOKEN}}\n"),
    );
    let mut input = workspace.config(&[
        ("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".to_owned()),
        ("OCTOCODE_STORAGE_MODE", "memory".to_owned()),
        ("HOME", workspace.home.to_string_lossy().into_owned()),
    ]);
    input.project_env = octocode_native::config::FileInput::Read {
        path: workspace.workspace.join(".octocode/.env"),
        text: format!("NPM_CONFIG_USERCONFIG={}\n", planted.display()),
    };
    let runtime = octocode_native::runtime::ToolRuntime::new(input).expect("runtime");
    let outcome = call(
        &runtime,
        "artifactSearch",
        json!({"ecosystem": "npm", "packageName": "runtime-env-auth", "registryUrl": server.uri()}),
    )
    .await
    .expect("artifactSearch");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    runtime.close().await;
    let requests = server.received_requests().await.unwrap_or_default();
    assert!(!requests.is_empty());
    assert!(
        requests
            .iter()
            .all(|request| request.headers.get("authorization").is_none()),
        "a workspace .env steered npm credentials"
    );
}

// ── Audit regressions: ghGetHistoryItem pullRequest ─────────────────────────

#[tokio::test]
async fn gh_get_history_item_pull_request_without_content_passes_output_contract() {
    // Regression: the per-row `next` menu omitted required pageSize, so every
    // plain PR fetch tripped outputContractViolation. The summary row carries
    // a body preview: the multibyte body rides the file-list read whole.
    let server = MockServer::builder().start().await;
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
    // The summary previews the body (whole multibyte chars); the file-list
    // read carries it whole.
    assert!(pr.get("body").is_none(), "{pr}");
    let preview = pr["bodyPreview"].as_str().expect("preview");
    assert!(
        preview.starts_with("修复并发") && preview.ends_with('…'),
        "{pr}"
    );
    // The body rides the file-list read.
    let get_body = &row_data(&outcome)["hints"]["readFiles"]["query"]["queries"][0];
    // Continuations omit defaulted fields; validation restores them on replay.
    assert!(get_body.get("pageSize").is_none(), "{get_body}");
    assert!(get_body.get("minify").is_none(), "{get_body}");
    assert_eq!(get_body["sections"], json!(["body", "files"]), "{get_body}");
    let replayed = octocode_native::contracts::validate_query("ghGetHistoryItem", get_body.clone())
        .expect("compact continuation validates");
    // pageSize has no contract default: each surface sizes its own page.
    assert!(replayed.get("pageSize").is_none(), "{replayed}");
    assert_eq!(replayed["minify"], "standard", "{replayed}");
    runtime.close().await;
}

// ── Audit regressions: ghSearchHistory ──────────────────────────────────────

#[tokio::test]
async fn history_repository_without_owner_is_rejected_before_provider_requests() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json!({"operation": "pullRequest", "repo": "b"}),
    )
    .await
    .expect_err("repository-only scope must fail native contract validation");
    assert_eq!(outcome.code, "invalidInput");
    let issues = outcome.validation_issues.as_ref().expect("typed issues");
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].rule_id, "history.repository-scope");
    assert_eq!(
        serde_json::to_value(&issues[0].path).unwrap(),
        json!(["queries", "0", "owner"])
    );
    assert!(issues[0].message.contains("repo requires owner"));
    let payload = outcome.payload.as_ref().expect("transport error payload");
    assert_eq!(payload["kind"], "octocode.toolError");
    assert_eq!(payload["tool"], "ghSearchHistory");
    assert_eq!(payload["errorCode"], "invalidInput");
    assert!(
        server
            .received_requests()
            .await
            .expect("requests")
            .is_empty()
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
    let rendered = serde_json::to_string(row_data(&outcome)).expect("json");
    assert!(rendered.contains("merged via UI"), "{rendered}");
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_file_content_on_directory_returns_tree_recovery() {
    let server = MockServer::builder().start().await;
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
        json!({"owner":"a","repo":"b","path":"src","ref":"main","forceRefresh":true}),
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
    assert_eq!(
        data["hints"]["viewTree"]["query"]["queries"][0]["path"], "src",
        "{data}"
    );
    runtime.close().await;
}

/// D7: a binary file is a request the text read cannot serve: it names the
/// size and blob SHA under an invalid-input code, not a decode failure.
#[tokio::test]
async fn gh_file_read_of_binary_content_reports_size_and_blob() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let blob = "b".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/icon.png"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type": "file",
            "encoding": "base64",
            "content": STANDARD.encode(b"\x89PNG\0\0\0\rIHDR"),
            "size": 12,
            "sha": blob,
            "path": "icon.png"
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"icon.png","ref":sha,"forceRefresh":true}),
    )
    .await
    .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "invalidInput", "{data}");
    let message = data["error"].as_str().unwrap_or_default();
    assert!(message.contains("12 bytes"), "{data}");
    assert!(message.contains(&blob), "{data}");
    assert!(
        octocode_native::response::rows::is_invalid_input_code("invalidInput"),
        "binary reads must exit as caller errors"
    );
    runtime.close().await;
}

/// D4: a wrong-case path recovers to the case-corrected file, and a missing
/// file to its nearest existing directory, never to another missing path.
#[tokio::test]
async fn gh_file_read_of_a_missing_path_recovers_to_what_exists() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    let listing = |entries: serde_json::Value| ResponseTemplate::new(200).set_body_json(entries);
    for (dir, entries) in [
        (
            "",
            json!([
                {"name":"tokio","path":"tokio","type":"dir"},
                {"name":"README.md","path":"README.md","type":"file","size":3,"sha":"1"}
            ]),
        ),
        (
            "/tokio",
            json!([{"name":"src","path":"tokio/src","type":"dir"}]),
        ),
        (
            "/tokio%2Fsrc",
            json!([
                {"name":"lib.rs","path":"tokio/src/lib.rs","type":"file","size":3,"sha":"2"}
            ]),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents{dir}")))
            .respond_with(listing(entries))
            .mount(&server)
            .await;
    }
    let not_found = || ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"}));
    for missing in [
        "Tokio%2Fsrc%2Flib.rs",
        "Tokio%2Fsrc",
        "Tokio",
        "tokio%2Fsrc%2Fnope.rs",
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{missing}")))
            .respond_with(not_found())
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let read =
        |file: &str| json!({"owner":"a","repo":"b","path":file,"ref":"main","forceRefresh":true});
    let outcome = call(&runtime, "ghGetFileContent", read("Tokio/src/lib.rs"))
        .await
        .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    assert_eq!(
        data["hints"]["read"]["query"]["queries"][0]["path"], "tokio/src/lib.rs",
        "{data}"
    );
    assert_eq!(
        data["hints"]["read"]["query"]["queries"][0]["ref"], "main",
        "{data}"
    );
    assert_eq!(
        data["hints"]["viewTree"]["query"]["queries"][0]["path"], "tokio/src",
        "{data}"
    );
    let outcome = call(&runtime, "ghGetFileContent", read("tokio/src/nope.rs"))
        .await
        .expect("error row");
    let data = row_data(&outcome);
    assert!(data["hints"].get("read").is_none(), "{data}");
    assert_eq!(
        data["hints"]["viewTree"]["query"]["queries"][0]["path"], "tokio/src",
        "{data}"
    );
    // R4: a hint is not proof, so no continuation carries confidence:"exact".
    assert!(
        data["hints"]["viewTree"].get("confidence").is_none(),
        "{data}"
    );
    runtime.close().await;
}

/// D9: a ghStructure listing of a missing path recovers like
/// ghGetFileContent: `next.viewTree` lists the nearest existing directory
/// (case-corrected), never another missing path.
#[tokio::test]
async fn gh_structure_of_a_missing_path_recovers_to_the_nearest_directory() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"sha": sha})))
        .mount(&server)
        .await;
    let listing = |entries: serde_json::Value| ResponseTemplate::new(200).set_body_json(entries);
    for (dir, entries) in [
        ("", json!([{"name":"src","path":"src","type":"dir"}])),
        (
            "/src",
            json!([{"name":"Tools","path":"src/Tools","type":"dir"}]),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents{dir}")))
            .respond_with(listing(entries))
            .mount(&server)
            .await;
    }
    let not_found = || ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"}));
    for missing in [
        "src%2Fnope",
        "src%2Ftools",
        "no",
        "no%2Fsuch",
        "no%2Fsuch%2Fdir",
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{missing}")))
            .respond_with(not_found())
            .mount(&server)
            .await;
    }
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let tree = |dir: &str| json!({"owner":"a","repo":"b","path":dir,"ref":"main"});
    for (requested, nearest) in [
        ("src/nope", "src"),
        ("src/tools", "src/Tools"),
        ("no/such/dir", "."),
    ] {
        let outcome = call(&runtime, "ghStructure", tree(requested))
            .await
            .expect("error row, not a contract violation");
        assert_eq!(
            row_status(&outcome),
            "error",
            "{}",
            outcome.structured_content
        );
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{data}");
        assert_eq!(data["hints"]["viewTree"]["tool"], "ghStructure", "{data}");
        assert_eq!(
            data["hints"]["viewTree"]["query"]["queries"][0]["path"], nearest,
            "{data}"
        );
        assert_eq!(
            data["hints"]["viewTree"]["query"]["queries"][0]["ref"], "main",
            "{data}"
        );
        // R4: a hint is not proof, so no continuation carries confidence:"exact".
        assert!(
            data["hints"]["viewTree"].get("confidence").is_none(),
            "{data}"
        );
        assert!(
            data["error"]
                .as_str()
                .unwrap_or_default()
                .contains(requested),
            "{data}"
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn gh_search_concise_repositories_are_contract_valid() {
    let server = MockServer::builder().start().await;
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
    // The top repository's tree lead is a hint, not a page.
    let lead = &row_data(&outcome)["hints"]["viewRepo"];
    assert_eq!(lead["tool"], "ghStructure", "{}", row_data(&outcome));
    assert_eq!(
        (
            &lead["query"]["queries"][0]["owner"],
            &lead["query"]["queries"][0]["repo"]
        ),
        (&json!("o"), &json!("r"))
    );
    assert!(row_data(&outcome).get("next").is_none());
    runtime.close().await;
}

/// A primary rate limit surfaces as a contract-valid error row carrying
/// rate-limit metadata (incl. resource), and the blocking fact is mirrored to
/// `<home>/tmp/ratelimit/` for other processes.
#[tokio::test]
async fn gh_primary_rate_limit_is_contract_valid_and_persisted_for_other_processes() {
    let server = MockServer::builder().start().await;
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
            json!({"owner": "a", "repo": "b", "path": name, "ref": sha, "forceRefresh": true}),
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
        // D6: an authenticated caller is not told to authenticate.
        assert!(!data.to_string().contains("auth login"), "{data}");
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

/// D6: only an anonymous caller is told that authenticating raises quota.
#[tokio::test]
async fn anonymous_rate_limit_advises_authentication() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "4102444800")
                .set_body_json(json!({"message": "API rate limit exceeded"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("GITHUB_TOKEN", String::new()),
    ]);
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner": "a", "repo": "b", "path": "x.rs", "ref": sha, "forceRefresh": true}),
    )
    .await
    .expect("row-level error");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "rateLimited", "{data}");
    assert!(
        data["hints"]["text"][0]
            .as_str()
            .is_some_and(|hint| hint.contains("auth login")),
        "{data}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_capped_file_scan_is_not_a_complete_count() {
    // A path-scoped scan that stops at the file-batch cap must not
    // report its file count as complete.
    let server = MockServer::builder().start().await;
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

// ── Validation-bench regressions ───────────────────────────────

/// Every hint string anywhere in a row.
fn all_hints(value: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if key == "hints"
                    && let Some(hints) = child.get("text").and_then(|text| text.as_array())
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
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/locked"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message":"Bad credentials"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/img.png"))
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
        json!({"owner":"a","repo":"b","path":"README.md","ref":"locked","debug":false}),
        json!({"owner":"a","repo":"b","path":"img.png","ref":sha,"debug":false}),
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
    let server = MockServer::builder().start().await;
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
    assert!(data.pointer("/hints/viewTree").is_none(), "{rendered}");
    assert!(!rendered.contains("commits#get-a-commit"), "{rendered}");
    runtime.close().await;
}

/// A failed read's provider status, retry flag, request id and
/// documentation link are support diagnostics (debug field class): a
/// default row drops them; `debug: true` keeps them.
#[tokio::test]
async fn gh_file_error_keeps_provider_diagnostics_for_debug_only() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src%2Fmissing.rs"))
        .respond_with(
            ResponseTemplate::new(404)
                .insert_header("x-github-request-id", "R1")
                .set_body_json(json!({
                    "message":"Not Found",
                    "documentation_url":"https://docs.github.com/rest/repos/contents#get-repository-content"
                })),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for debug in [false, true] {
        let outcome = call(
            &runtime,
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"src/missing.rs","ref":sha,"debug":debug}),
        )
        .await
        .expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{data}");
        assert_eq!(data.get("httpStatus").is_some(), debug, "{data}");
        // N7: a 404 is not retryable, so no mode states `retryable`.
        assert!(data.get("retryable").is_none(), "{data}");
        assert_eq!(data.get("requestId").is_some(), debug, "{data}");
        assert_eq!(data.get("documentationUrl").is_some(), debug, "{data}");
    }
    runtime.close().await;
}

/// D11: a pull-request read of a number that is an issue offers the issue
/// read instead of a generic not-found.
#[tokio::test]
async fn gh_pull_request_read_of_an_issue_number_offers_read_issue() {
    let server = MockServer::builder().start().await;
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
    let read = &data["hints"]["readIssue"]["query"]["queries"][0];
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
            .map(|n| json!({"sha":format!("{:040x}", page * 1000 + n),"commit":{"message":format!("c{n}\n\nWhy: detail {n}"),"author":{"name":"a","date":"2024-01-01T00:00:00Z"}}}))
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
    let server = MockServer::builder().start().await;
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
    assert_ne!(data["filePagination"]["countScope"], "complete", "{data}");
    // HI12(a): the commit list names each commit by its headline; the
    // first one with more message leads to its whole-message read.
    assert_eq!(data["commits"][0]["messageHeadline"], "c0", "{data}");
    assert!(data["commits"][0].get("message").is_none(), "{data}");
    assert_eq!(
        data["hints"]["readCommit"]["query"]["queries"][0]["ref"], data["commits"][0]["sha"],
        "{data}"
    );
    // P6: the commit list is count-cut with a known total (`totalCommits`):
    // its page states the total and the page count.
    assert_eq!(data["pagination"]["totalItems"], 150, "{data}");
    assert_eq!(data["pagination"]["totalPages"], 2, "{data}");
    let commit_page = data["next"]["nextPage"]["query"]["queries"][0].clone();
    assert!(commit_page.get("filePage").is_none(), "{commit_page}");
    assert_eq!(commit_page["head"], HEAD_SHA, "{commit_page}");
    let file_page = data["next"]["nextFilePage"]["query"]["queries"][0].clone();
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

/// A path scope past GitHub's 300-file compare list is not "unchanged": the
/// row warns and leads to the path's commits up to the pinned head.
#[tokio::test]
async fn capped_compare_with_a_path_leads_to_the_path_history() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let out = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2","path":"lib/late.rs","debug":false}),
    )
    .await
    .expect("compare");
    let data = row_data(&out).clone();
    assert_eq!(data["files"], json!([]), "{data}");
    let lead = &data["hints"]["narrowScope"];
    assert_eq!(lead["tool"], "ghSearchHistory", "{data}");
    let row = &lead["query"]["queries"][0];
    assert_eq!(row["operation"], "commit", "{data}");
    assert_eq!(row["path"], "lib/late.rs", "{data}");
    assert_eq!(row["ref"], HEAD_SHA, "{data}");
    assert!(
        data["warnings"].to_string().contains("lib/late.rs"),
        "{data}"
    );
    // FIX §0 #3: the range's commit list is not path-scoped, so it names
    // each commit by its headline only.
    let commit = &data["commits"][0];
    assert_eq!(commit["messageHeadline"], "c0", "{data}");
    assert!(commit.get("message").is_none(), "{data}");
    runtime.close().await;
}

/// E17: a commit's patch view names the commit by its headline once; the
/// full message is one `readCommit` lead away.
#[tokio::test]
async fn commit_patch_view_carries_the_headline_not_the_whole_message() {
    let server = MockServer::builder().start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    let message = format!("Fix the parser (#42)\n\n{}", "Long rationale. ".repeat(400));
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "commit": {"message": message, "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}},
            "files": [{"filename": "src/a.rs", "status": "modified", "additions": 1, "deletions": 1,
                       "patch": "@@ -1 +1 @@\n-a\n+b"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"sections":["patches"],"debug":false}),
    )
    .await
    .expect("commit patches");
    let data = row_data(&outcome);
    assert!(data.get("message").is_none(), "{data}");
    assert_eq!(data["messageHeadline"], "Fix the parser (#42)", "{data}");
    assert!(!data.to_string().contains("Long rationale"), "{data}");
    let lead = &data["hints"]["readCommit"];
    assert_eq!(lead["tool"], "ghGetHistoryItem", "{data}");
    assert!(
        lead["query"]["queries"][0].get("sections").is_none(),
        "{data}"
    );
    runtime.close().await;
}

/// E18: an issue comment page fits one response page: the comments that
/// fit are shown, and the one comment cursor resumes exactly at the first
/// unshown comment (no second, response-level cursor).
#[tokio::test]
async fn issue_comment_pages_fit_the_response_with_one_cursor() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Big thread", "state": "open", "body": "repro",
            "user": {"login": "alice"}, "labels": [], "comments": 30,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let comments = (0..30)
        .map(|n| json!({"id": n, "user": {"login": "bob"}, "body": format!("comment {n} {}", "x".repeat(900)),
                        "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"}))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(comments)))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"queries":[{"operation":"issue","owner":"a","repo":"b","number":7,
               "sections":["comments"],"pageSize":30,"debug":false}],"responseLength":12000}),
    )
    .await
    .expect("issue comments");
    let structured = &outcome.structured_content;
    assert!(
        structured
            .get("responsePagination")
            .is_none_or(|page| page["hasMore"] != true),
        "{structured}"
    );
    let data = row_data(&outcome);
    let shown = data["issues"][0]["comments"].as_array().map_or(0, Vec::len);
    assert!((1..30).contains(&shown), "{shown}: {data}");
    let next = &data["next"]["nextCommentPage"]["query"]["queries"][0];
    let (page, size) = (
        next["commentPage"].as_u64().expect("page"),
        next["pageSize"].as_u64().expect("size"),
    );
    assert_eq!((page - 1) * size, shown as u64, "{data}");
    // Each comment's whole body restates no window.
    assert!(!data.to_string().contains("bodyPagination"), "{data}");
    runtime.close().await;
}

/// P4 (issue twin of D9): a comment-body continuation hop lists only the
/// bodies it continues, and pages exactly the comments its first window
/// showed. The first window reads the issue body beside the discussion, so
/// its comments get less of the page than a comments-only hop would: the
/// hop must still stop where the first window stopped, or it shows comments
/// first at a later offset (their opening text never read) and its
/// `nextCommentPage` skips them. Every lead of every response is followed;
/// every body (the issue's and each comment's) arrives contiguous and
/// exactly once, and no finished comment is listed again.
#[tokio::test]
async fn issue_comment_body_hops_list_only_unfinished_bodies() {
    let server = MockServer::builder().start().await;
    let issue_body = format!("issue body {}", "b".repeat(9_000));
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Long thread", "state": "open", "body": issue_body,
            "user": {"login": "alice"}, "labels": [], "comments": 40,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let body = |n: usize| {
        let size = if n % 2 == 1 { 8_000 } else { 300 };
        format!("comment {n} ") + &format!("{n:02}.").repeat(size / 3)
    };
    let comments = (0..40)
        .map(|n| {
            json!({"id": n, "user": {"login": "bob"}, "body": body(n),
                        "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(comments)))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH", "20000".into()),
    ]);
    let first = json!({"queries":[{"operation":"issue","owner":"a","repo":"b","number":7,
        "sections":["body","comments"],"length":6000,"debug":false}]});
    let mut queue = std::collections::VecDeque::from([first]);
    let mut followed = std::collections::HashSet::<String>::new();
    let mut read = std::collections::BTreeMap::<u64, String>::new();
    let mut issue_text = String::new();
    let (mut calls, mut relisted) = (0, 0);
    // One lead offered by two responses runs once, whatever its key order.
    fn canonical(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Object(map) => {
                let mut keys = map.keys().collect::<Vec<_>>();
                keys.sort();
                let fields = keys
                    .into_iter()
                    .map(|key| format!("{key:?}:{}", canonical(&map[key])))
                    .collect::<Vec<_>>();
                format!("{{{}}}", fields.join(","))
            }
            serde_json::Value::Array(items) => {
                format!(
                    "[{}]",
                    items.iter().map(canonical).collect::<Vec<_>>().join(",")
                )
            }
            scalar => scalar.to_string(),
        }
    }
    while let Some(input) = queue.pop_front() {
        if !followed.insert(canonical(&input)) {
            continue;
        }
        calls += 1;
        assert!(calls <= 60, "walk did not finish");
        let outcome = runtime
            .execute(
                format!("walk-{calls}"),
                "ghGetHistoryItem".into(),
                input.clone(),
            )
            .await
            .unwrap_or_else(|error| panic!("call {calls} failed: {error:?}\n{input}"));
        let content = &outcome.structured_content;
        let data = &content["results"][0]["data"];
        let issue = &data["issues"][0];
        if let Some(text) = issue["body"].as_str() {
            let offset = issue["contentPagination"]["body"]["offset"]
                .as_u64()
                .unwrap_or(0);
            assert_eq!(
                offset as usize,
                issue_text.chars().count(),
                "call {calls}: issue body"
            );
            issue_text.push_str(text);
        }
        for comment in issue["comments"].as_array().into_iter().flatten() {
            let id = match &comment["id"] {
                serde_json::Value::String(id) => id.parse::<u64>().expect("numeric id"),
                id => id.as_u64().expect("id"),
            };
            let text = comment["body"].as_str().unwrap_or("");
            let offset = comment["bodyPagination"]["offset"].as_u64().unwrap_or(0);
            let so_far = read.entry(id).or_default();
            if text.is_empty() && so_far.chars().count() == body(id as usize).chars().count() {
                relisted += 1;
                continue;
            }
            assert_eq!(
                offset as usize,
                so_far.chars().count(),
                "call {calls}: comment {id} {comment}"
            );
            so_far.push_str(text);
        }
        if let Some(next) = content["responsePagination"]["next"]["query"].as_object() {
            queue.push_back(serde_json::Value::Object(next.clone()));
        }
        for (_, lead) in data["next"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(name, _)| {
                matches!(
                    name.as_str(),
                    "continueBody" | "continueCommentBody" | "nextCommentPage"
                )
            })
        {
            queue.push_back(lead["query"].clone());
        }
    }
    assert_eq!(relisted, 0, "finished comments listed again");
    assert_eq!(issue_text, issue_body);
    assert_eq!(read.len(), 40, "{:?}", read.keys().collect::<Vec<_>>());
    for (id, text) in &read {
        assert_eq!(text, &body(*id as usize), "comment {id}");
    }
    runtime.close().await;
}

/// A comparison's patch window past the first carries files only: the
/// commit list rode the first window with its own page cursor.
#[tokio::test]
async fn compare_patch_windows_do_not_resend_the_commit_list() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let hop = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2",
               "pageSize":100,"sections":["patches"],"offset":5,"debug":false}),
    )
    .await
    .expect("compare hop");
    let data = row_data(&hop);
    assert!(data.get("commits").is_none(), "{data}");
    assert!(data["next"].get("nextPage").is_none(), "{data}");
    runtime.close().await;
}

/// HI6 (was D6): a path-scoped commit read keeps the whole commit's counts
/// at the top level; the scope's own count is its file page's.
#[tokio::test]
async fn path_scoped_commit_labels_whole_commit_totals() {
    let server = MockServer::builder().start().await;
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
    assert_eq!(data["changedFilesCount"], 2, "{data}");
    assert_eq!(data["additions"], 30, "{data}");
    assert!(data.get("commitTotals").is_none(), "{data}");
    assert_eq!(data["files"].as_array().map(Vec::len), Some(1), "{data}");
    runtime.close().await;
}

/// orangu: a whole-file read of a large file stays under the host output
/// cap (about 40k chars) and continues instead of returning 74–82k.
#[tokio::test]
async fn gh_full_content_first_page_stays_under_the_host_output_cap() {
    let server = MockServer::builder().start().await;
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
        json!({"owner":"a","repo":"b","path":"src/big.rs","ref":sha,"fullContent":true,"debug":false}),
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

/// An issue read lists the pull requests that closed it
/// (merged first) and offers the merged fix as `next.readPullRequest`; without
/// GraphQL the read falls back to the keyword search hop.
#[tokio::test]
async fn issue_read_lists_closing_pull_requests_and_reads_the_merged_fix() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42, "title": "model_config mutates", "state": "closed",
            "state_reason": "completed", "body": "repro", "user": {"login": "alice"},
            "labels": [], "comments": 0, "closed_at": "2026-09-25T00:29:37Z",
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:29:37Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"issue": {"closedByPullRequestsReferences": {"nodes": [
                {"number": 13794, "state": "CLOSED", "mergedAt": null},
                {"number": 13825, "state": "MERGED", "mergedAt": "2026-09-25T00:29:36Z"}
            ]}}}
        }})))
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
    .expect("issue read");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "{data}");
    let issue = &data["issues"][0];
    assert_eq!(
        issue["closedBy"],
        json!([
            {"number": 13825, "state": "merged", "mergedAt": "2026-09-25T00:29:36Z"},
            {"number": 13794, "state": "closed"}
        ]),
        "{data}"
    );
    let read = &data["hints"]["readPullRequest"];
    assert_eq!(read["tool"], "ghGetHistoryItem", "{data}");
    assert_eq!(
        read["query"]["queries"][0]["operation"], "pullRequest",
        "{data}"
    );
    assert_eq!(read["query"]["queries"][0]["number"], 13825, "{data}");
    runtime.close().await;

    // GraphQL unavailable: no closedBy, the search hop instead.
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42, "title": "t", "state": "closed", "state_reason": "completed",
            "body": "", "user": {"login": "alice"}, "labels": [], "comments": 0
        })))
        .mount(&server)
        .await;
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42}),
    )
    .await
    .expect("issue read");
    let data = row_data(&outcome);
    assert!(data["issues"][0].get("closedBy").is_none(), "{data}");
    let find = &data["hints"]["findPullRequest"];
    assert_eq!(find["tool"], "ghSearchHistory", "{data}");
    assert_eq!(
        find["query"]["queries"][0]["keywords"],
        json!(["42"]),
        "{data}"
    );
    runtime.close().await;
}

/// A bounded closing-reference set stays disclosed on every body window,
/// not only the first one that lists it; later windows ask only for the
/// count instead of the linked pull requests again.
#[tokio::test]
async fn issue_body_windows_keep_closing_reference_coverage() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42, "title": "synthetic", "state": "closed", "state_reason": "completed",
            "body": "abcdefghijklmno", "user": {"login": "audit"}, "labels": [], "comments": 0,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-10-02T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let nodes = (0..25)
        .map(|n| json!({"number": 101 + n, "state": "CLOSED", "mergedAt": null}))
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
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut query = json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42,
        "length": 5, "debug": false});
    let mut bodies = Vec::new();
    for window in 0..3 {
        let outcome = call(&runtime, "ghGetHistoryItem", query.clone())
            .await
            .expect("issue window");
        let data = row_data(&outcome);
        bodies.push(
            data["issues"][0]["body"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        );
        assert_eq!(data["isPartial"], true, "window {window}: {data}");
        assert_eq!(data["terminalLimit"], true, "window {window}: {data}");
        let reasons = data["partialReasons"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert!(
            reasons.contains(&json!("closingReferenceLimit")),
            "window {window}: {data}"
        );
        assert_eq!(
            reasons.contains(&json!("contentPagination")),
            window < 2,
            "window {window}: {data}"
        );
        let warning = data["warnings"][0].as_str().unwrap_or_default();
        assert!(warning.contains("25 of 31"), "window {window}: {data}");
        if window < 2 {
            // `partialReasons` say why the row is partial; the page line
            // still names the continuation that reaches the remaining body.
            assert!(
                data["next"]["continueBody"].is_object(),
                "window {window}: {data}"
            );
            assert!(
                data["warnings"]
                    .to_string()
                    .contains("follow next.continueBody"),
                "window {window}: {data}"
            );
        }
        if window == 0 {
            assert_eq!(
                data["issues"][0]["closedBy"].as_array().map(Vec::len),
                Some(25),
                "{data}"
            );
        } else {
            assert!(data["issues"][0].get("closedBy").is_none(), "{data}");
            assert!(data["hints"].get("readPullRequest").is_none(), "{data}");
        }
        match data["next"]["continueBody"]["query"].as_object() {
            Some(next) => query = serde_json::Value::Object(next.clone()),
            None => assert_eq!(window, 2, "{data}"),
        }
    }
    assert_eq!(bodies, ["abcde", "fghij", "klmno"]);
    let documents = server
        .received_requests()
        .await
        .expect("recorded")
        .into_iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect::<Vec<_>>();
    assert_eq!(documents.len(), 3, "{documents:?}");
    assert!(documents[0].contains("nodes"), "{documents:?}");
    assert!(
        documents[1..]
            .iter()
            .all(|document| !document.contains("nodes")),
        "{documents:?}"
    );
    runtime.close().await;
}

/// Through the whole response stage, a code search pinned to a non-default
/// ref warns with the index commit and keeps its ghStructure lead to the ref
/// first among the leads.
#[tokio::test]
async fn pinned_ref_code_search_warns_and_leads_to_the_ref_listing() {
    let server = MockServer::builder().start().await;
    let at_ref = "0123456789abcdef0123456789abcdef01234567";
    let head = "fedcba9876543210fedcba9876543210fedcba98";
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":1,"incomplete_results":false,"items":[
                {"name":"base.py","path":"pkg/base.py","sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"},
                 "text_matches":[{"fragment":"class Base:","matches":[{"text":"class","indices":[0,5]}]}]}
            ]
        })))
        .mount(&server)
        .await;
    for (reference, sha) in [("dev", at_ref), ("HEAD", head)] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/commits/{reference}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(sha))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/pkg%2Fbase.py"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("class Base:\n    pass\n")
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let out = call(
        &runtime,
        "ghSearchCode",
        json!({"owner":"a","repo":"b","keywords":["class"],"path":"pkg","ref":"dev"}),
    )
    .await
    .expect("search");
    let data = row_data(&out);
    let warnings = data["warnings"].to_string();
    assert!(
        warnings.contains("default-branch index at fedcba9, not dev")
            && warnings.contains("hints.viewRepo"),
        "{data}"
    );
    let lead = &data["hints"]["viewRepo"];
    assert_eq!(lead["tool"], "ghStructure", "{data}");
    assert_eq!(lead["query"]["queries"][0]["ref"], at_ref, "{data}");
    assert_eq!(lead["query"]["queries"][0]["path"], "pkg", "{data}");
    assert_eq!(
        data["hints"]
            .as_object()
            .and_then(|hints| hints.keys().next())
            .map(String::as_str),
        Some("viewRepo"),
        "{data}"
    );
    runtime.close().await;
}

// ── H lane: numbered sides, side reads, compare scope, comment pages ───────

/// Patch gutters number each line on the side of its sign (` `/`+`: the
/// commit, `-`: its first parent); `readAtCommit` reads the top file whole
/// at the commit and `readParent` reads its old-side hunk windows at the
/// parent, so a gutter number is a direct `ranges` entry on its side.
#[tokio::test]
async fn commit_patch_view_numbers_both_sides_and_reads_each_side() {
    let server = MockServer::builder().start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    let parent = "9999999def456abc123def456abc123def456abc";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha, "parents": [{"sha": parent}],
            "commit": {"message": "Fix f", "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}},
            "files": [{"filename": "src/a.rs", "status": "modified", "additions": 2, "deletions": 1,
                       "patch": "@@ -40,3 +60,4 @@ fn f\n a\n-b\n+c\n+d\n e"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"sections":["patches"],"debug":false}),
    )
    .await
    .expect("commit patches");
    let data = row_data(&outcome);
    assert_eq!(
        data["files"][0]["patch"], "@@ -40,3 +60,4 @@ fn f\n60\t a\n41\t-b\n61\t+c\n62\t+d\n63\t e",
        "{data}"
    );
    assert_eq!(
        data["hints"]["readAtCommit"]["query"],
        json!({"queries":[{"owner":"a","repo":"b","path":"src/a.rs","ref":sha}]}),
        "{data}"
    );
    assert_eq!(
        data["hints"]["readParent"]["query"],
        json!({"queries":[{"owner":"a","repo":"b","path":"src/a.rs","ref":parent,"ranges":["30-52"]}]}),
        "{data}"
    );
    runtime.close().await;
}

/// A bogus commit SHA (GitHub 422 "No commit found") is a missing commit:
/// `notFound`, not an invalid query.
#[tokio::test]
async fn bad_commit_sha_is_not_found() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/deadbeef"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: deadbeef"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b","ref":"deadbeef","debug":false}),
    )
    .await
    .expect("commit row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    assert!(
        data["error"]
            .as_str()
            .is_some_and(|e| e.contains("Commit not found")),
        "{data}"
    );
    runtime.close().await;
}

/// `include` scopes a comparison's files like a pull request's; past
/// GitHub's 300-file compare list a scoped path is disclosed as possibly
/// missing, with the path's commit history as the lead.
#[tokio::test]
async fn compare_include_scopes_files_and_capped_scope_leads_to_path_history() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let listed = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2",
               "include":["src/f001.rs","src/f2*.rs"],"debug":false}),
    )
    .await
    .expect("compare include");
    let data = row_data(&listed).clone();
    assert_eq!(row_status(&listed), "success", "{data}");
    let files = data["files"].to_string();
    assert!(
        files.contains("f001.rs") && files.contains("f200.rs"),
        "{data}"
    );
    assert!(!files.contains("f002.rs"), "{data}");
    // HI6: the top-level count is the comparison's; the scope's is its page's.
    assert_eq!(data["changedFilesCount"], 300, "{data}");
    assert_eq!(data["filePagination"]["totalItems"], 101, "{data}");
    let beyond = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":"v2",
               "include":["lib/**"],"debug":false}),
    )
    .await
    .expect("compare include beyond cap");
    let data = row_data(&beyond).clone();
    assert!(
        data["warnings"].to_string().contains("300 changed files"),
        "{data}"
    );
    let row = &data["hints"]["narrowScope"]["query"]["queries"][0];
    assert_eq!(
        data["hints"]["narrowScope"]["tool"], "ghSearchHistory",
        "{data}"
    );
    assert_eq!(row["path"], "lib/", "{data}");
    assert_eq!(row["ref"], HEAD_SHA, "{data}");
    runtime.close().await;
}

/// A comparison's patch view reads its top file at the head and at the
/// merge base (the old side of a three-dot diff).
#[tokio::test]
async fn compare_patch_view_reads_head_and_merge_base() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex(format!("^/api/v3/repos/a/b/compare/{BASE_SHA}\\.\\.\\.{HEAD_SHA}$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"ahead","ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE_SHA},"merge_base_commit":{"sha":"3333333ccccccccccccccccccccccccccccccccc"},
            "commits":[{"sha":HEAD_SHA,"commit":{"message":"m","author":{"name":"a","date":"2024-01-01T00:00:00Z"}}}],
            "files":[{"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,
                      "patch":"@@ -5,1 +7,1 @@\n-x\n+y"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":BASE_SHA,"head":HEAD_SHA,
               "sections":["patches"],"debug":false}),
    )
    .await
    .expect("compare patches");
    let data = row_data(&outcome);
    assert_eq!(
        data["files"][0]["patch"], "@@ -5,1 +7,1 @@\n5\t-x\n7\t+y",
        "{data}"
    );
    assert_eq!(
        data["hints"]["readAtCommit"]["query"]["queries"][0]["ref"], HEAD_SHA,
        "{data}"
    );
    let parent = &data["hints"]["readParent"]["query"]["queries"][0];
    assert_eq!(
        parent["ref"], "3333333ccccccccccccccccccccccccccccccccc",
        "{data}"
    );
    assert_eq!(parent["ranges"], json!(["1-15"]), "{data}");
    runtime.close().await;
}

/// GitHub issue comments paged by `per_page`/`page` from one list.
struct CommentList(Vec<serde_json::Value>, String);

impl Respond for CommentList {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let param = |name: &str| {
            request
                .url
                .query_pairs()
                .find(|(key, _)| key == name)
                .and_then(|(_, value)| value.parse::<usize>().ok())
        };
        let per = param("per_page").unwrap_or(30);
        let page = param("page").unwrap_or(1);
        let start = (page - 1) * per;
        let items = self
            .0
            .iter()
            .skip(start)
            .take(per)
            .cloned()
            .collect::<Vec<_>>();
        let mut response = ResponseTemplate::new(200).set_body_json(json!(items));
        if start + per < self.0.len() {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/issues/7/comments?per_page={per}&page={}>; rel=\"next\"",
                    self.1,
                    page + 1
                )
                .as_str(),
            );
        }
        response
    }
}

/// A 105-comment issue thread (two bot comments) walked by its own
/// cursor: every human comment arrives once, hidden bots are counted with
/// an `includeBots` lead, each page's remainder count is exact, and pages
/// fill the response budget (few calls).
#[tokio::test]
async fn issue_comment_walk_discloses_bots_and_counts_the_rest() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Big thread", "state": "open", "body": "repro",
            "user": {"login": "alice"}, "labels": [], "comments": 105,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let comments = (0..105)
        .map(|n| {
            let login = if n == 40 || n == 90 { "github-actions[bot]" } else { "bob" };
            json!({"id": n, "user": {"login": login}, "body": format!("comment {n} {}", "x".repeat(700)),
                   "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .respond_with(CommentList(comments, server.uri()))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH", "20000".into()),
    ]);
    let mut query = json!({"operation":"issue","owner":"a","repo":"b","number":7,"sections":["comments"],"debug":false});
    let mut ids = Vec::new();
    let (mut bots, mut calls, mut bot_lead) = (0, 0, false);
    loop {
        calls += 1;
        assert!(calls <= 20, "walk did not finish");
        let outcome = call(
            &runtime,
            "ghGetHistoryItem",
            json!({"queries":[query.clone()]}),
        )
        .await
        .expect("comment page");
        let structured = &outcome.structured_content;
        assert!(
            structured.get("responsePagination").is_none(),
            "{structured}"
        );
        let data = row_data(&outcome).clone();
        let issue = &data["issues"][0];
        assert!(issue["comments"].is_array(), "call {calls}: {data}");
        for comment in issue["comments"].as_array().into_iter().flatten() {
            assert!(comment.get("commentType").is_none(), "{comment}");
            assert!(comment.get("updatedAt").is_none(), "{comment}");
            ids.push(comment["id"].as_str().expect("id").to_owned());
        }
        let page = &issue["contentPagination"]["comments"];
        bots += page["botsHidden"].as_u64().unwrap_or(0);
        bot_lead |= data["hints"]["includeBots"]["query"]["queries"][0]["includeBots"] == true;
        let consumed = ids.len() as u64 + bots;
        match data["next"]["nextCommentPage"]["query"]["queries"][0].as_object() {
            Some(next) => {
                let left = 105 - consumed;
                assert!(
                    data["warnings"]
                        .to_string()
                        .contains(&format!("{left} more comments")),
                    "consumed {consumed}: {data}"
                );
                query = serde_json::Value::Object(next.clone());
            }
            None => break,
        }
    }
    let unique = ids.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!((ids.len(), unique.len(), bots), (103, 103, 2));
    assert!(bot_lead, "no includeBots lead");
    // ~73 KB of comments at a 20k page: budget-filled pages.
    assert!(calls <= 5, "{calls} calls");
    runtime.close().await;
}

/// A PR summary previews its body (template comments minified away) and
/// leads to the whole body.
#[tokio::test]
async fn pr_summary_previews_the_body() {
    let server = MockServer::builder().start().await;
    let body = format!(
        "<!-- template: describe the change -->\nFixes the cache race.\n\n{}",
        "Details. ".repeat(100)
    );
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Fix cache", "state": "open", "draft": false, "body": body,
            "user": {"login": "bob"}, "head": {"sha": "def456", "ref": "feat"}, "base": {"ref": "main"},
            "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z",
            "changed_files": 2, "additions": 3, "deletions": 1
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
    .expect("PR summary");
    let data = row_data(&outcome);
    let preview = data["pullRequests"][0]["bodyPreview"]
        .as_str()
        .expect("preview");
    assert!(preview.starts_with("Fixes the cache race."), "{preview}");
    assert!(
        preview.ends_with('…') && preview.chars().count() <= 301,
        "{preview}"
    );
    let leads = data["hints"].to_string();
    assert!(leads.contains("\"body\""), "no whole-body read: {data}");
    runtime.close().await;
}

/// X13: a ref that does not resolve is `notFound` (not `invalidInput`) on
/// both GitHub read tools, with one runnable lead to the repository's refs.
#[tokio::test]
async fn gh_bad_ref_is_not_found_with_a_refs_lead() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/no-such-ref"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: no-such-ref"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (tool, query) in [
        (
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"README.md","ref":"no-such-ref","forceRefresh":true}),
        ),
        (
            "ghStructure",
            json!({"owner":"a","repo":"b","ref":"no-such-ref"}),
        ),
    ] {
        let outcome = call(&runtime, tool, query).await.expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{tool}: {data}");
        assert!(
            data["error"]
                .as_str()
                .unwrap_or_default()
                .contains("\"no-such-ref\""),
            "{tool}: {data}"
        );
        let lead = &data["hints"]["viewRefs"];
        assert_eq!(lead["tool"], "ghStructure", "{tool}: {data}");
        let row = &lead["query"]["queries"][0];
        for (field, value) in [("owner", "a"), ("repo", "b"), ("operation", "refs")] {
            assert_eq!(row[field], value, "{tool}: {data}");
        }
    }
    runtime.close().await;
}

/// QA2: a full 40-hex SHA that GitHub does not have skips ref resolution, so
/// the listing's tree read is a bare 404. ghStructure must still name the
/// ref and lead to the repository's refs, as ghGetFileContent does, instead
/// of the generic "Repository, resource, or path not found".
#[tokio::test]
async fn gh_missing_full_sha_is_named_with_a_refs_lead() {
    const MISSING: &str = "2bd066d87f5bafd315be9f40889d0a60b9e58e0b";
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{MISSING}")))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":format!("No commit found for SHA: {MISSING}")})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/contents"))
        .respond_with(
            ResponseTemplate::new(404)
                .set_body_json(json!({"message":format!("No commit found for the ref {MISSING}")})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (tool, query) in [
        (
            "ghGetFileContent",
            json!({"owner":"a","repo":"b","path":"README.md","ref":MISSING,"forceRefresh":true}),
        ),
        ("ghStructure", json!({"owner":"a","repo":"b","ref":MISSING})),
        (
            "ghStructure",
            json!({"owner":"a","repo":"b","ref":MISSING,"path":"src"}),
        ),
    ] {
        let outcome = call(&runtime, tool, query).await.expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{tool}: {data}");
        assert!(
            data["error"].as_str().unwrap_or_default().contains(MISSING),
            "{tool}: {data}"
        );
        assert_eq!(
            data["hints"]["viewRefs"]["tool"], "ghStructure",
            "{tool}: {data}"
        );
    }
    runtime.close().await;
}

/// QA2: GitHub answers a missing pull request, issue, or comparison ref
/// with a bare 404. The error row must name what was asked for (the number
/// or the compared refs), not the generic "Repository, resource, or path not
/// found"; a comparison also leads to the repository's refs.
#[tokio::test]
async fn gh_history_item_not_found_names_the_item() {
    let server = MockServer::builder().start().await;
    // An existing repository: `main` resolves, `no-such-head` does not (422),
    // and the comparison of an unknown base is GitHub's bare 404.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HISTORY_A_HEAD))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/no-such-head"))
        .respond_with(
            ResponseTemplate::new(422)
                .set_body_json(json!({"message":"No commit found for SHA: no-such-head"})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (query, named) in [
        (
            json!({"operation":"pullRequest","owner":"a","repo":"b","number":987654}),
            "#987654",
        ),
        (
            json!({"operation":"issue","owner":"a","repo":"b","number":987654}),
            "#987654",
        ),
        (
            json!({"operation":"compare","owner":"a","repo":"b","base":"no-such-base","head":"main"}),
            "no-such-base...",
        ),
        (
            json!({"operation":"compare","owner":"a","repo":"b","base":"main","head":"no-such-head"}),
            "no-such-head",
        ),
    ] {
        let outcome = call(&runtime, "ghGetHistoryItem", query.clone())
            .await
            .expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{query}: {data}");
        let error = data["error"].as_str().unwrap_or_default();
        assert!(error.contains(named), "{query}: {data}");
        if query["operation"] != "compare" || query["head"] == "main" {
            assert!(error.contains("a/b"), "{query}: {data}");
        }
        if query["operation"] == "compare" {
            let lead = &data["hints"]["viewRefs"];
            assert_eq!(lead["tool"], "ghStructure", "{data}");
            assert_eq!(lead["query"]["queries"][0]["operation"], "refs", "{data}");
        }
    }
    runtime.close().await;
}

/// QA2: a commit listing GitHub answers with a bare 404 (missing repository
/// or ref) names the repository and ref it listed.
#[tokio::test]
async fn gh_search_history_commit_not_found_names_the_scope() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (query, named) in [
        (
            json!({"operation":"commit","owner":"a","repo":"b","ref":"no-such-ref"}),
            "\"no-such-ref\"",
        ),
        (json!({"operation":"commit","owner":"a","repo":"b"}), "a/b"),
    ] {
        let outcome = call(&runtime, "ghSearchHistory", query.clone())
            .await
            .expect("error row");
        let data = row_data(&outcome);
        assert_eq!(data["errorCode"], "notFound", "{query}: {data}");
        let error = data["error"].as_str().unwrap_or_default();
        assert!(
            error.contains(named) && error.contains("a/b"),
            "{query}: {data}"
        );
    }
    runtime.close().await;
}

/// QA2: an owner-only ghSearchRepo listing of a login GitHub does not know
/// (bare 404) names the owner instead of "Repository, resource, or path".
#[tokio::test]
async fn gh_search_repo_missing_owner_is_named() {
    let server = MockServer::builder().start().await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(&runtime, "ghSearchRepo", json!({"owner":"ghost-login"}))
        .await
        .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "notFound", "{data}");
    assert!(
        data["error"]
            .as_str()
            .unwrap_or_default()
            .contains("\"ghost-login\""),
        "{data}"
    );
    runtime.close().await;
}

// ── history lane A: PR/commit/compare/issue views ───────────────────────────

const HISTORY_A_HEAD: &str = "0123456789abcdef0123456789abcdef01234567";
const HISTORY_A_BASE: &str = "89abcdef0123456789abcdef0123456789abcdef";
const HISTORY_A_MERGE: &str = "fedcba9876543210fedcba9876543210fedcba98";

fn history_a_rest_pr(merged: bool) -> serde_json::Value {
    json!({
        "number": 9, "title": "Fix parser", "state": "closed",
        "merged_at": merged.then_some("2024-01-03T00:00:00Z"),
        "merge_commit_sha": HISTORY_A_MERGE, "draft": false, "body": "Fixes it.",
        "user": {"login": "alice"}, "labels": [],
        "head": {"sha": HISTORY_A_HEAD, "ref": "feat"},
        "base": {"sha": HISTORY_A_BASE, "ref": "main"},
        "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z",
        "closed_at": "2024-01-03T00:00:00Z", "comments": 2, "review_comments": 1,
        "commits": 3, "changed_files": 2, "additions": 70000, "deletions": 1
    })
}

fn history_a_file(name: &str, patch: Option<&str>, additions: u64) -> serde_json::Value {
    let mut file = json!({
        "sha": "1111111111111111111111111111111111111111", "filename": name,
        "status": "modified", "additions": additions, "deletions": 1,
        "changes": additions + 1
    });
    if let Some(patch) = patch {
        file["patch"] = json!(patch);
    }
    file
}

async fn history_a_mount_pr(server: &MockServer, merged: bool, files: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(history_a_rest_pr(merged)))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(files))
        .mount(server)
        .await;
}

/// HI4: every PR view names the head, the recorded target-branch tip and
/// (merged) the merge commit: patch, file-list, and matchString views.
#[tokio::test]
async fn history_a_every_pr_view_carries_source_target_and_merge_commits() {
    let server = MockServer::builder().start().await;
    history_a_mount_pr(
        &server,
        true,
        json!([
            history_a_file(
                "src/parse.rs",
                Some("@@ -1,2 +1,2 @@\n-let a = 1;\n+let parsed = parse(input);\n ctx"),
                1
            ),
            history_a_file("src/huge.rs", None, 69_999),
        ]),
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for extra in [
        json!({"sections":["patches"]}),
        json!({"sections":["files"]}),
        json!({"sections":["patches"],"matchString":"parsed"}),
    ] {
        let mut query =
            json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,"debug":false});
        for (key, value) in extra.as_object().expect("fields") {
            query[key] = value.clone();
        }
        let outcome = call(&runtime, "ghGetHistoryItem", query)
            .await
            .expect("read");
        let data = row_data(&outcome);
        assert_eq!(row_status(&outcome), "success", "{extra}: {data}");
        let pr = &data["pullRequests"][0];
        assert_eq!(pr["sourceSha"], HISTORY_A_HEAD, "{extra}: {data}");
        assert_eq!(pr["targetSha"], HISTORY_A_BASE, "{extra}: {data}");
        assert_eq!(pr["mergeCommitSha"], HISTORY_A_MERGE, "{extra}: {data}");
        assert!(
            !outcome
                .structured_content
                .to_string()
                .contains("outputContractViolation"),
            "{extra}: {}",
            outcome.structured_content
        );
    }
    runtime.close().await;
}

/// HI8: a patch read whose page holds a file GitHub sent without a patch
/// reads that file at both sides (its change is in no response), even on a
/// merged PR whose merge read stands for the patched file's new side.
#[tokio::test]
async fn history_a_unpatched_file_gets_head_and_parent_reads() {
    let server = MockServer::builder().start().await;
    history_a_mount_pr(
        &server,
        true,
        json!([
            history_a_file(
                "src/parse.rs",
                Some("@@ -1,2 +1,2 @@\n-let a = 1;\n+let parsed = parse(input);\n ctx"),
                1
            ),
            history_a_file("src/huge.rs", None, 69_999),
        ]),
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,
            "sections":["patches"],"debug":false}),
    )
    .await
    .expect("patch read");
    let data = row_data(&outcome);
    // The response's lead cap keeps the head read (and the merge read, by
    // priority); `readParent` is built the same way (unit-tested).
    let head = &data["hints"]["readAtCommit"]["query"]["queries"][0];
    assert_eq!(head["path"], "src/huge.rs", "{data}");
    assert_eq!(head["ref"], HISTORY_A_HEAD, "{data}");
    // HI11: the merge read locates the patched file's hunks by text.
    let merge = &data["hints"]["readAtMerge"]["query"]["queries"][0];
    assert_eq!(merge["ref"], HISTORY_A_MERGE, "{data}");
    assert_eq!(
        merge["matchString"],
        json!(["let parsed = parse(input);"]),
        "{data}"
    );
    assert!(merge.get("ranges").is_none(), "{data}");
    runtime.close().await;
}

/// HI4/HI7: the GraphQL read carries `targetSha` from `baseRefOid` on every
/// view, and a metadata view the commit and review-thread totals.
#[tokio::test]
async fn history_a_graphql_pr_reads_target_sha_and_totals() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"pullRequest": {
                "number": 7, "title": "Fix parser", "url": "https://x", "state": "MERGED",
                "body": "Fixes the parser.", "isDraft": false, "author": {"login": "bob"},
                "labels": {"pageInfo": {"hasNextPage": false}, "nodes": []},
                "baseRefName": "main", "baseRefOid": HISTORY_A_BASE,
                "headRefName": "feat/parser", "headRefOid": HISTORY_A_HEAD,
                "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z",
                "closedAt": "2026-01-03T00:00:00Z", "mergedAt": "2026-01-03T00:00:00Z",
                "mergeCommit": {"oid": HISTORY_A_MERGE},
                "comments": {"totalCount": 0}, "changedFiles": 1, "additions": 1, "deletions": 0,
                "commitsCount": {"totalCount": 4}, "reviewThreads": {"totalCount": 2},
                "files": {"pageInfo": {"hasNextPage": false}, "nodes": [
                    {"path": "src/a.rs", "additions": 1, "deletions": 0, "changeType": "MODIFIED"}
                ]},
                "reviews": {"pageInfo": {"hasNextPage": false}, "nodes": [
                    {"databaseId": 11, "author": {"login": "ann"}, "state": "APPROVED",
                     "body": "", "submittedAt": "2026-01-03T00:00:00Z", "commit": {"oid": HISTORY_A_HEAD}}
                ]}
            }}
        }})))
        .expect(2)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    for (sections, totals) in [
        (json!(["body", "reviews"]), true),
        (json!(["body", "files"]), false),
    ] {
        let outcome = call(
            &runtime,
            "ghGetHistoryItem",
            json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,
                "sections":sections,"debug":false}),
        )
        .await
        .expect("GraphQL read");
        let data = row_data(&outcome);
        assert_eq!(row_status(&outcome), "success", "{data}");
        assert!(data.get("graphqlFallback").is_none(), "{data}");
        let pr = &data["pullRequests"][0];
        assert_eq!(pr["targetSha"], HISTORY_A_BASE, "{sections}: {data}");
        assert_eq!(pr["mergeCommitSha"], HISTORY_A_MERGE, "{sections}: {data}");
        if totals {
            assert_eq!(pr["commitsCount"], 4, "{data}");
            assert_eq!(pr["reviewThreadsCount"], 2, "{data}");
        }
    }
    let documents = server
        .received_requests()
        .await
        .expect("recorded")
        .into_iter()
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect::<Vec<_>>();
    assert!(documents[0].contains("baseRefOid"), "{documents:?}");
    runtime.close().await;
}

/// HI7: an issue summary counts its comments and offers the discussion
/// read; an issue without comments offers neither.
#[tokio::test]
async fn history_a_issue_summary_counts_comments_and_offers_the_discussion() {
    for (comments, offered) in [(105, true), (0, false)] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/issues/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "number": 42, "title": "Parser OOM", "state": "open", "body": "repro",
                "user": {"login": "alice"}, "labels": [], "comments": comments,
                "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
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
        .expect("issue read");
        let data = row_data(&outcome);
        assert_eq!(row_status(&outcome), "success", "{data}");
        let issue = &data["issues"][0];
        let read = &data["hints"]["readDiscussion"];
        if offered {
            assert_eq!(issue["commentsCount"], 105, "{data}");
            assert_eq!(read["tool"], "ghGetHistoryItem", "{data}");
            let q = &read["query"]["queries"][0];
            assert_eq!(q["sections"], json!(["comments"]), "{data}");
            assert_eq!(q["number"], 42, "{data}");
            octocode_native::contracts::validate_query("ghGetHistoryItem", q.clone())
                .expect("the discussion read validates");
        } else {
            assert!(issue.get("commentsCount").is_none(), "{data}");
            assert!(read.is_null(), "{data}");
        }
        runtime.close().await;
    }
}

/// HI6 + N5: an `include` that matches none of a commit's files keeps the
/// whole commit's counts and names the changed directories nearest it.
#[tokio::test]
async fn history_a_commit_include_matching_nothing_warns_with_nearest_dirs() {
    let server = MockServer::builder().start().await;
    let sha = "abc123def456abc123def456abc123def456abc1";
    let files = [
        "docs/a.md",
        "packages/react-dom/src/a.js",
        "packages/react/src/b.js",
        "packages/react/index.js",
        "scripts/x.js",
    ]
    .iter()
    .map(|name| json!({"filename": name, "status": "modified", "additions": 1, "deletions": 0}))
    .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/commits/{sha}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "commit": {"message": "five files", "author": {"name": "A", "date": "2024-01-01T00:00:00Z"}},
            "stats": {"additions": 5, "deletions": 0, "total": 5},
            "files": files
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let read = |include: &str| json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"include":[include],"debug":false});
    let outcome = call(&runtime, "ghGetHistoryItem", read("packages/react/**"))
        .await
        .expect("commit");
    let data = row_data(&outcome);
    assert_eq!(data["changedFilesCount"], 5, "{data}");
    assert_eq!(data["files"].as_array().map(Vec::len), Some(2), "{data}");
    assert!(data.get("commitTotals").is_none(), "{data}");
    assert!(
        !data["warnings"].to_string().contains("matched 0"),
        "{data}"
    );
    let outcome = call(&runtime, "ghGetHistoryItem", read("packages/reakt/**"))
        .await
        .expect("commit");
    let data = row_data(&outcome);
    assert_eq!(data["changedFilesCount"], 5, "{data}");
    let warnings = data["warnings"].to_string();
    assert!(
        warnings.contains("include matched 0 of 5 changed files; changed directories: packages/react-dom/src/, packages/react/src/, packages/react/, docs/, scripts/."),
        "{data}"
    );
    runtime.close().await;
}

/// HI12(a) + X5: a comparison lists commit headlines (login first) and
/// one `readCommit` lead for the first commit with more message.
#[tokio::test]
async fn history_a_compare_commits_are_headlines_with_login_authors() {
    let server = MockServer::builder().start().await;
    let head = "1111111111111111111111111111111111111111";
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/compare/v1...{head}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status": "ahead", "ahead_by": 3, "behind_by": 0, "total_commits": 3,
            "commits": [
                {"sha": "c1", "author": {"login": "rickhanlonii"},
                 "commit": {"message": "Fix A (#1)", "author": {"name": "Ricky", "date": "2024-01-01T00:00:00Z"}}},
                {"sha": "c2", "author": null,
                 "commit": {"message": "Fix B (#2)\n\nLong body explaining B.", "author": {"name": "Unlinked", "date": "2024-01-02T00:00:00Z"}}},
                {"sha": "c3", "author": {"login": "acdlite"},
                 "commit": {"message": "Fix C\n\nmore", "author": {"name": "Andrew", "date": "2024-01-03T00:00:00Z"}}}
            ],
            "files": [{"filename": "src/a.js", "status": "modified", "additions": 1, "deletions": 0}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","base":"v1","head":head,"debug":false}),
    )
    .await
    .expect("compare");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "{data}");
    let commits = data["commits"].as_array().expect("commits");
    assert_eq!(
        commits
            .iter()
            .map(|c| (c["messageHeadline"].clone(), c["author"].clone()))
            .collect::<Vec<_>>(),
        [
            (json!("Fix A (#1)"), json!("rickhanlonii")),
            (json!("Fix B (#2)"), json!("Unlinked")),
            (json!("Fix C"), json!("acdlite")),
        ],
        "{data}"
    );
    assert!(commits.iter().all(|c| c.get("message").is_none()), "{data}");
    let read = &data["hints"]["readCommit"]["query"]["queries"][0];
    assert_eq!(read["operation"], "commit", "{data}");
    assert_eq!(read["ref"], "c2", "{data}");
    runtime.close().await;
}

/// X9: a read of a renamed repository (GitHub answers 301 to
/// `/repositories/<id>`) warns once and names the canonical repository; a
/// later process reading from the immutable cache (no request, so no
/// redirect) still knows the rename from the memo.
#[tokio::test]
async fn gh_file_read_of_a_renamed_repository_names_the_canonical_repository() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    for (from, to) in [
        (
            "/api/v3/repos/a/b/commits/main",
            "/api/v3/repositories/1/commits/main",
        ),
        (
            "/api/v3/repos/a/b/contents/README.md",
            "/api/v3/repositories/1/contents/README.md",
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(from))
            .respond_with(
                ResponseTemplate::new(301)
                    .insert_header("location", format!("{}{to}?ref={sha}", server.uri())),
            )
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/commits/main"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repositories/1/contents/README.md"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("hello\n"),
            "size":6,"sha":"f".repeat(40),"path":"README.md"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch":"main","full_name":"c/d"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/c/d/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let renamed = |data: &serde_json::Value| {
        let warnings = data["warnings"].to_string();
        assert_eq!(
            warnings.matches("renamed to c/d").count(),
            1,
            "one rename warning: {data}"
        );
        assert_eq!(data["owner"], "c", "{data}");
        assert_eq!(data["repo"], "d", "{data}");
    };
    let first = workspace.runtime(&settings);
    let outcome = call(
        &first,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"README.md","ref":"main"}),
    )
    .await
    .expect("read");
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    renamed(row_data(&outcome));
    first.close().await;

    let second = workspace.runtime(&settings);
    let outcome = call(
        &second,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"README.md","ref":sha}),
    )
    .await
    .expect("cached read");
    renamed(row_data(&outcome));
    second.close().await;
}

/// X9: a repository GitHub never redirected costs no metadata request.
#[tokio::test]
async fn gh_file_read_of_a_standing_repository_reads_no_metadata() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/README.md"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("hello\n"),
            "size":6,"sha":"f".repeat(40),"path":"README.md"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"default_branch":"main"})))
        .expect(0)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"a","repo":"b","path":"README.md","ref":sha}),
    )
    .await
    .expect("read");
    let data = row_data(&outcome);
    assert!(!data.to_string().contains("renamed"), "{data}");
    runtime.close().await;
}
