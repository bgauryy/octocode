//! ghGetHistoryItem pull request reads: output contract, body preview, squash merges and issue numbers.

use crate::support::{self};
use serde_json::json;
use support::{Workspace, call, checks, row_data, row_status};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A squash-merge headline names its pull request: read it directly
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
    let pr_sha = "d".repeat(40);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/8506"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number":8506,"title":"Linked fix","state":"closed","body":"requested PR",
            "user":{"login":"Alice"},"head":{"sha":pr_sha,"ref":"fix"},"base":{"ref":"main"}
        })))
        .expect(1)
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
    let read = &crate::support::checks::find_related_call(data, "ghGetHistoryItem", |input| {
        input["queries"][0]["operation"] == "pullRequest"
    })
    .unwrap_or(&serde_json::Value::Null);
    assert_eq!(read["tool"], "ghGetHistoryItem", "{data}");
    assert_eq!(read["input"]["queries"][0]["number"], 8506, "{data}");
    assert!(
        crate::support::checks::find_related_call(
            data,
            "ghSearchHistory",
            |input| input["queries"][0]["operation"] == "pullRequest"
        )
        .unwrap_or(&serde_json::Value::Null)
        .is_null(),
        "{data}"
    );
    let linked = call(&runtime, "ghGetHistoryItem", read["input"].clone())
        .await
        .expect("linked PR executes");
    assert_eq!(row_data(&linked)["pullRequests"][0]["number"], 8506);
    assert_eq!(row_data(&linked)["pullRequests"][0]["headSha"], pr_sha);
    checks::assert_output_contract("ghGetHistoryItem", &outcome.structured_content);
    runtime.close().await;
}

#[tokio::test]
async fn gh_get_history_item_pull_request_without_content_passes_output_contract() {
    // A summary previews text; its issued continuation returns the full
    // multibyte body unchanged.
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
            "head": {"sha": "dddddddddddddddddddddddddddddddddddddddd", "ref": "feat/buf"},
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
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let read = checks::related_call(row_data(&outcome), "ghGetHistoryItem", |input| {
        input["queries"][0]["sections"]
            .as_array()
            .is_some_and(|sections| sections.contains(&json!("body")))
    });
    let full = call(&runtime, "ghGetHistoryItem", read["input"].clone())
        .await
        .expect("body replay");
    assert_eq!(row_data(&full)["pullRequests"][0]["body"], body);
    runtime.close().await;
}

/// A pull-request read of a number that is an issue offers the issue
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
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":9}),
    )
    .await
    .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "error", "{data}");
    let read = &crate::support::checks::find_related_call(data, "ghGetHistoryItem", |input| {
        input["queries"][0]["operation"] == "issue"
    })
    .unwrap_or(&serde_json::Value::Null)["input"]["queries"][0];
    assert_eq!(read["operation"], "issue", "{data}");
    assert_eq!(read["number"], 9, "{data}");
    let linked = support::checks::related_call(data, "ghGetHistoryItem", |input| {
        input["queries"][0]["operation"] == "issue"
    });
    let issue = call(&runtime, "ghGetHistoryItem", linked["input"].clone())
        .await
        .expect("corrected issue read");
    assert_eq!(row_data(&issue)["issues"][0]["number"], 9);
    assert!(
        data["error"].as_str().unwrap_or("").contains("issue"),
        "{data}"
    );
    runtime.close().await;
}

/// A PR summary previews the opening of its verbatim body (an HTML
/// comment is text like any other, never minified away) and leads to the
/// whole body.
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
            "user": {"login": "bob"}, "head": {"sha": "dddddddddddddddddddddddddddddddddddddddd", "ref": "feat"}, "base": {"ref": "main"},
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
    assert!(
        preview.starts_with("<!-- template: describe the change --> Fixes the cache race."),
        "{preview}"
    );
    assert!(
        preview.ends_with('…') && preview.chars().count() <= 301,
        "{preview}"
    );
    let read = checks::related_call(data, "ghGetHistoryItem", |input| {
        input["queries"][0]["sections"]
            .as_array()
            .is_some_and(|sections| sections.contains(&json!("body")))
    });
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let full = call(&runtime, "ghGetHistoryItem", read["input"].clone())
        .await
        .expect("whole body");
    assert_eq!(row_data(&full)["pullRequests"][0]["body"], body);
    runtime.close().await;
}
