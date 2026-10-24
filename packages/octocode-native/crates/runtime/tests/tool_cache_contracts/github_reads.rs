//! Cacheable GitHub reads: a repeated ghGetFileContent or ghStructure call
//! is served from the cache (and `forceRefresh` bypasses it), while
//! ghGetHistoryItem revalidates every read.
use super::*;

/// A second identical `ghGetFileContent` call must be served from the
/// `GitHubContentCache` and report `cache:1` in the result row.
///
/// Flow:
///  1. First call: 200 + ETag "v1"  →  stored in cache, `cache` flag absent.
///  2. Second call: If-None-Match sent, server replies 304  →  served from
///     cache, `cache:1` set in the result row.
#[tokio::test]
async fn ghgetfilecontent_second_call_sets_cache_flag() {
    let server = MockServer::builder().start().await;
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
    let settings = support::github_api(&server);
    let query = json!({"owner":"o","repo":"r","path":"file.rs","ref":sha});

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
    assert_eq!(row_data(&second)["content"], "1\thello cache\n");
    runtime.close().await;
}

/// `forceRefresh: true` must bypass the populated cache and issue a fresh HTTP
/// request, leaving `cache:1` absent from the result row.
#[tokio::test]
async fn ghgetfilecontent_force_refresh_bypasses_populated_cache() {
    let server = MockServer::builder().start().await;
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
    let settings = support::github_api(&server);
    let runtime = workspace.runtime(&settings);

    // First call: populates cache.
    let first = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"o","repo":"r","path":"fresh.rs","ref":sha}),
    )
    .await
    .unwrap();
    assert_eq!(row_status(&first), "success");

    // Second call with forceRefresh: must not use cache.
    let second = call(
        &runtime,
        "ghGetFileContent",
        json!({"owner":"o","repo":"r","path":"fresh.rs","ref":sha,"forceRefresh":true}),
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

/// A repeated `ghStructure` traversal with `maxDepth >= 2` must cache the
/// git-tree response.  The second call must not issue a new tree HTTP request.
///
/// Proof: the mock is mounted with `expect(1)`.  If the second call fetches
/// again, wiremock will record 2 requests and `Mock::verify` will fail.
#[tokio::test]
async fn ghsearch_tree_second_call_hits_cache_and_skips_http() {
    let server = MockServer::builder().start().await;
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
    let settings = support::github_api(&server);
    let runtime = workspace.runtime(&settings);

    let query = json!({
        "operation": "tree",
        "owner": "o",
        "repo": "r",
        "ref": sha,
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

/// `ghGetHistoryItem` revalidates every read: the second read of the same PR
/// sends the stored ETag, a 304 serves the same item without a body, and the
/// row never claims `cache:1` (the item was checked, not assumed).
#[tokio::test]
async fn ghgethistoryitem_revalidates_with_a_conditional_read() {
    let server = MockServer::builder().start().await;
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
        "base": {"ref": "main", "sha": "0".repeat(40), "repo": {"full_name": "o/r"}},
        "head": {"ref": "feat", "sha": "a".repeat(40)},
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
        .and(wiremock::matchers::header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304).insert_header("etag", "\"v1\""))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/pulls/1"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_json(pr_body),
        )
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let settings = [
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_GITHUB_GRAPHQL", "false".into()),
    ];
    let runtime = workspace.runtime(&settings);

    let query = json!({"operation":"pullRequest","owner":"o","repo":"r","number":1});
    let mut rows = Vec::new();
    for _ in 0..2 {
        let outcome = call(&runtime, "ghGetHistoryItem", query.clone())
            .await
            .unwrap();
        rows.push(row_data(&outcome)["pullRequests"].clone());
        assert_eq!(
            row_cache_flag(&outcome),
            0,
            "ghGetHistoryItem must never set cache:1 — items are mutable"
        );
    }
    assert_eq!(rows[0], rows[1], "a 304 serves the same item");
    let conditional = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.headers.contains_key("if-none-match"))
        .count();
    assert_eq!(conditional, 1, "the second read revalidates");
    runtime.close().await;
}
