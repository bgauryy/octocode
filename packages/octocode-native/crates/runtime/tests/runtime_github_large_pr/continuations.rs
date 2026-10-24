//! Pull request continuations: identity-only headers, pinned heads, merge state, verbatim views and file-page grids.

use super::{SHA, mount_file_batches, mount_pr, numbered, pr, raw_patch, rest_file, run};
use crate::support::{self, Run as _};
use serde_json::{Value, json};
use std::time::Duration;
use support::Workspace;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn inventory_cursors_keep_identity_and_reach_each_file_once() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    mount_file_batches(
        &server,
        vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut input = json!({"queries":[{"operation":"pullRequest","owner":"a","repo":"b","number":9,"sections":["files"]}],"responseChars":3000});
    let mut seen = Vec::new();
    for _ in 0..100 {
        let result = support::run(&runtime, "ghGetHistoryItem", input).await;
        let data = result.data();
        let row = &data["pullRequests"][0];
        if row.get("headSha").is_some() {
            assert_eq!(row["headSha"], SHA);
        }
        seen.extend(inventory_paths(&row["files"]));
        let next = result
            .structured_content
            .pointer("/responsePagination/next")
            .or_else(|| data.pointer("/next/nextFilePage"));
        let Some(next) = next else { break };
        assert_eq!(next["kind"], "continue");
        assert!(next["input"]["cursor"].is_string());
        input = next["input"].clone();
    }
    assert_eq!(seen.len(), 250);
    assert_eq!(
        seen.iter().collect::<std::collections::BTreeSet<_>>().len(),
        250
    );
    runtime.close().await;
}

#[tokio::test]
async fn patch_window_does_not_repeat_the_file_cursor_in_content_pagination() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 2).await;
    let big = format!("@@ -1,2000 +1,2000 @@\n{}", "+line\n".repeat(12_000));
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/big.rs", Some(&big), 4_000, 0),
            rest_file("src/small.rs", Some("@@ -1 +1 @@\n-a\n+b"), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"sections":["patches"],"include":["src/big.rs"]}),
    )
    .await;
    let row = &data["pullRequests"][0];
    let file = &row["files"][0];
    assert_eq!(file["patchPagination"]["hasMore"], true, "{row}");
    let pages = &row["contentPagination"];
    // The finished single-page file list and the cursor copy add nothing.
    assert_eq!(pages["files"]["hasMore"], false, "{pages}");
    assert_eq!(pages["patches"]["offset"], 0, "{pages}");
    assert!(pages["patches"].get("totalChars").is_none(), "{pages}");
    assert!(data["next"]["continuePatch"].is_object(), "{data}");
}

/// The default PR view is GitHub's verbatim text: a body's HTML comment
/// is evidence and stays, with no raw re-read to chase; code hunks are
/// never minified either: every context line arrives, numbered on the new
/// side.
#[tokio::test]
async fn pr_views_are_verbatim_by_default() {
    let server = MockServer::builder().start().await;
    let raw_body = "Fixes the cache.\n<!-- reviewer checklist: perf tested -->\nDetails here.";
    let mut meta = pr(2);
    meta["body"] = json!(raw_body);
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(meta))
        .mount(&server)
        .await;
    let context = (1..=60).map(|n| format!(" ctx {n}\n")).collect::<String>();
    let big = format!("@@ -1,121 +1,121 @@ fn cache()\n{context}-old\n+new\n{context}");
    let small = "@@ -1 +1 @@\n-a\n+b";
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/big.rs", Some(&big), 1, 1),
            rest_file("src/small.rs", Some(small), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(&server, json!({"sections":["body","patches"]})).await;
    let row = &data["pullRequests"][0];
    assert_eq!(row["body"], raw_body, "{row}");
    assert!(row.get("bodyView").is_none(), "{row}");
    let files = row["files"].as_array().expect("files");
    let whole = files
        .iter()
        .find(|f| f["path"] == "src/big.rs")
        .and_then(|f| f["patch"].as_str())
        .expect("big");
    assert_eq!(raw_patch(whole), big, "{whole}");
    assert!(
        whole.starts_with("@@ -1,121 +1,121 @@ fn cache()\n1\t ctx 1\n"),
        "{whole}"
    );
    assert!(
        whole.contains("\n61\t-old\n61\t+new\n62\t ctx 1\n"),
        "{whole}"
    );
    assert!(data["next"]["readUntrimmed"].is_null(), "{data}");
    assert!(data["next"]["readRawBody"].is_null(), "{data}");
    let merge_read = support::checks::related_call(&data, "ghGetFileContent", |input| {
        input["queries"][0]["ref"] == "fedcba9876543210fedcba9876543210fedcba98"
    });
    assert_eq!(merge_read["kind"], "related");
}

/// A merged PR's merge state rides every PR row, whatever the content
/// selection: summary, body + inventory, inventory, patch read and a later
/// file page all carry `state:"merged"` and the `mergeCommitSha`; a read that
/// names no section (the summary, a bare `matchString`) also carries
/// `mergedAt`, `baseBranch` and the labels (never `closedAt`).
#[tokio::test]
async fn merged_pr_rows_keep_merge_state_on_every_read() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    let mut batches = vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)];
    batches[0][0] = rest_file("src/hit.rs", Some("@@ -1 +1 @@\n-a\n+needle"), 1, 1);
    mount_file_batches(&server, batches, Duration::ZERO).await;
    for (label, query, _full_header) in [
        ("summary", json!({}), true),
        (
            "body+inventory",
            json!({"sections":["body","files"]}),
            false,
        ),
        ("inventory", json!({"sections":["files"]}), false),
        (
            "patches",
            json!({"sections":["patches"],"include":["src/hit.rs"]}),
            false,
        ),
        ("matchString", json!({"matchString": ["needle"]}), true),
    ] {
        let data = run(&server, query).await;
        let row = &data["pullRequests"][0];
        assert_eq!(row["state"], "merged", "{label}: {row}");
        assert_eq!(
            row["mergeCommitSha"], "fedcba9876543210fedcba9876543210fedcba98",
            "{label}: {row}"
        );
        assert_eq!(row["mergedAt"], "2024-01-03T00:00:00Z", "{label}: {row}");
        assert_eq!(row["baseBranch"], "main", "{label}: {row}");
        assert_eq!(row["labels"], json!(["refactor"]), "{label}: {row}");
    }
}

/// A file page pins the head it read (`headSha` on `next.nextFilePage`);
/// replayed after the head moved, the page is a stale snapshot whose
/// restart is the first page without the pin, never a mix of two heads.
#[tokio::test]
async fn inventory_cursor_replay_returns_the_same_pinned_file_page() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 450).await;
    mount_file_batches(
        &server,
        (1..=5)
            .map(|n| numbered(n, if n == 5 { 50 } else { 100 }))
            .collect(),
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let first = support::run(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,"sections":["files"]}),
    )
    .await;
    let next = first.data()["next"]["nextFilePage"]["input"].clone();
    assert!(next["cursor"].is_string());
    let second = support::run(&runtime, "ghGetHistoryItem", next.clone()).await;
    let replay = support::run(&runtime, "ghGetHistoryItem", next).await;
    assert_eq!(second.data()["pullRequests"][0]["headSha"], SHA);
    let paths = inventory_paths(&second.data()["pullRequests"][0]["files"]);
    assert!(!paths.is_empty());
    assert_eq!(
        paths,
        inventory_paths(&replay.data()["pullRequests"][0]["files"])
    );
    let first_paths = inventory_paths(&first.data()["pullRequests"][0]["files"]);
    assert!(paths.iter().all(|path| !first_paths.contains(path)));
    runtime.close().await;
}

/// The changed-file paths a compact inventory page lists, in order:
/// `M +1 -1 path` rows and `{"dir/": [rows]}` groups.
fn inventory_paths(files: &Value) -> Vec<String> {
    let name = |row: &Value| {
        row.as_str()
            .and_then(|row| row.rsplit(' ').next())
            .map(str::to_owned)
    };
    files
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| match row {
            Value::Object(group) => group
                .iter()
                .flat_map(|(dir, rows)| {
                    rows.as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(name)
                        .map(|file| format!("{dir}{file}"))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
            row => name(row).into_iter().collect(),
        })
        .collect()
}

/// A budget-sized file page pins its grid: `next.nextFilePage`, copied
/// unchanged, continues the same file list whether it runs alone or batched
/// with other rows (a different budget share), so every walk lists each
/// changed file exactly once.
#[tokio::test]
async fn file_page_continuations_continue_the_same_grid_in_any_batch() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 450).await;
    mount_file_batches(
        &server,
        (1..=5)
            .map(|batch| numbered(batch, if batch == 5 { 50 } else { 100 }))
            .collect(),
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let files_row = json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,
        "sections":["files"]});
    let body_row = json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,
        "sections":["body"]});
    // One call: the files row with or without a batched body row; returns
    // the files row's paths and its copied-unchanged next file page.
    for first_batched in [true, false] {
        let queries = if first_batched {
            json!([files_row, body_row])
        } else {
            json!([files_row])
        };
        let mut input = json!({"queries":queries});
        let mut seen = Vec::new();
        for hop in 0..100 {
            let outcome = runtime
                .run(
                    format!("grid-{first_batched}-{hop}"),
                    "ghGetHistoryItem".into(),
                    input,
                )
                .await
                .expect("file page");
            let content = &outcome.structured_content;
            let data = &content["results"][0]["data"];
            seen.extend(inventory_paths(&data["pullRequests"][0]["files"]));
            let next = content
                .pointer("/responsePagination/next")
                .or_else(|| data.pointer("/next/nextFilePage"));
            let Some(next) = next else { break };
            assert!(next["input"]["cursor"].is_string(), "{next}");
            input = next["input"].clone();
        }
        assert_eq!(seen.len(), 450);
        assert_eq!(
            seen.iter().collect::<std::collections::BTreeSet<_>>().len(),
            450
        );
    }
    runtime.close().await;
}
