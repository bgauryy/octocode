//! Comparison patches: patch windows, the head/merge-base patch view and matchString file filtering.

use super::{BASE_SHA, HEAD_SHA, mount_capped_compare};
use crate::support::{self};
use serde_json::json;
use support::{Workspace, call, row_data};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A comparison's patch window past the first carries files only: the
/// commit list rode the first window with its own page cursor.
#[tokio::test]
async fn patch_selection_does_not_include_unrequested_commits() {
    let server = MockServer::builder().start().await;
    mount_capped_compare(&server).await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let hop = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":"v1","headRef":"v2",
               "sections":["patches"]}),
    )
    .await
    .expect("compare hop");
    let data = row_data(&hop);
    assert!(data.get("commits").is_none(), "{data}");
    assert!(data["next"].get("nextPage").is_none(), "{data}");
    runtime.close().await;
}

/// A comparison's patch view reads its top file at the head and at the
/// merge base (the old side of a three-dot diff). Two files: a complete
/// one-file answer offers one lead only.
#[tokio::test]
async fn compare_patch_view_reads_head_and_merge_base() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex(format!("^/api/v3/repos/a/b/compare/{BASE_SHA}\\.\\.\\.{HEAD_SHA}$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"ahead","ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE_SHA},"merge_base_commit":{"sha":"3333333ccccccccccccccccccccccccccccccccc"},
            "commits":[{"sha":HEAD_SHA,"commit":{"message":"m","author":{"name":"a","date":"2024-01-01T00:00:00Z"}}}],
            "files":[{"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,
                      "patch":"@@ -5,1 +7,1 @@\n-x\n+y"},
                     {"filename":"docs/b.md","status":"modified","additions":1,"deletions":0,
                      "patch":"@@ -1,0 +1,1 @@\n+z"}]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":BASE_SHA,"headRef":HEAD_SHA,
               "sections":["patches"]}),
    )
    .await
    .expect("compare patches");
    let data = row_data(&outcome);
    assert_eq!(
        data["files"][0]["patch"], "@@ -5,1 +7,1 @@\n5\t-x\n7\t+y",
        "{data}"
    );
    use base64::Engine as _;
    for (pinned, marker) in [
        (HEAD_SHA, "new"),
        ("3333333ccccccccccccccccccccccccccccccccc", "old"),
    ] {
        let content = (1..=20)
            .map(|line| format!("{marker} line {line}\n"))
            .collect::<String>();
        Mock::given(method("GET")).and(wiremock::matchers::path("/api/v3/repos/a/b/contents/src%2Fa.rs"))
            .and(wiremock::matchers::query_param("ref",pinned))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"type":"file","path":"src/a.rs","encoding":"base64","content":base64::engine::general_purpose::STANDARD.encode(content)})))
            .expect(1).mount(&server).await;
        let lead = support::checks::related_call(data, "ghGetFileContent", |input| {
            input["queries"][0]["ref"] == pinned
        });
        let read = call(&runtime, "ghGetFileContent", lead["input"].clone())
            .await
            .expect("pinned side read");
        assert!(
            row_data(&read)
                .to_string()
                .contains(&format!("{marker} line 7")),
            "{}",
            read.structured_content
        );
    }
    runtime.close().await;
}

/// A compare patch grep keeps the files whose patch it hits, each narrowed
/// to its hits, like a pull-request grep; the internal narrowed-view marker
/// stays off the rows.
#[tokio::test]
async fn compare_match_string_keeps_only_hit_files() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path_regex(format!("^/api/v3/repos/a/b/compare/{BASE_SHA}\\.\\.\\.{HEAD_SHA}$")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"ahead","ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE_SHA},"merge_base_commit":{"sha":BASE_SHA},
            "commits":[{"sha":HEAD_SHA,"commit":{"message":"m","author":{"name":"a","date":"2024-01-01T00:00:00Z"}}}],
            "files":[
                {"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,
                 "patch":"@@ -1,3 +1,3 @@\n a\n-old needle\n+new needle\n b"},
                {"filename":"docs/b.md","status":"modified","additions":1,"deletions":0,
                 "patch":"@@ -1,0 +1,1 @@\n+unrelated"}
            ]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":BASE_SHA,"headRef":HEAD_SHA,
               "sections":["patches"],"matchString": ["needle"],"contextLines": {"before":0,"after":0}}),
    )
    .await
    .expect("compare grep");
    let data = row_data(&outcome);
    let files = data["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{data}");
    assert_eq!(files[0]["path"], "src/a.rs", "{data}");

    let patch = files[0]["patch"].as_str().expect("patch");
    assert!(
        patch.contains("needle") && !patch.contains("\n a\n"),
        "{patch}"
    );
    runtime.close().await;
}
