//! Repeats inside one running runtime: a repeated search spends one search, and
//! a branch resolves once per call, with no session memo.

use crate::support::{Workspace, call, row_data, row_status};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// An identical repository or history search repeated in one session spends
/// one search and answers the same evidence: GitHub search has no validator,
/// so inside the window its answer stands, and the repeats wait for nothing.
#[tokio::test]
async fn repeated_repository_and_history_searches_spend_one_search() {
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
            let mut data = row_data(&outcome).clone();
            assert!(
                data["hints"][0]
                    .as_str()
                    .is_some_and(|hint| hint.contains("observed"))
            );
            data.as_object_mut().unwrap().remove("hints");
            assert_eq!(&data, &first[index], "{tool} repeat");
        }
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "six repeats took {elapsed:?}"
    );
    server.verify().await;
    runtime.close().await;
}

/// A branch resolves once per call: a file read and a later commit read of
/// several file batches on the same branch each resolve it afresh (no
/// session memo), and every batch of the commit read reads the one SHA its
/// call pinned.
#[tokio::test]
async fn each_call_resolves_its_branch_once_and_batches_read_the_pinned_sha() {
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
    assert_eq!(resolutions, 2, "one branch resolution per call");
    let batches = requests
        .iter()
        .filter(|request| request.url.path().contains("/commits/"))
        .filter(|request| request.url.path() != "/api/v3/repos/a/b/commits/main")
        .collect::<Vec<_>>();
    assert!(batches.len() > 1, "a multi-batch read");
    assert!(
        batches
            .iter()
            .all(|request| request.url.path().ends_with(SHA)),
        "the commit batches read the pinned SHA"
    );
    runtime.close().await;
}
