use crate::support;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

mod pinned_walks;
mod session_repeats;

/// Reads of file contents `server` received.
async fn contents_reads(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path().contains("/contents/"))
        .count()
}

/// A commit-pinned read cannot change: a restarted runtime answers it from
/// what the last one saw, with nothing sent, and the same evidence.
#[tokio::test]
async fn a_restarted_runtime_answers_a_pinned_file_with_nothing_sent() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/source.rs"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"v1\"")
                .set_body_json(json!({
                    "type":"file", "encoding":"base64",
                    "content":STANDARD.encode("source body\n")
                })),
        )
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
    let answered = row_data(&outcome).clone();
    first.close().await;

    let second = workspace.runtime(&settings);
    let outcome = call(&second, "ghGetFileContent", query).await.unwrap();
    assert_eq!(row_status(&outcome), "success");
    assert_eq!(row_data(&outcome), &answered);
    assert_eq!(answered["content"], "1\tsource body\n");
    second.close().await;
    server.verify().await;
}

/// A mutable read is confirmed, not assumed: the restarted runtime asks with
/// the validator it kept, and GitHub's 304 answers the whole row. A change
/// reaches the next read.
#[tokio::test]
async fn a_restarted_runtime_confirms_a_mutable_read_and_sees_a_change() {
    let server = MockServer::builder().start().await;
    let version = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1));
    let current = version.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .respond_with(move |request: &Request| {
            let version = current.load(std::sync::atomic::Ordering::SeqCst);
            let etag = format!("\"v{version}\"");
            if request
                .headers
                .get("if-none-match")
                .is_some_and(|presented| presented == etag.as_str())
            {
                return ResponseTemplate::new(304).insert_header("etag", etag.as_str());
            }
            ResponseTemplate::new(200)
                .insert_header("etag", etag.as_str())
                .set_body_json(json!([{
                    "sha": format!("{version}").repeat(40),
                    "commit": {"message": format!("change {version}"),
                        "author": {"name":"Ada","email":"a@x","date":"2024-01-01T00:00:00Z"}}
                }]))
        })
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let query = json!({"operation":"commit", "owner":"a", "repo":"b", "limit":1});
    let first = workspace.runtime(&settings);
    let outcome = call(&first, "ghSearchHistory", query.clone())
        .await
        .unwrap();
    let answered = row_data(&outcome).clone();
    first.close().await;

    let second = workspace.runtime(&settings);
    let confirmed = call(&second, "ghSearchHistory", query.clone())
        .await
        .unwrap();
    assert_eq!(
        row_data(&confirmed),
        &answered,
        "a 304 answers the same row"
    );
    second.close().await;
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].headers["if-none-match"], "\"v1\"");

    version.store(2, std::sync::atomic::Ordering::SeqCst);
    let third = workspace.runtime(&settings);
    let changed = call(&third, "ghSearchHistory", query).await.unwrap();
    assert!(
        row_data(&changed).to_string().contains("change 2"),
        "{}",
        changed.structured_content
    );
    third.close().await;
}

/// GitHub search has no validator. A repeat spends no quota and says when its
/// answer was observed: a row reused unasked is never presented as current.
#[tokio::test]
async fn a_repeated_search_spends_no_quota_and_discloses_when_it_was_observed() {
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
    let workspace = Workspace::new();
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let query = json!({"operation":"search","keywords": ["ripgrep"]});
    let first = workspace.runtime(&settings);
    let fresh = call(&first, "ghSearchRepo", query.clone()).await.unwrap();
    assert!(
        row_data(&fresh).get("hints").is_none(),
        "{}",
        fresh.structured_content
    );
    let observed_from = fresh.structured_content.clone();
    first.close().await;

    let second = workspace.runtime(&settings);
    let reused = call(&second, "ghSearchRepo", query).await.unwrap();
    assert_eq!(row_status(&reused), "success");
    assert_eq!(
        row_data(&reused)["repositories"],
        row_data(&fresh)["repositories"],
        "{observed_from}"
    );
    let hint = row_data(&reused)["hints"][0].as_str().unwrap_or_default();
    assert!(hint.contains("observed"), "{}", reused.structured_content);
    assert!(hint.contains("20"), "names the time: {hint}");
    second.close().await;
    server.verify().await;
}

/// A credential the gate would rewrite never reaches the persistent store.
/// The answer still serves its own process; a restarted one reads again.
#[tokio::test]
async fn an_answer_holding_a_credential_never_reaches_disk() {
    let secret = format!("token = \"ghp_{}\"\n", "a1B2c3D4e5F6".repeat(3));
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/secret.rs"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            secret.as_bytes(),
            "application/vnd.github.raw; charset=utf-8",
        ))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let settings = [("GITHUB_API_URL", format!("{}/api/v3", server.uri()))];
    let query = json!({"owner":"a", "repo":"b", "path":"secret.rs", "ref":"b".repeat(40)});
    let first = workspace.runtime(&settings);
    let outcome = call(&first, "ghGetFileContent", query.clone())
        .await
        .unwrap();
    assert_eq!(row_status(&outcome), "success");
    let again = call(&first, "ghGetFileContent", query.clone())
        .await
        .unwrap();
    assert_eq!(row_data(&again)["content"], row_data(&outcome)["content"]);
    assert_eq!(
        contents_reads(&server).await,
        1,
        "the process that holds it answers from memory"
    );
    first.close().await;
    assert!(
        !pinned_walks::any_file_holds(&workspace.home, b"ghp_"),
        "no file under the home holds the credential"
    );

    let second = workspace.runtime(&settings);
    let outcome = call(&second, "ghGetFileContent", query).await.unwrap();
    assert_eq!(row_status(&outcome), "success");
    assert_eq!(contents_reads(&server).await, 2);
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
