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
    let read = &crate::support::checks::hint(data, "readPullRequest");
    assert_eq!(read["tool"], "ghGetHistoryItem", "{data}");
    assert_eq!(read["query"]["queries"][0]["number"], 8506, "{data}");
    assert!(
        crate::support::checks::hint(data, "findPullRequest").is_null(),
        "{data}"
    );
    checks::assert_output_contract("ghGetHistoryItem", &outcome.structured_content);
    runtime.close().await;
}

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
    let get_body =
        &crate::support::checks::hint(row_data(&outcome), "readFiles")["query"]["queries"][0];
    // Continuations omit defaulted fields; validation restores them on replay.
    assert!(get_body.get("pageSize").is_none(), "{get_body}");
    assert!(get_body.get("minify").is_none(), "{get_body}");
    assert_eq!(get_body["sections"], json!(["body", "files"]), "{get_body}");
    let replayed = octocode_native::contracts::validate_query("ghGetHistoryItem", get_body.clone())
        .expect("compact continuation validates");
    // pageSize has no contract default: each surface sizes its own page.
    assert!(replayed.get("pageSize").is_none(), "{replayed}");
    // History text is always verbatim: no minify field exists.
    assert!(replayed.get("minify").is_none(), "{replayed}");
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
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,"extras":[]}),
    )
    .await
    .expect("error row");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "error", "{data}");
    let read = &crate::support::checks::hint(data, "readIssue")["query"]["queries"][0];
    assert_eq!(read["operation"], "issue", "{data}");
    assert_eq!(read["number"], 9, "{data}");
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
    assert!(
        preview.starts_with("<!-- template: describe the change --> Fixes the cache race."),
        "{preview}"
    );
    assert!(
        preview.ends_with('…') && preview.chars().count() <= 301,
        "{preview}"
    );
    let leads = crate::support::checks::leads(data);
    assert!(
        leads.iter().any(|lead| lead.query["queries"][0]["sections"]
            .as_array()
            .is_some_and(|sections| sections.contains(&json!("body")))),
        "no whole-body read: {data}"
    );
    runtime.close().await;
}
