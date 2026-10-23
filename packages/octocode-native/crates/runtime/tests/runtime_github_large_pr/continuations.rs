//! Pull request continuations: identity-only headers, pinned heads, merge state, verbatim views and file-page grids.

use super::{
    SHA, mount_file_batches, mount_pr, numbered, pr, raw_patch, rest_file, run, run_status,
};
use crate::support::{self, Run as _};
use serde_json::{Value, json};
use std::time::Duration;
use support::Workspace;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn pr_continuation_reads_carry_only_the_identity_header() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    mount_file_batches(
        &server,
        vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)],
        Duration::ZERO,
    )
    .await;
    // Page one carries the identity header and the menu; a later page
    // re-proves identity only (the summary read owns the rest of the header):
    // number, title, author, state and the commits code is read at.
    let query = json!({"sections":["files"], "pageSize": 100, "extras": ["hints"]});
    let follow = json!({"sections":["files"], "pageSize": 100, "page": 2, "extras": ["hints"]});
    let first = run(&server, query).await;
    let first_row = &first["pullRequests"][0];
    assert_eq!(first_row["headSha"], SHA, "{first_row}");
    assert_eq!(first_row["title"], "Large refactor", "{first_row}");
    assert!(first_row.get("bodyPreview").is_none(), "{first_row}");
    assert!(first_row.get("hints").is_none(), "{first_row}");
    assert!(first.get("hints").is_some(), "{first}");
    // A literal search of every patch needs the caller's literal: it is
    // never offered as an executable placeholder.
    assert!(
        crate::support::checks::hint(&first, "findInPatches").is_null(),
        "{first}"
    );

    let data = run(&server, follow).await;
    let row = &data["pullRequests"][0];
    for kept in [
        "number",
        "title",
        "author",
        "state",
        "headSha",
        "mergeCommitSha",
    ] {
        assert!(row.get(kept).is_some(), "{kept} missing: {row}");
    }
    for dropped in [
        "changedFilesCount",
        "mergedAt",
        "baseBranch",
        "closedAt",
        "labels",
        "headBranch",
        "updatedAt",
        "commentsCount",
        "additions",
        "deletions",
        "bodyPreview",
        "next",
    ] {
        assert!(row.get(dropped).is_none(), "{dropped} repeated: {row}");
    }
    // 100 files in one directory: one group of 100 compact rows.
    let group = &row["files"][0]["src/b2/"];
    assert_eq!(group.as_array().map(Vec::len), Some(100), "{row}");
    assert!(data["next"]["nextFilePage"].is_object(), "{data}");

    // The details extra keeps the full header.
    let details = json!({"sections":["files"], "pageSize": 100, "page": 2, "extras": ["details"]});
    let details = run(&server, details).await;
    assert_eq!(details["pullRequests"][0]["labels"], json!(["refactor"]));
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
        json!({"sections":["patches"],"include":["src/big.rs"],
               "extras": []}),
    )
    .await;
    let row = &data["pullRequests"][0];
    let file = &row["files"][0];
    assert_eq!(file["patchPagination"]["hasMore"], true, "{row}");
    let pages = &row["contentPagination"];
    // The finished single-page file list and the cursor copy add nothing.
    assert!(pages.get("files").is_none(), "{pages}");
    assert!(pages["patches"].get("offset").is_none(), "{pages}");
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
    let data = run(
        &server,
        json!({"sections":["body","patches"], "extras": ["hints"]}),
    )
    .await;
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
    assert!(
        crate::support::checks::hint(&data, "readUntrimmed").is_null(),
        "{data}"
    );
    assert!(
        crate::support::checks::hint(&data, "readRawBody").is_null(),
        "{data}"
    );
    assert!(
        !crate::support::checks::hint(&data, "readAtMerge").is_null(),
        "{data}"
    );
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
    for (label, query, full_header) in [
        ("summary", json!({"extras": []}), true),
        (
            "body+inventory",
            json!({"sections":["body","files"], "extras": []}),
            false,
        ),
        (
            "inventory",
            json!({"sections":["files"], "extras": []}),
            false,
        ),
        (
            "patches",
            json!({"sections":["patches"],"include":["src/hit.rs"],
                   "extras": []}),
            false,
        ),
        (
            "matchString",
            json!({"matchString": "needle", "extras": []}),
            true,
        ),
        (
            "later page",
            json!({"sections":["files"], "pageSize": 100, "page": 2,
                   "extras": []}),
            false,
        ),
    ] {
        let data = run(&server, query).await;
        let row = &data["pullRequests"][0];
        assert_eq!(row["state"], "merged", "{label}: {row}");
        assert_eq!(
            row["mergeCommitSha"], "fedcba9876543210fedcba9876543210fedcba98",
            "{label}: {row}"
        );
        // The merge time is the close time; it is not stated twice.
        assert!(row.get("closedAt").is_none(), "{label}: {row}");
        if full_header {
            assert_eq!(row["mergedAt"], "2024-01-03T00:00:00Z", "{label}: {row}");
            assert_eq!(row["baseBranch"], "main", "{label}: {row}");
            assert_eq!(row["labels"], json!(["refactor"]), "{label}: {row}");
        } else {
            for dropped in ["mergedAt", "baseBranch", "labels"] {
                assert!(row.get(dropped).is_none(), "{label}: {dropped}: {row}");
            }
        }
    }
}

/// A file page pins the head it read (`headSha` on `next.nextFilePage`);
/// replayed after the head moved, the page is a stale snapshot whose
/// restart is the first page without the pin, never a mix of two heads.
#[tokio::test]
async fn pull_request_pages_pin_the_head_and_a_moved_head_restarts() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    mount_file_batches(
        &server,
        vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)],
        Duration::ZERO,
    )
    .await;
    let first = run(
        &server,
        json!({"sections":["files"], "pageSize": 100, "extras": []}),
    )
    .await;
    let page = &first["next"]["nextFilePage"]["query"]["queries"][0];
    assert_eq!(page["headSha"], SHA, "{first}");
    let mut moved = page.clone();
    moved["headSha"] = json!("abcdef0123456789abcdef0123456789abcdef01");
    for field in ["operation", "owner", "repo", "number"] {
        moved.as_object_mut().expect("row").remove(field);
    }
    moved["extras"] = json!([]);
    let (status, stale) = run_status(&server, moved).await;
    assert_eq!(status, "error", "{stale}");
    assert_eq!(stale["errorCode"], "staleSnapshot", "{stale}");
    let restart = &stale["next"]["restart"]["query"]["queries"][0];
    assert!(restart.get("headSha").is_none(), "{stale}");
    assert!(restart.get("page").is_none(), "{stale}");
    // An abbreviated pin of the same head reads the page, not a false
    // stale snapshot.
    let mut short = page.clone();
    short["headSha"] = json!(&SHA[..7]);
    for field in ["operation", "owner", "repo", "number"] {
        short.as_object_mut().expect("row").remove(field);
    }
    short["extras"] = json!([]);
    let (status, read) = run_status(&server, short).await;
    assert_ne!(status, "error", "{read}");
    assert!(read.get("errorCode").is_none(), "{read}");
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
        "mainGoal":"g","reasoning":"r","sections":["files"]});
    let body_row = json!({"operation":"pullRequest","owner":"a","repo":"b","number":9,
        "mainGoal":"g","reasoning":"r","sections":["body"]});
    // One call: the files row with or without a batched body row; returns
    // the files row's paths and its copied-unchanged next file page.
    let calls = std::sync::atomic::AtomicUsize::new(0);
    let call = |row: Value, batched: bool| {
        let runtime = &runtime;
        let id = calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let body_row = body_row.clone();
        async move {
            let queries = if batched {
                json!([row, body_row])
            } else {
                json!([row])
            };
            let outcome = runtime
                .run(
                    format!("grid-{id}"),
                    "ghGetHistoryItem".into(),
                    json!({ "queries": queries }),
                )
                .await
                .expect("file page");
            let content = outcome.structured_content;
            assert!(content.get("responsePagination").is_none(), "{content}");
            let data = &content["results"][0]["data"];
            let paths = inventory_paths(&data["pullRequests"][0]["files"]);
            let next = data.pointer("/next/nextFilePage/query/queries/0").cloned();
            (paths, next)
        }
    };
    for first_batched in [true, false] {
        let mut seen = Vec::new();
        let (paths, mut next) = call(files_row.clone(), first_batched).await;
        let first_page = paths.len();
        seen.extend(paths);
        // Every later page runs in the other batch shape.
        while let Some(row) = next {
            let (paths, after) = call(row, !first_batched).await;
            assert!(!paths.is_empty(), "an empty page mid-walk");
            seen.extend(paths);
            next = after;
        }
        let unique = seen.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), seen.len(), "a file repeated: {first_page}");
        assert_eq!(seen.len(), 450, "first page {first_page}: {seen:?}");
    }
    runtime.close().await;
}
