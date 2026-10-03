// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Per-tool cache-contract integration tests.
//!
//! Every cacheable tool must have at least one test that proves a cache hit
//! occurs (or does not occur) under the expected conditions. Every
//! non-cacheable tool must have a test that confirms the `cache` row flag is
//! never set.
//!
//! # Layout
//!
//! | Section | Tool | Contract |
//! |---|---|---|
//! | ghGetFileContent | ghGetFileContent | hit sets `cache:1`; forceRefresh bypasses |
//! | ghStructure | ghStructure | tree traversal hit saves one HTTP round-trip |
//! | ghSearchHistory | ghSearchHistory | never sets `cache:1` |
//! | ghGetHistoryItem | ghGetHistoryItem | never sets `cache:1` |
//! | artifactSearch | artifactSearch | memory-mode disables in-process cache |
//! | local tools | localSearch/localFetch | never sets `cache:1` |

mod support;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

// ─── helpers ────────────────────────────────────────────────────────────────

/// Returns the `cache` field value from the first result row, or 0 if absent.
fn row_cache_flag(outcome: &octocode_native::runtime::ToolOutcome) -> u64 {
    outcome
        .structured_content
        .pointer("/results/0/cache")
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

/// A 40-char hex string used as a branch SHA that skips commit resolution.
fn sha(ch: char) -> String {
    ch.to_string().repeat(40)
}

// ─── ghGetFileContent ────────────────────────────────────────────────────────

/// A second identical `ghGetFileContent` call must be served from the
/// `GitHubContentCache` and report `cache:1` in the result row.
///
/// Flow:
///  1. First call: 200 + ETag "v1"  →  stored in cache, `cache` flag absent.
///  2. Second call: If-None-Match sent, server replies 304  →  served from
///     cache, `cache:1` set in the result row.
#[tokio::test]
async fn ghgetfilecontent_second_call_sets_cache_flag() {
    let server = MockServer::start().await;
    let sha = sha('a');
    // Serve the file; respond with 304 when a conditional request arrives.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/contents/file.rs"))
        .respond_with(|req: &Request| {
            if req.headers.get("if-none-match").is_some() {
                ResponseTemplate::new(304)
            } else {
                ResponseTemplate::new(200)
                    .insert_header("etag", "\"etag-v1\"")
                    .set_body_json(json!({
                        "type": "file",
                        "encoding": "base64",
                        "content": STANDARD.encode("hello cache\n"),
                    }))
            }
        })
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let query = json!({"owner":"o","repo":"r","path":"file.rs","branch":sha});

    let runtime = workspace.runtime(&settings);

    // First call: cache miss — result must NOT carry cache:1.
    let first = call(&runtime, "ghGetFileContent", query.clone())
        .await
        .unwrap();
    assert_eq!(row_status(&first), "success");
    assert_eq!(
        row_cache_flag(&first),
        0,
        "first call must not be a cache hit"
    );

    // Second call: cache hit — result MUST carry cache:1.
    let second = call(&runtime, "ghGetFileContent", query).await.unwrap();
    assert_eq!(row_status(&second), "success");
    assert_eq!(
        row_cache_flag(&second),
        1,
        "second call must be served from cache (cache:1)"
    );
    assert_eq!(row_data(&second)["files"][0]["content"], "1\thello cache\n");
    runtime.close().await;
}

/// `forceRefresh: true` must bypass the populated cache and issue a fresh HTTP
/// request, leaving `cache:1` absent from the result row.
#[tokio::test]
async fn ghgetfilecontent_force_refresh_bypasses_populated_cache() {
    let server = MockServer::start().await;
    let sha = sha('b');
    // The mock always returns 200 — it must NOT return 304 even on the second
    // call because forceRefresh skips the cached ETag.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/contents/fresh.rs"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"etag-fresh\"")
                .set_body_json(json!({
                    "type": "file",
                    "encoding": "base64",
                    "content": STANDARD.encode("fresh\n"),
                })),
        )
        .expect(2) // both calls must reach HTTP; no 304 shortcut
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let runtime = workspace.runtime(&settings);

    // First call: populates cache.
    let first = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"o","repo":"r","path":"fresh.rs","branch":sha}),
    )
    .await
    .unwrap();
    assert_eq!(row_status(&first), "success");

    // Second call with forceRefresh: must not use cache.
    let second = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"o","repo":"r","path":"fresh.rs","branch":sha,"forceRefresh":true}),
    )
    .await
    .unwrap();
    assert_eq!(row_status(&second), "success");
    assert_eq!(
        row_cache_flag(&second),
        0,
        "forceRefresh must bypass cache; no cache:1"
    );
    // wiremock verifies expect(2) on drop
    runtime.close().await;
}

// ─── ghStructure ──────────────────────────────────────────────────────────────

/// A repeated `ghStructure` traversal with `maxDepth >= 2` must cache the
/// git-tree response.  The second call must not issue a new tree HTTP request.
///
/// Proof: the mock is mounted with `expect(1)`.  If the second call fetches
/// again, wiremock will record 2 requests and `Mock::verify` will fail.
#[tokio::test]
async fn ghsearch_tree_second_call_hits_cache_and_skips_http() {
    let server = MockServer::start().await;
    let sha = sha('c');

    // Git tree endpoint — must be fetched exactly once.
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/o/r/git/trees/{sha}")))
        .and(query_param("recursive", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": sha,
            "truncated": false,
            "tree": [
                {"path":"src","type":"tree","mode":"040000","sha":"aaa"},
                {"path":"src/lib.rs","type":"blob","mode":"100644","sha":"bbb"},
            ]
        })))
        .expect(1) // second call must be a cache hit
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let runtime = workspace.runtime(&settings);

    let query = json!({
        "owner": "o",
        "repo": "r",
        "branch": sha,
        "maxDepth": 2,   // > 1 activates the ConditionalCache path in traverse()
    });

    let first = call(&runtime, "ghStructure", query.clone()).await.unwrap();
    assert_eq!(row_status(&first), "success");

    // Second call: must be served entirely from cache.
    let second = call(&runtime, "ghStructure", query).await.unwrap();
    assert_eq!(row_status(&second), "success");
    // wiremock expect(1) enforced on Mock drop.
    runtime.close().await;
}

// ─── ghSearchHistory ─────────────────────────────────────────────────────────

/// `ghSearchHistory` bypasses `ConditionalCache` intentionally — history
/// search results are mutable.  Neither call should ever set `cache:1`.
#[tokio::test]
async fn ghsearchhistory_never_sets_cache_flag() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 0,
            "incomplete_results": false,
            "items": []
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let runtime = workspace.runtime(&settings);

    let query = json!({
        "operation": "pullRequest",
        "owner": "o",
        "repo": "r",
    });
    for _ in 0..2 {
        let outcome = call(&runtime, "ghSearchHistory", query.clone())
            .await
            .unwrap();
        assert_eq!(
            row_cache_flag(&outcome),
            0,
            "ghSearchHistory must never set cache:1 — history is mutable"
        );
    }
    runtime.close().await;
}

// ─── ghGetHistoryItem ────────────────────────────────────────────────────────

/// `ghGetHistoryItem` bypasses `ConditionalCache` intentionally.  Two calls to
/// the same PR must never result in `cache:1` — the item may have changed.
#[tokio::test]
async fn ghgethistoryitem_never_sets_cache_flag() {
    let server = MockServer::start().await;
    // Disable GraphQL so the tool falls back to the REST PR endpoint.
    let pr_body = json!({
        "number": 1,
        "title": "cache test PR",
        "state": "open",
        "html_url": "https://github.com/o/r/pull/1",
        "body": "",
        "draft": false,
        "merged": false,
        "user": {"login": "actor"},
        "base": {"ref": "main", "sha": "000", "repo": {"full_name": "o/r"}},
        "head": {"ref": "feat", "sha": "aaa"},
        "labels": [],
        "created_at": "2024-01-01T00:00:00Z",
        "updated_at": "2024-01-01T00:00:00Z",
        "merged_at": null,
        "closed_at": null,
        "additions": 1,
        "deletions": 0,
        "changed_files": 1,
        "commits": 1,
        "comments": 0,
    });
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/pulls/1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pr_body))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let settings = [
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_GITHUB_GRAPHQL", "false".into()),
    ];
    let runtime = workspace.runtime(&settings);

    let query = json!({"operation":"pullRequest","owner":"o","repo":"r","number":1});
    for _ in 0..2 {
        let outcome = call(&runtime, "ghGetHistoryItem", query.clone())
            .await
            .unwrap();
        assert_eq!(
            row_cache_flag(&outcome),
            0,
            "ghGetHistoryItem must never set cache:1 — items are mutable"
        );
    }
    runtime.close().await;
}

// ─── artifactSearch ──────────────────────────────────────────────────────────

/// When `storage.mode=memory` the artifact in-process HTTP cache must be
/// bypassed: both calls to the same package URL must reach the HTTP layer.
///
/// Contrast: with the default `persistent` mode the second call is a cache hit
/// (not tested here to avoid process-static cache pollution across tests).
#[tokio::test]
async fn artifactsearch_memory_storage_mode_bypasses_in_process_cache() {
    let server = MockServer::start().await;
    // Use a unique package name so this test doesn't share state with the
    // process-global artifact cache from other tests.
    let pkg = format!("cache-bypass-test-{}", std::process::id());
    // The npm `exact` code fetches `/{name}/{spec}` where spec defaults to
    // "latest".  The response must be a single-version object (not the full
    // registry envelope) containing at minimum `name` and `version`.
    Mock::given(method("GET"))
        .and(path(format!("/{pkg}/latest")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "name": pkg,
            "version": "1.0.0",
            "description": "cache bypass fixture",
        })))
        .expect(2) // memory mode must not serve the second call from cache
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let registry = server.uri();
    let settings = [
        ("OCTOCODE_STORAGE_MODE", "memory".into()),
        ("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".into()),
        ("MAX_RETRIES", "0".into()),
    ];
    let runtime = workspace.runtime(&settings);

    let query = json!({
        "type": "npm",
        "packageName": pkg,
        "registry": registry,
    });

    for call_n in 1..=2u32 {
        let outcome = call(&runtime, "artifactSearch", query.clone())
            .await
            .unwrap();
        assert_eq!(
            row_status(&outcome),
            "success",
            "call {call_n} must succeed"
        );
    }
    // wiremock enforces expect(2) on drop → fails if second call was cached.
    runtime.close().await;
}

// ─── local tools (no caching) ────────────────────────────────────────────────

/// `localSearch` and `localFetch` never cache their results.  Repeated calls
/// for the same file must never produce a `cache:1` row flag.
#[tokio::test]
async fn local_tools_never_set_cache_flag() {
    let workspace = Workspace::new();
    let target = workspace.write(
        "greet.rs",
        b"pub fn greet() -> &'static str { \"hello\" }\n",
    );
    let runtime = workspace.runtime(&[]);

    for _ in 0..2 {
        // localSearch
        let search = call(
            &runtime,
            "localSearch",
            json!({"path": target.parent().unwrap(), "searchText": "greet"}),
        )
        .await
        .unwrap();
        assert_eq!(
            row_cache_flag(&search),
            0,
            "localSearch must never set cache:1"
        );

        // localFetch
        let fetch = call(&runtime, "localFetch", json!({"path": target}))
            .await
            .unwrap();
        assert_eq!(
            row_cache_flag(&fetch),
            0,
            "localFetch must never set cache:1"
        );
    }
    runtime.close().await;
}
