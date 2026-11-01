//! One workspace-relative path form and one name per purpose.

use crate::support::{self, Run as _, Workspace};
use serde_json::json;

// ---------- one workspace-relative path form ----------

#[tokio::test]
async fn ast_search_error_rows_name_workspace_relative_paths() {
    let workspace = Workspace::new();
    workspace.write("sub/dir/Cargo.toml", "[package]\nname = \"x\"\n");
    let runtime = workspace.runtime(&[]);
    let outcome = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"symbols","path":"sub/dir/Cargo.toml"}),
    )
    .await;
    let row = &outcome.structured_content["results"][0];
    assert_eq!(row["status"], "error", "{row}");
    // The row repeats no unchanged request field; its recovery names the
    // whole path, since a basename (`Cargo.toml`) cannot be pasted into localFetch.
    assert!(row["data"].get("path").is_none(), "{row}");
    assert_eq!(
        row["data"]["related"][0]["input"]["queries"][0]["path"], "sub/dir/Cargo.toml",
        "{row}"
    );
    runtime.close().await;
}

#[tokio::test]
async fn ast_search_matches_name_workspace_relative_paths() {
    let workspace = Workspace::new();
    workspace.write("sub/dir/a.rs", "fn a() { old(1); }\n");
    workspace.write("sub/dir/b.rs", "fn b() { old(2); }\n");
    let runtime = workspace.runtime(&[]);
    let outcome = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"sub/dir","parser":"rust","rule":{"pattern":"old($A)"}}),
    )
    .await;
    let data = outcome.data();
    for file in data["files"].as_array().expect("files") {
        let path = file["path"].as_str().expect("file path");
        assert!(path.starts_with("sub/dir/"), "{path}: {data}");
    }
    runtime.close().await;
}

// ---------- one name per purpose ----------

#[tokio::test]
async fn artifact_search_takes_ecosystem_and_registry_url() {
    use wiremock::matchers::path_regex;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::builder().start().await;
    Mock::given(path_regex(".*"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"error":"Not found"})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("OCTOCODE_ALLOW_PRIVATE_REGISTRY", "true".into())]);
    // The new names reach the registry the row names.
    let outcome = support::run(
        &runtime,
        "artifactSearch",
        json!({"operation":"resolve","ecosystem":"npm","packageName":"zzqq-missing","registryUrl":server.uri()}))
    .await;
    let requests = server.received_requests().await.expect("recorded");
    assert!(!requests.is_empty(), "{}", outcome.structured_content);
    // Retired field names fail admission before reaching the registry.
    for (row, removed) in [
        (
            json!({"operation":"resolve","type":"npm","packageName":"zod"}),
            "type",
        ),
        (
            json!({"operation":"resolve","ecosystem":"npm","packageName":"zod","registry":server.uri()}),
            "registry",
        ),
    ] {
        let error = runtime
            .run(
                "d1-artifact-retired".into(),
                "artifactSearch".into(),
                json!({"queries":[row]}),
            )
            .await
            .expect_err("a retired input field is rejected");
        assert_eq!(error.code, octocode_contract_types::ErrorCode::InvalidInput);
        assert!(
            error
                .validation_issues
                .expect("issues")
                .iter()
                .any(|issue| issue.path.last().is_some_and(|field| field == removed))
        );
    }
    runtime.close().await;
}

#[tokio::test]
async fn history_reviews_and_issue_comments_name_author_and_commit_sha() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    const HEAD: &str = "1111111111111111111111111111111111111111";
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"pullRequest": {
                "number": 7, "title": "Fix parser", "url": "https://x", "state": "OPEN",
                "body": "Fixes the parser.", "isDraft": false, "author": {"login": "bob"},
                "labels": {"pageInfo": {"hasNextPage": false}, "nodes": []},
                "baseRefName": "main", "baseRefOid": "2222222222222222222222222222222222222222",
                "headRefName": "feat/parser", "headRefOid": HEAD,
                "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z",
                "closedAt": null, "mergedAt": null, "mergeCommit": null,
                "comments": {"totalCount": 0}, "changedFiles": 1, "additions": 1, "deletions": 0,
                "commitsCount": {"totalCount": 1}, "reviewThreads": {"totalCount": 0},
                "files": {"pageInfo": {"hasNextPage": false}, "nodes": [
                    {"path": "src/a.rs", "additions": 1, "deletions": 0, "changeType": "MODIFIED"}
                ]},
                "reviews": {"pageInfo": {"hasNextPage": false}, "nodes": [
                    {"databaseId": 11, "author": {"login": "ann"}, "state": "APPROVED",
                     "body": "ok", "submittedAt": "2026-01-03T00:00:00Z", "commit": {"oid": HEAD}}
                ]}
            }}
        }})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 9, "title": "Bug", "state": "open", "body": "repro",
            "user": {"login": "alice"}, "labels": [], "comments": 1,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/9/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 1, "user": {"login": "bob"}, "body": "same here",
             "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"}
        ])))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let reviews = support::run(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":7,"sections":["body","reviews"]}))
    .await;
    let review = &reviews.data()["pullRequests"][0]["reviews"][0];
    let seen = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|request| format!("{} {}", request.method, request.url.path()))
        .collect::<Vec<_>>();
    assert_eq!(
        review["author"], "ann",
        "{} {seen:?}",
        reviews.structured_content
    );
    assert_eq!(review["commitSha"], HEAD, "{}", reviews.structured_content);
    let comments = support::run(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"issue","owner":"a","repo":"b","number":9,"sections":["comments"]}),
    )
    .await;
    let comment = &comments.data()["issues"][0]["comments"][0];
    assert_eq!(comment["author"], "bob", "{}", comments.structured_content);
    runtime.close().await;
}
