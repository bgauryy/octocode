// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used)]

use crate::support;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

#[tokio::test]
async fn github_cache_survives_runtime_close_until_explicitly_cleared() {
    let server = MockServer::start().await;
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
        .expect(2)
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
    let outcome = call(&second, "ghGetFileContent", query.clone())
        .await
        .unwrap();
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

    second.clear_github_cache();
    let outcome = call(&second, "ghGetFileContent", query).await.unwrap();
    assert_eq!(row_status(&outcome), "success");
    let requests = server.received_requests().await.unwrap();
    let last_content = requests
        .iter()
        .rev()
        .find(|request| request.url.path().contains("/contents/"))
        .unwrap();
    assert!(last_content.headers.get("if-none-match").is_none());
    second.close().await;
}

/// When `storage.mode=memory` the disk tier of `GitHubContentCache` is never
/// written.  A second fresh runtime (which has an empty in-memory cache) must
/// therefore re-fetch the content even though the first runtime served it
/// successfully — there is no disk cache to fall back to.
#[tokio::test]
async fn memory_storage_mode_does_not_persist_github_content_to_disk() {
    let server = MockServer::start().await;
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
