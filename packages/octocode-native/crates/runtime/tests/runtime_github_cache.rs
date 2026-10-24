use crate::support;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

#[tokio::test]
async fn github_cache_survives_runtime_close() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/source.rs"))
        .respond_with(|request: &Request| {
            if request.headers.get("if-none-match").is_some() {
                ResponseTemplate::new(304)
            } else {
                ResponseTemplate::new(200)
                    .insert_header("etag", "\"v1\"")
                    .set_body_json(json!({
                        "type":"file", "encoding":"base64",
                        "content":STANDARD.encode("source body\n")
                    }))
            }
        })
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let query = json!({"owner":"a", "repo":"b", "path":"source.rs", "ref":"a".repeat(40)});
    let first = workspace.runtime(&settings);
    let outcome = call(&first, "ghGetFileContent", query.clone())
        .await
        .unwrap();
    assert_eq!(row_status(&outcome), "success");
    first.close().await;

    let second = workspace.runtime(&settings);
    let outcome = call(&second, "ghGetFileContent", query).await.unwrap();
    assert_eq!(row_status(&outcome), "success");
    assert_eq!(row_data(&outcome)["content"], "1\tsource body\n");
    let requests = server.received_requests().await.unwrap();
    let contents: Vec<_> = requests
        .iter()
        .filter(|request| request.url.path().contains("/contents/"))
        .collect();
    // The body is keyed by the commit SHA, so the surviving disk entry is
    // served as is: no conditional (304) round trip after a restart.
    assert_eq!(contents.len(), 1);
    assert!(contents[0].headers.get("if-none-match").is_none());
    second.close().await;
}

/// When `storage.mode=memory` the disk tier of `GitHubContentCache` is never
/// written.  A second fresh runtime (which has an empty in-memory cache) must
/// therefore re-fetch the content even though the first runtime served it
/// successfully — there is no disk cache to fall back to.
#[tokio::test]
async fn memory_storage_mode_does_not_persist_github_content_to_disk() {
    let server = MockServer::builder().start().await;
    // Content endpoint: every non-conditional request gets a fresh 200 +
    // ETag; a conditional request would get 304.  In memory-only mode the
    // second runtime must NOT send a conditional request because it has no
    // cached ETag — the disk was never written.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/c/d/contents/readme.md"))
        .respond_with(|request: &Request| {
            if request.headers.get("if-none-match").is_some() {
                // This would only happen if the disk cache had been written.
                ResponseTemplate::new(304)
            } else {
                ResponseTemplate::new(200)
                    .insert_header("etag", "\"mem-v1\"")
                    .set_body_json(json!({
                        "type":"file", "encoding":"base64",
                        "content":STANDARD.encode("mem-only body\n")
                    }))
            }
        })
        .expect(2) // both runtimes must issue unconditional fetches
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let settings = [
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_STORAGE_MODE", "memory".into()),
    ];
    let sha = "c".repeat(40);
    let query = json!({
        "owner": "c", "repo": "d",
        "path": "readme.md",
        "ref": sha,
    });

    // First runtime: fetches and stores in memory (not on disk).
    let first = workspace.runtime(&settings);
    let outcome = call(&first, "ghGetFileContent", query.clone())
        .await
        .unwrap();
    assert_eq!(row_status(&outcome), "success");
    assert!(
        !workspace.home.join("tmp").join("response").exists()
            || std::fs::read_dir(workspace.home.join("tmp").join("response"))
                .map(|mut d| d.next().is_none())
                .unwrap_or(true),
        "memory mode must not write any disk cache files"
    );
    first.close().await;

    // Second runtime: empty memory cache, no disk fallback — must re-fetch.
    let second = workspace.runtime(&settings);
    let outcome = call(&second, "ghGetFileContent", query).await.unwrap();
    assert_eq!(row_status(&outcome), "success");
    assert_eq!(row_data(&outcome)["content"], "1\tmem-only body\n");
    // Verify neither request sent If-None-Match (no cached ETag from disk).
    let requests = server.received_requests().await.unwrap();
    let content_requests: Vec<_> = requests
        .iter()
        .filter(|r| r.url.path().contains("/contents/"))
        .collect();
    assert_eq!(content_requests.len(), 2, "both runtimes must fetch fresh");
    for req in &content_requests {
        assert!(
            req.headers.get("if-none-match").is_none(),
            "memory mode must never send If-None-Match across runtimes"
        );
    }
    second.close().await;
}

/// Repository and history searches follow the code-search cache policy: an
/// identical repeat in the same session is answered from the cache, with no
/// request and no wait for the 2 s search spacing.
#[tokio::test]
async fn repeated_repository_and_history_searches_are_cache_hits() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/repositories"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1, "incomplete_results": false,
            "items": [{"full_name": "a/ripgrep", "name": "ripgrep",
                       "html_url": "https://github.com/a/ripgrep", "default_branch": "main",
                       "created_at": null, "updated_at": null, "pushed_at": null,
                       "language": "Rust", "homepage": null, "license": null}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count": 1, "incomplete_results": false,
            "items": [{"number": 700, "title": "Merged change", "state": "closed",
                       "user": {"login": "alice"}, "labels": [],
                       "pull_request": {"merged_at": "2026-01-01T00:00:00Z"}}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let repo = json!({"operation":"search","keywords": ["ripgrep"]});
    let history =
        json!({"operation": "pullRequest", "owner": "a", "repo": "b", "filters":{"isMerged":true}});
    let mut first = Vec::new();
    for (tool, query) in [("ghSearchRepo", &repo), ("ghSearchHistory", &history)] {
        let outcome = call(&runtime, tool, query.clone()).await.unwrap();
        assert_eq!(
            row_status(&outcome),
            "success",
            "{}",
            outcome.structured_content
        );
        first.push(row_data(&outcome).clone());
    }
    let started = std::time::Instant::now();
    for _ in 0..3 {
        for (index, (tool, query)) in [("ghSearchRepo", &repo), ("ghSearchHistory", &history)]
            .into_iter()
            .enumerate()
        {
            let outcome = call(&runtime, tool, query.clone()).await.unwrap();
            assert_eq!(row_data(&outcome), &first[index], "{tool} repeat");
        }
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "six cached repeats took {elapsed:?}"
    );
    runtime.close().await;
}

/// A branch resolves once per session: a file read and a later commit read
/// of several file batches on the same branch share the memoized single-flight
/// resolution, so `commits/{branch}` is requested once.
#[tokio::test]
async fn a_file_read_and_a_multi_batch_commit_read_resolve_one_branch_once() {
    const SHA: &str = "2222222222222222222222222222222222222222";
    let server = MockServer::builder().start().await;
    let base_url = server.uri();
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("^/api/v3/repos/a/b/commits/"))
        .respond_with(move |request: &Request| {
            if request
                .headers
                .get("accept")
                .is_some_and(|accept| accept == "application/vnd.github.sha")
            {
                return ResponseTemplate::new(200).set_body_string(SHA);
            }
            let first = request.url.query_pairs().all(|(key, value)| key != "page" || value == "1");
            let file = |name: String| {
                json!({"filename":name,"status":"modified","additions":1,"deletions":1,
                    "patch":"@@ -1 +1 @@\n-old\n+new"})
            };
            let files: Vec<_> = if first {
                (0..100).map(|i| file(format!("f-{i:03}.rs"))).collect()
            } else {
                vec![file("last.rs".to_owned())]
            };
            let mut response = ResponseTemplate::new(200).set_body_json(json!({
                "sha": SHA, "files": files,
                "commit": {"message": "change",
                    "author": {"name":"Ada","email":"a@x","date":"2024-01-01T00:00:00Z"}}
            }));
            if first {
                response = response.insert_header(
                    "Link",
                    format!("<{base_url}/api/v3/repos/a/b/commits/{SHA}?page=2&per_page=100>; rel=\"next\""),
                );
            }
            response
        })
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/source.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file", "encoding":"base64", "content":STANDARD.encode("source body\n")
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let file = json!({"owner":"a", "repo":"b", "path":"source.rs", "ref":"main"});
    let outcome = call(&runtime, "ghGetFileContent", file).await.unwrap();
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let commit = json!({"operation":"commit", "owner":"a", "repo":"b", "ref":"main",
        "sections":["files"], "include":["last.rs"]});
    let outcome = call(&runtime, "ghGetHistoryItem", commit).await.unwrap();
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    assert_eq!(row_data(&outcome)["commitSha"], SHA);
    let requests = server.received_requests().await.unwrap();
    let resolutions = requests
        .iter()
        .filter(|request| request.url.path() == "/api/v3/repos/a/b/commits/main")
        .count();
    assert_eq!(resolutions, 1, "one branch resolution per session");
    assert!(
        requests
            .iter()
            .filter(|request| request.url.path().contains("/commits/"))
            .skip(1)
            .all(|request| request.url.path().ends_with(SHA)),
        "the commit batches read the pinned SHA"
    );
    runtime.close().await;
}
