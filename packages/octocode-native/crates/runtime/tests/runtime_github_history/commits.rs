//! ghGetHistoryItem commit reads: REST fetch, capped file scans, path-scoped totals and patch views.

use crate::support::{self};
use serde_json::json;
use support::{Workspace, call, checks, inventory_len, row_data, row_status};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
    let data = row_data(&outcome);
    assert_eq!(data["message"], "fix: critical regression", "{data}");
    // A SHA ref is not restated beside the same sha.
    let data = row_data(&outcome);
    assert_eq!(data["commitSha"], sha, "{data}");

    assert_eq!(
        crate::support::checks::find_related_call(
            data,
            "ghSearchHistory",
            |input| input["queries"][0]["operation"] == "pullRequest"
        )
        .unwrap_or(&serde_json::Value::Null)["input"]["queries"][0]["keywords"],
        json!([sha]),
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
        json!({"operation": "commit", "owner": "a", "repo": "b", "ref": sha, "include": ["wanted/"]}),
    )
    .await
    .expect("capped commit scan");
    let data = row_data(&outcome);
    assert_eq!(
        data["changedFilesCountScope"], "partial",
        "{}",
        outcome.structured_content
    );
    assert_eq!(data["terminalLimit"], true, "{data}");
    assert_ne!(
        data.pointer("/coverage/state")
            .and_then(serde_json::Value::as_str),
        Some("complete"),
        "{data}"
    );
    runtime.close().await;
}

/// A commit's patch view names the commit by its headline once; the
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
        json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"sections":["patches"]}),
    )
    .await
    .expect("commit patches");
    let data = row_data(&outcome);
    assert!(data.get("message").is_none(), "{data}");
    assert_eq!(data["messageHeadline"], "Fix the parser (#42)", "{data}");
    checks::assert_absent(data, "Long rationale");
    let read = checks::related_call(data, "ghGetHistoryItem", |input| {
        input["queries"][0]["operation"] == "commit"
            && input["queries"][0].get("sections").is_none()
    });
    let full = call(&runtime, "ghGetHistoryItem", read["input"].clone())
        .await
        .expect("whole commit message");
    assert_eq!(row_data(&full)["message"], message);
    // The pull request the headline names stays a lead beside it: a commit
    // view reads the same discussion route, one file or many.
    assert_eq!(
        crate::support::checks::find_related_call(
            data,
            "ghGetHistoryItem",
            |input| input["queries"][0]["operation"] == "pullRequest"
        )
        .unwrap_or(&serde_json::Value::Null)["input"],
        json!({"queries":[{"operation":"pullRequest","owner":"a","repo":"b","number":42}]}),
        "{data}"
    );
    runtime.close().await;
}

/// A path-scoped commit read keeps the whole commit's counts
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
        json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"include":["src/**"]}),
    )
    .await
    .expect("commit");
    let data = row_data(&outcome);
    assert_eq!(data["changedFilesCount"], 2, "{data}");
    assert_eq!(data["additions"], 30, "{data}");
    assert!(data.get("commitTotals").is_none(), "{data}");
    assert_eq!(inventory_len(&data["files"]), 1, "{data}");
    runtime.close().await;
}

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
            // Two changed files: a single-hit row keeps one lead only.
            "files": [{"filename": "src/a.rs", "status": "modified", "additions": 2, "deletions": 1,
                       "patch": "@@ -40,3 +60,4 @@ fn f\n a\n-b\n+c\n+d\n e"},
                      {"filename": "src/b.rs", "status": "modified", "additions": 1, "deletions": 0,
                       "patch": "@@ -1,1 +1,2 @@\n x\n+y"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"commit","owner":"a","repo":"b","ref":sha,"sections":["patches"]}),
    )
    .await
    .expect("commit patches");
    let data = row_data(&outcome);
    assert_eq!(
        data["files"][0]["patch"], "@@ -40,3 +60,4 @@ fn f\n60\t a\n41\t-b\n61\t+c\n62\t+d\n63\t e",
        "{data}"
    );
    use base64::Engine as _;
    for (pinned, marker) in [(sha, "new"), (parent, "old")] {
        let content = (1..=100)
            .map(|line| format!("{marker} line {line}\n"))
            .collect::<String>();
        Mock::given(method("GET")).and(path("/api/v3/repos/a/b/contents/src%2Fa.rs"))
            .and(wiremock::matchers::query_param("ref",pinned))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","path":"src/a.rs","encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(content)})))
            .expect(1).mount(&server).await;
        let lead = checks::related_call(data, "ghGetFileContent", |input| {
            input["queries"][0]["ref"] == pinned
        });
        let read = call(&runtime, "ghGetFileContent", lead["input"].clone())
            .await
            .expect("pinned side read");
        assert_eq!(row_status(&read), "success", "{}", read.structured_content);
        let file = row_data(&read);
        assert!(
            file.to_string().contains(&format!("{marker} line 41")),
            "{file}"
        );
    }
    runtime.close().await;
}
