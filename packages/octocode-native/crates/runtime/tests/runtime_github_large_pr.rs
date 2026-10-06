// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::panic)]
//! Large pull-request review: changed-file inventory, patch availability,
//! provider file-list cap, continuation payload size, and scan latency.

use crate::support;

use serde_json::{Value, json};
use std::time::{Duration, Instant};
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// No page and no lead: `next` is absent and `hints` holds at most prose.
fn no_continuation(value: &serde_json::Value) -> bool {
    value.get("next").is_none()
        && value
            .get("hints")
            .and_then(serde_json::Value::as_object)
            .is_none_or(|hints| hints.keys().all(|key| key == "text"))
}

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn pr(changed_files: usize) -> Value {
    json!({
        "number": 9, "title": "Large refactor", "state": "closed",
        "merged_at": "2024-01-03T00:00:00Z",
        "merge_commit_sha": "fedcba9876543210fedcba9876543210fedcba98",
        "draft": false, "body": "x".repeat(900),
        "user": {"login": "alice"}, "labels": [{"name": "refactor"}],
        "head": {"sha": SHA, "ref": "feat"}, "base": {"ref": "main"},
        "created_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z",
        "closed_at": "2024-01-03T00:00:00Z",
        "comments": 4, "review_comments": 2,
        "changed_files": changed_files, "additions": 5000, "deletions": 4000
    })
}

/// A numbered patch view without its gutter (`N\t`: new-side numbers on
/// kept and added lines, old-side on removed ones): the raw patch.
fn raw_patch(view: &str) -> String {
    view.split_inclusive('\n')
        .map(|line| match line.split_once('\t') {
            Some((gutter, text))
                if !line.starts_with("@@") && gutter.bytes().all(|b| b.is_ascii_digit()) =>
            {
                text
            }
            _ => line,
        })
        .collect()
}

fn rest_file(name: &str, patch: Option<&str>, additions: u64, deletions: u64) -> Value {
    let mut file = json!({
        "sha": "1111111111111111111111111111111111111111", "filename": name,
        "status": "modified", "additions": additions, "deletions": deletions,
        "changes": additions + deletions
    });
    if let Some(patch) = patch {
        file["patch"] = json!(patch);
    }
    file
}

async fn mount_pr(server: &MockServer, changed_files: usize) {
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pr(changed_files)))
        .mount(server)
        .await;
}

/// Mount `batches` provider file pages; every page but the last links `next`.
async fn mount_file_batches(server: &MockServer, batches: Vec<Vec<Value>>, delay: Duration) {
    let count = batches.len();
    for (index, files) in batches.into_iter().enumerate() {
        let page = index + 1;
        let mut response = ResponseTemplate::new(200)
            .set_delay(delay)
            .set_body_json(Value::Array(files));
        if page < count {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/pulls/9/files?per_page=100&page={}>; rel=\"next\"",
                    server.uri(),
                    page + 1
                )
                .as_str(),
            );
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/pulls/9/files"))
            .and(query_param("page", page.to_string()))
            .respond_with(response)
            .mount(server)
            .await;
    }
}

fn numbered(batch: usize, size: usize) -> Vec<Value> {
    (0..size)
        .map(|i| {
            rest_file(
                &format!("src/b{batch}/f{i}.rs"),
                Some("@@ -1 +1 @@\n-a\n+b"),
                1,
                1,
            )
        })
        .collect()
}

async fn run(server: &MockServer, query: Value) -> Value {
    let (status, data) = run_status(server, query).await;
    assert_eq!(status, "success", "{data}");
    data
}

async fn run_status(server: &MockServer, query: Value) -> (String, Value) {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let mut query = query;
    query["operation"] = json!("pullRequest");
    query["owner"] = json!("a");
    query["repo"] = json!("b");
    query["number"] = json!(9);
    let outcome = match call(&runtime, "ghGetHistoryItem", query).await {
        Ok(outcome) => outcome,
        Err(error) => {
            let seen = server
                .received_requests()
                .await
                .unwrap_or_default()
                .iter()
                .map(|r| r.url.to_string())
                .collect::<Vec<_>>();
            panic!("PR read failed: {error:?}; requests: {seen:?}");
        }
    };
    let status = row_status(&outcome).to_owned();
    let data = row_data(&outcome).clone();
    runtime.close().await;
    (status, data)
}

#[tokio::test]
async fn large_pr_inventory_flags_patchless_files_and_keeps_rename_origin() {
    let server = MockServer::start().await;
    mount_pr(&server, 5).await;
    let mut renamed = rest_file("src/new_name.rs", None, 0, 0);
    renamed["status"] = json!("renamed");
    renamed["previous_filename"] = json!("src/old_name.rs");
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/ok.rs", Some("@@ -1 +1 @@\n-a\n+b"), 1, 1),
            renamed,
            rest_file("src/checker.ts", None, 39_550, 39_342),
            rest_file("assets/logo.png", None, 0, 0),
            rest_file("src/omitted.ts", None, 0, 0),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(&server, json!({"sections":["files"], "debug": false})).await;
    // Compact rows: consecutive files of one directory share a group.
    assert_eq!(
        data["pullRequests"][0]["files"],
        json!([
            {"src/": [
                "M +1 -1 ok.rs",
                "R +0 -0 new_name.rs <- src/old_name.rs",
                "M +39550 -39342 !tooLarge checker.ts"
            ]},
            "M +0 -0 !binary assets/logo.png",
            "M +0 -0 !omitted src/omitted.ts"
        ]),
        "{data}"
    );
}

#[tokio::test]
async fn pr_inventory_carries_the_identity_header_and_its_own_next_steps_only() {
    let server = MockServer::start().await;
    mount_pr(&server, 250).await;
    mount_file_batches(
        &server,
        vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)],
        Duration::ZERO,
    )
    .await;
    let data = run(&server, json!({"sections":["files"], "debug": false})).await;
    let row = &data["pullRequests"][0];
    for kept in [
        "number",
        "title",
        "state",
        "sourceSha",
        "mergeCommitSha",
        "additions",
        "deletions",
    ] {
        assert!(row.get(kept).is_some(), "{kept} missing: {row}");
    }
    // A merged PR's close time is its merge time.
    for dropped in ["bodyPreview", "updatedAt", "sourceBranch", "closedAt"] {
        assert!(row.get(dropped).is_none(), "{dropped} kept: {row}");
    }
    // Merge state and labels ride the first page of every read.
    for kept in ["mergedAt", "targetBranch", "labels"] {
        assert!(row.get(kept).is_some(), "{kept} missing: {row}");
    }
    // The next steps are the response's leads, not a list nested in the row.
    assert!(row.get("hints").is_none(), "{row}");
    let menu = data["hints"].as_object().expect("hints");
    // 250 files: no every-patch read, no placeholder literal search (only
    // the caller knows the literal), and no merge-commit read beside the
    // row's mergeCommitSha.
    assert_eq!(
        menu.keys().collect::<Vec<_>>(),
        ["readSelectedPatches"],
        "{row}"
    );
    // The review read names up to one patch page of source files.
    let review = &menu["readSelectedPatches"]["query"]["queries"][0]["include"];
    assert_eq!(review.as_array().map(Vec::len), Some(30), "{row}");
    // An omitted pageSize reads the whole 250-file inventory in one page.
    let page = row.get("contentPagination").cloned().unwrap_or_default();
    assert!(page.get("files").is_none(), "{page}");
    let groups = row["files"].as_array().expect("files");
    let rows = groups
        .iter()
        .map(|group| {
            group
                .as_object()
                .and_then(|g| g.values().next())
                .and_then(Value::as_array)
                .map_or(1, Vec::len)
        })
        .sum::<usize>();
    assert_eq!(rows, 250, "{row}");
}

/// The review pick is every source file, most changed first (not the
/// changeset, not the tests), labelled as a ranking guess; a small PR keeps
/// the every-patch read beside it.
#[tokio::test]
async fn pr_inventory_picks_the_largest_source_patch_and_keeps_all_patches() {
    let server = MockServer::start().await;
    mount_pr(&server, 5).await;
    let hunk = Some("@@ -1 +1 @@\n-a\n+b");
    mount_file_batches(
        &server,
        vec![vec![
            rest_file(".changeset/sse-keepalive.md", hunk, 8, 0),
            rest_file("src/server/sseKeepAlive.ts", hunk, 15, 0),
            rest_file("src/server/webStandardStreamableHttp.ts", hunk, 241, 94),
            rest_file("test/server/sseKeepAlive.test.ts", hunk, 29, 0),
            rest_file("test/server/streamableHttp.test.ts", hunk, 879, 3),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(&server, json!({"sections":["files"], "debug": false})).await;
    let menu = &data["hints"];
    let selected = &menu["readSelectedPatches"];
    assert_eq!(
        selected["query"]["queries"][0]["include"],
        json!([
            "src/server/webStandardStreamableHttp.ts",
            "src/server/sseKeepAlive.ts"
        ]),
        "{menu}"
    );
    assert_eq!(
        menu["readPatches"]["query"]["queries"][0]["sections"],
        json!(["patches"]),
        "{menu}"
    );
    assert!(data["pullRequests"][0].get("hints").is_none(), "{data}");
}

#[tokio::test]
async fn pr_file_filter_narrows_the_inventory_and_its_counts() {
    let server = MockServer::start().await;
    mount_pr(&server, 250).await;
    let mut batches = vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)];
    batches[1][7] = rest_file("docs/guide.md", Some("@@ -1 +1 @@\n-a\n+b"), 30, 2);
    batches[2][3]["status"] = json!("added");
    mount_file_batches(&server, batches, Duration::ZERO).await;
    let data = run(
        &server,
        json!({"sections":["files"], "debug": true,
               "include":["*.md","src/b3/"],"status":["added","modified"],"minChanges":3}),
    )
    .await;
    let row = &data["pullRequests"][0];
    assert_eq!(row["files"], json!(["M +30 -2 docs/guide.md"]), "{row}");
    assert_eq!(row["contentPagination"]["files"]["totalItems"], 1, "{row}");
    let data = run(
        &server,
        json!({"sections":["files"], "debug": false,
               "include":["src/b3/"],"status":["added"]}),
    )
    .await;
    assert_eq!(
        data["pullRequests"][0]["files"],
        json!(["A +1 -1 src/b3/f3.rs"]),
        "{data}"
    );
    assert!(
        no_continuation(&data["pullRequests"][0]),
        "filtered reads carry no menu: {data}"
    );
}

#[tokio::test]
async fn pure_rename_patch_is_empty_not_a_provider_omission() {
    let server = MockServer::start().await;
    mount_pr(&server, 2).await;
    let mut renamed = rest_file("src/new_name.rs", None, 0, 0);
    renamed["status"] = json!("renamed");
    renamed["previous_filename"] = json!("src/old_name.rs");
    mount_file_batches(
        &server,
        vec![vec![renamed, rest_file("src/checker.ts", None, 10, 2)]],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"sections":["patches"], "debug": false, "minify": "none"}),
    )
    .await;
    let files = &data["pullRequests"][0]["files"];
    assert_eq!(files[0]["path"], "src/new_name.rs", "{files}");
    assert_eq!(files[0]["previousPath"], "src/old_name.rs", "{files}");
    assert_eq!(files[0]["patch"], "", "{files}");
    assert!(files[0].get("patchUnavailable").is_none(), "{files}");
    assert_eq!(files[1]["patchUnavailable"], "tooLarge", "{files}");
}

#[tokio::test]
async fn pr_file_list_stopped_by_the_provider_cap_is_not_complete() {
    // GitHub lists at most 3000 files: the last listable page has no `next`
    // link although the PR reports more changed files.
    let server = MockServer::start().await;
    mount_pr(&server, 3500).await;
    mount_file_batches(&server, vec![numbered(1, 100)], Duration::ZERO).await;
    let data = run(
        &server,
        json!({"sections":["files"], "pageSize": 100, "debug": false}),
    )
    .await;
    let page = &data["pullRequests"][0]["contentPagination"]["files"];
    assert_ne!(page["countScope"], "complete", "{page}");
    assert_eq!(page["terminalLimit"], true, "{page}");
    assert_eq!(
        page["providerLimit"]["reason"], "providerFileListLimit",
        "{page}"
    );
    assert_eq!(page["providerLimit"]["listed"], 100, "{page}");
    assert_eq!(page["providerLimit"]["changedFilesCount"], 3500, "{page}");
}

#[tokio::test]
async fn pr_inventory_page_reports_the_provider_total() {
    let server = MockServer::start().await;
    mount_pr(&server, 250).await;
    mount_file_batches(
        &server,
        vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"sections":["files"], "pageSize": 100, "debug": false}),
    )
    .await;
    let page = &data["pullRequests"][0]["contentPagination"]["files"];
    assert_eq!(page["totalItems"], 250, "{page}");
    assert_eq!(page["totalPages"], 3, "{page}");
    assert_eq!(page["hasMore"], true, "{page}");
    assert_eq!(page["nextPage"], 2, "{page}");
}

#[tokio::test]
async fn pr_continuation_reads_carry_only_the_identity_header() {
    let server = MockServer::start().await;
    mount_pr(&server, 250).await;
    mount_file_batches(
        &server,
        vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)],
        Duration::ZERO,
    )
    .await;
    // Page one carries the header and menu; a later page re-proves identity only.
    let query = json!({"sections":["files"], "pageSize": 100, "debug": false});
    let follow = json!({"sections":["files"], "pageSize": 100, "filePage": 2, "debug": false});
    let first = run(&server, query).await;
    let first_row = &first["pullRequests"][0];
    assert_eq!(first_row["title"], "Large refactor", "{first_row}");
    assert!(first_row.get("hints").is_none(), "{first_row}");
    assert!(first.get("hints").is_some(), "{first}");
    // A literal search of every patch needs the caller's literal: it is
    // never offered as an executable placeholder.
    assert!(first["hints"].get("findInPatches").is_none(), "{first}");

    let data = run(&server, follow).await;
    let row = &data["pullRequests"][0];
    for kept in [
        "number",
        "title",
        "state",
        "sourceSha",
        "mergeCommitSha",
        "changedFilesCount",
    ] {
        assert!(row.get(kept).is_some(), "{kept} missing: {row}");
    }
    for kept in ["mergedAt", "targetBranch"] {
        assert!(row.get(kept).is_some(), "{kept} missing: {row}");
    }
    for dropped in [
        "closedAt",
        "labels",
        "sourceBranch",
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

    // debug keeps the full header.
    let debug = json!({"sections":["files"], "pageSize": 100, "filePage": 2, "debug": true});
    let debug = run(&server, debug).await;
    assert_eq!(debug["pullRequests"][0]["labels"], json!(["refactor"]));
}

#[tokio::test]
async fn patch_window_does_not_repeat_the_file_cursor_in_content_pagination() {
    let server = MockServer::start().await;
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
               "minify": "none", "debug": false}),
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

#[tokio::test]
async fn selected_patch_scan_reads_file_batches_concurrently() {
    // A selected late file must not cost one sequential round trip per
    // provider batch before it: six 300 ms batches read in parallel.
    let server = MockServer::start().await;
    mount_pr(&server, 600).await;
    let mut batches = (1..=6).map(|b| numbered(b, 100)).collect::<Vec<_>>();
    batches[5][99] = rest_file("src/late.rs", Some("@@ -1 +1 @@\n-a\n+late"), 1, 1);
    mount_file_batches(&server, batches, Duration::from_millis(300)).await;
    let started = Instant::now();
    let data = run(
        &server,
        json!({"sections":["patches"],"include":["src/late.rs"],
               "minify": "none", "debug": false}),
    )
    .await;
    let elapsed = started.elapsed();
    let file = &data["pullRequests"][0]["files"][0];
    assert_eq!(file["path"], "src/late.rs", "{data}");
    assert_eq!(file["patch"], "@@ -1 +1 @@\n1\t-a\n1\t+late");
    assert!(
        elapsed < Duration::from_millis(1_200),
        "sequential scan: {elapsed:?}"
    );
}

#[tokio::test]
async fn match_string_returns_matching_hunks_and_offers_the_whole_patch() {
    let server = MockServer::start().await;
    mount_pr(&server, 2).await;
    let body = (1..=200)
        .map(|n| format!(" line {n}\n"))
        .collect::<String>();
    let big = format!("@@ -1,401 +1,401 @@\n{body}-old esbuild\n+new esbuild\n{body}");
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/big.rs", Some(&big), 1, 1),
            rest_file("src/other.rs", Some("@@ -1 +1 @@\n-a\n+b"), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"sections":["patches"], "matchString": "esbuild",
               "minify": "none", "debug": false}),
    )
    .await;
    let files = &data["pullRequests"][0]["files"];
    assert_eq!(files.as_array().map(Vec::len), Some(1), "{data}");
    let patch = files[0]["patch"].as_str().expect("patch");
    // 10 context lines around each hit by default, the rest re-read whole.
    assert!(
        patch.starts_with("@@ -191,21 +191,21 @@\n191\t line 191\n"),
        "{patch}"
    );
    assert!(
        patch.contains("\n201\t-old esbuild\n201\t+new esbuild\n"),
        "{patch}"
    );
    assert!(patch.ends_with("211\t line 10\n"), "{patch}");
    assert!(patch.len() < 600, "{patch}");
    // The narrowed-view marker selects the continuation; rows omit it.
    assert!(files[0].get("fullPatchChars").is_none(), "{data}");
    // A patch read carries the identity it re-proves (number, state, the
    // head it read) and no follow-up menu; the metadata read names the PR.
    let row = &data["pullRequests"][0];
    assert!(no_continuation(row), "{row}");
    for dropped in [
        "mergeCommitSha",
        "additions",
        "deletions",
        "title",
        "author",
        "createdAt",
    ] {
        assert!(row.get(dropped).is_none(), "{dropped} kept: {row}");
    }
    for kept in ["number", "state", "sourceSha", "mergedAt", "targetBranch"] {
        assert!(row.get(kept).is_some(), "{kept} dropped: {row}");
    }
    // Without a patch row the read proves nothing about files: it keeps the
    // full identity.
    let (status, none) = run_status(
        &server,
        json!({"sections":["patches"], "matchString": "absent-term",
               "minify": "none", "debug": false}),
    )
    .await;
    assert_eq!(none["pullRequests"][0]["title"], "Large refactor", "{none}");
    // A matchString that hits no patch line is an empty read that says so,
    // not a summary that looks like an ignored filter.
    assert_eq!(status, "empty", "{none}");
    let hint = none["hints"]["text"][0].as_str().unwrap_or("");
    assert!(hint.contains("matchString"), "{none}");
    let only_hits = run(
        &server,
        json!({"sections":["patches"], "matchString": "esbuild",
               "contextLines": 0, "minify": "none", "debug": false}),
    )
    .await;
    assert_eq!(
        only_hits["pullRequests"][0]["files"][0]["patch"],
        "@@ -201,1 +201,1 @@\n201\t-old esbuild\n201\t+new esbuild\n"
    );
    // An explicit contextLines still narrows the patch: the whole patch
    // stays reachable.
    assert_eq!(
        only_hits["next"]["readFullPatches"]["query"]["queries"][0]["include"],
        json!(["src/big.rs"]),
        "{only_hits}"
    );
    assert_eq!(
        data["next"]["readFullPatches"]["query"]["queries"][0]["include"],
        json!(["src/big.rs"]),
        "{data}"
    );
    // Following the read returns the whole patch, numbered.
    let mut full = data["next"]["readFullPatches"]["query"]["queries"][0].clone();
    for key in ["operation", "owner", "repo", "number"] {
        full.as_object_mut().map(|q| q.remove(key));
    }
    let full = run(&server, full).await;
    let whole = full["pullRequests"][0]["files"][0]["patch"]
        .as_str()
        .unwrap_or("");
    assert_eq!(raw_patch(whole), big, "{full}");
    assert!(whole.contains("\n201\t+new esbuild\n"), "{full}");
}

/// The default minified PR view drops markdown noise (HTML comments,
/// badges) from bodies, comments and reviews, and says so with a lossless
/// raw re-read; code hunks are never minified: every context line arrives,
/// numbered on the new side.
#[tokio::test]
async fn minified_pr_views_carry_lossless_raw_reads() {
    let server = MockServer::start().await;
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
        json!({"sections":["body","patches"], "debug": false}),
    )
    .await;
    let row = &data["pullRequests"][0];
    assert_eq!(row["bodyView"], "minified", "{data}");
    assert!(
        !row["body"].as_str().unwrap_or("").contains("checklist"),
        "{row}"
    );
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
    assert!(data.pointer("/hints/readUntrimmed").is_none(), "{data}");
    // The merged-source check keeps its slot beside the raw re-read.
    assert!(data.pointer("/hints/readAtMerge").is_some(), "{data}");
    let raw_read = &data["hints"]["readRawBody"]["query"]["queries"][0];
    assert_eq!(raw_read["minify"], "none", "{data}");
    let mut follow = raw_read.clone();
    for key in ["operation", "owner", "repo", "number"] {
        follow.as_object_mut().map(|q| q.remove(key));
    }
    let body = run(&server, follow).await;
    assert_eq!(body["pullRequests"][0]["body"], raw_body, "{body}");
    assert!(body["pullRequests"][0].get("bodyView").is_none(), "{body}");
    // A raw read is never flagged.
    let plain = run(
        &server,
        json!({"sections":["body","patches"], "minify": "none"}),
    )
    .await;
    assert!(plain.pointer("/hints/readRawBody").is_none(), "{plain}");
}

/// D1: `matchString` searches patches, so a pull-request read with a
/// literal and no content selection searches every patch (within
/// `include`) instead of silently returning only the summary.
#[tokio::test]
async fn match_string_without_content_searches_the_filtered_patches() {
    let server = MockServer::start().await;
    mount_pr(&server, 3).await;
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/mod/a.ts", Some("@@ -1 +1 @@\n-x\n+import y"), 1, 1),
            rest_file("src/mod/b.ts", Some("@@ -1 +1 @@\n-x\n+z"), 1, 1),
            rest_file("lib/c.ts", Some("@@ -1 +1 @@\n-x\n+import w"), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"matchString": "import", "contextLines": 0,
               "include":["src/mod/**"], "debug": false}),
    )
    .await;
    let files = data["pullRequests"][0]["files"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let paths = files
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec!["src/mod/a.ts"], "{data}");
    assert!(
        files[0]["patch"]
            .as_str()
            .unwrap_or("")
            .contains("+import y"),
        "{data}"
    );
}

/// D2: a patch search never implies absence for files GitHub sent without a
/// patch: it lists them, marks the row partial, and offers a source read at
/// the PR head.
#[tokio::test]
async fn match_string_lists_the_patchless_files_it_could_not_search() {
    let server = MockServer::start().await;
    mount_pr(&server, 4).await;
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/hit.ts", Some("@@ -1 +1 @@\n-x\n+import y"), 1, 1),
            rest_file("src/module.ts", None, 900, 12),
            rest_file("src/system.ts", None, 0, 0),
            rest_file("assets/logo.png", None, 0, 0),
        ]],
        Duration::ZERO,
    )
    .await;
    let data = run(
        &server,
        json!({"sections":["patches"], "matchString": "import",
               "debug": false}),
    )
    .await;
    assert_eq!(data["isPartial"], true, "{data}");
    let reasons = data["partialReasons"].to_string();
    assert!(reasons.contains("patchUnavailable"), "{data}");
    let unsearched = data["pullRequests"][0]["unsearchedFiles"].to_string();
    assert!(unsearched.contains("src/module.ts"), "{data}");
    assert!(unsearched.contains("src/system.ts"), "{data}");
    assert!(
        !unsearched.contains("logo.png"),
        "binary is not text: {data}"
    );
    let read = &data["next"]["searchUnpatchedFile"];
    assert_eq!(read["tool"], "ghGetFileContent", "{data}");
    assert_eq!(read["query"]["queries"][0]["ref"], SHA, "{data}");
    assert_eq!(
        read["query"]["queries"][0]["matchString"], "import",
        "{data}"
    );
    assert_eq!(
        read["query"]["queries"][0]["path"], "src/module.ts",
        "{data}"
    );
}

/// A merged PR's merge state rides every PR row, whatever the
/// content selection: summary, body + inventory, inventory, patch read and a
/// later file page all carry `mergedAt`/`closedAt`/`targetBranch`; the first
/// page also keeps the labels.
#[tokio::test]
async fn merged_pr_rows_keep_merge_state_on_every_read() {
    let server = MockServer::start().await;
    mount_pr(&server, 250).await;
    let mut batches = vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)];
    batches[0][0] = rest_file("src/hit.rs", Some("@@ -1 +1 @@\n-a\n+needle"), 1, 1);
    mount_file_batches(&server, batches, Duration::ZERO).await;
    for (label, query, first_page) in [
        ("summary", json!({"debug": false}), true),
        (
            "body+inventory",
            json!({"sections":["body","files"], "debug": false}),
            true,
        ),
        (
            "inventory",
            json!({"sections":["files"], "debug": false}),
            true,
        ),
        (
            "patches",
            json!({"sections":["patches"],"include":["src/hit.rs"],
                   "minify": "none", "debug": false}),
            true,
        ),
        (
            "matchString",
            json!({"matchString": "needle", "debug": false}),
            true,
        ),
        (
            "later page",
            json!({"sections":["files"], "pageSize": 100, "filePage": 2,
                   "debug": false}),
            false,
        ),
    ] {
        let data = run(&server, query).await;
        let row = &data["pullRequests"][0];
        assert_eq!(row["mergedAt"], "2024-01-03T00:00:00Z", "{label}: {row}");
        // The merge time is the close time; it is not stated twice.
        assert!(row.get("closedAt").is_none(), "{label}: {row}");
        assert_eq!(row["targetBranch"], "main", "{label}: {row}");
        assert_eq!(row["state"], "merged", "{label}: {row}");
        if first_page {
            assert_eq!(row["labels"], json!(["refactor"]), "{label}: {row}");
        }
    }
}

/// Rows of one call share one patch budget, so two large patch
/// reads fit one response page instead of spilling into response
/// pagination; each row keeps its own exact `continuePatch`.
#[tokio::test]
async fn patch_rows_in_one_call_share_one_budget() {
    let server = MockServer::start().await;
    mount_pr(&server, 2).await;
    let big = |tag: &str| {
        format!(
            "@@ -1,9000 +1,9000 @@\n{}",
            format!("+{tag} line\n").repeat(3_000)
        )
    };
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("pyproject.toml", Some(&big("a")), 3_000, 0),
            rest_file("src/_runtime.py", Some(&big("b")), 3_000, 0),
        ]],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let row = |file: &str| {
        json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 9,
               "mainGoal": "g", "reasoning": "r", "minify": "none",
               "sections": ["patches"], "include": [file]})
    };
    let outcome = runtime
        .execute(
            "test-1".into(),
            "ghGetHistoryItem".into(),
            json!({"queries": [row("pyproject.toml"), row("src/_runtime.py")]}),
        )
        .await
        .expect("two patch rows");
    let content = &outcome.structured_content;
    assert!(
        content.get("responsePagination").is_none(),
        "two rows overflowed the page: {}",
        content
            .get("responsePagination")
            .cloned()
            .unwrap_or_default()
    );
    for index in 0..2 {
        let data = &content["results"][index]["data"];
        let file = &data["pullRequests"][0]["files"][0];
        let taken = file["patchPagination"]["length"].as_u64().unwrap_or(0);
        // Half the default window each (one budget), not a whole window each.
        assert!((4_000..=20_000).contains(&taken), "row {index}: {taken}");
        assert!(
            data["next"]["continuePatch"].is_object(),
            "row {index}: {data}"
        );
    }
    runtime.close().await;
}

/// A whole-PR patch walk that asks for a larger response page gets patch
/// windows sized to it, and the windows still tile every patch exactly (no
/// gap, no repeat). Every hop has the page budget of the first window and
/// runs unchanged.
#[tokio::test]
async fn explicit_response_page_sizes_patch_walk_windows() {
    let server = MockServer::start().await;
    mount_pr(&server, 2).await;
    let patch = |tag: &str| {
        format!(
            "@@ -1,9000 +1,9000 @@\n{}",
            (0..6_000)
                .map(|i| format!("+{tag} line {i}\n"))
                .collect::<String>()
        )
    };
    let patches = [patch("a"), patch("b")];
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/a.rs", Some(&patches[0]), 6_000, 0),
            rest_file("src/b.rs", Some(&patches[1]), 6_000, 0),
        ]],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH", "20000".into()),
    ]);
    let walk = |page: Option<u64>, char_length: Option<u64>| {
        let runtime = &runtime;
        async move {
            let mut first = json!({"operation": "pullRequest", "owner": "a", "repo": "b",
                "number": 9, "mainGoal": "g", "reasoning": "r", "minify": "none",
                "sections": ["patches"]});
            if let Some(length) = char_length {
                first["length"] = json!(length);
            }
            let with_page = |row: Value| {
                let mut envelope = json!({"queries": [row]});
                if let Some(page) = page {
                    envelope["responseLength"] = json!(page);
                }
                envelope
            };
            let mut hops = std::collections::VecDeque::from([with_page(first)]);
            let mut read = std::collections::BTreeMap::<String, String>::new();
            let mut calls = 0;
            while let Some(envelope) = hops.pop_front() {
                calls += 1;
                assert!(calls <= 60, "walk did not finish");
                let outcome = runtime
                    .execute(format!("walk-{calls}"), "ghGetHistoryItem".into(), envelope)
                    .await
                    .expect("patch window");
                let content = &outcome.structured_content;
                // An explicit page reports its (single) response page.
                assert_ne!(
                    content["responsePagination"]["hasMore"], true,
                    "a patch window overflowed its page: {content}"
                );
                let data = &content["results"][0]["data"];
                // A length above the page is clamped and says so on the
                // call that asked; the windows below still tile every patch.
                let clamped = data["warnings"].as_array().is_some_and(|text| {
                    text.iter()
                        .any(|t| t.as_str().is_some_and(|t| t.starts_with("length")))
                });
                assert_eq!(
                    clamped,
                    char_length.is_some() && calls == 1,
                    "call {calls}: {data}"
                );
                for file in data["pullRequests"][0]["files"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let path = file["path"].as_str().expect("path").to_owned();
                    let text = read.entry(path).or_default();
                    let offset = file["patchPagination"]["offset"].as_u64().unwrap_or(0);
                    assert_eq!(offset as usize, text.chars().count(), "{file}");
                    text.push_str(file["patch"].as_str().unwrap_or(""));
                }
                // One check at the merge commit, on the first window only.
                assert_eq!(
                    data["hints"].get("readAtMerge").is_some(),
                    calls == 1,
                    "call {calls}: {data}"
                );
                // Each hop is a complete one-row input: the same file page
                // at the stream cursor, with the page budget of the first
                // window.
                if let Some(next) = data["next"]["continuePatch"]["query"]["queries"][0].as_object()
                {
                    assert_eq!(
                        data["next"]["continuePatch"]["query"]["queries"]
                            .as_array()
                            .map(Vec::len),
                        Some(1),
                        "{data}"
                    );
                    assert_eq!(next.get("sections"), Some(&json!(["patches"])), "{data}");
                    assert!(next.get("include").is_none(), "{data}");
                    // A caller's oversized length rides the hop clamped to
                    // the window it got, so the clamp is said once.
                    match char_length {
                        None => assert!(next.get("length").is_none(), "{data}"),
                        Some(asked) => assert!(
                            next["length"].as_u64().is_some_and(|length| length < asked),
                            "{data}"
                        ),
                    }
                    hops.push_back(with_page(Value::Object(next.clone())));
                }
            }
            (calls, read)
        }
    };
    let (default_calls, default_read) = walk(None, None).await;
    let (explicit_calls, explicit_read) = walk(Some(50_000), None).await;
    let (_, oversized_read) = walk(None, Some(80_000)).await;
    for (path, patch) in ["src/a.rs", "src/b.rs"].iter().zip(&patches) {
        for read in [&default_read, &explicit_read, &oversized_read] {
            assert_eq!(
                read.get(*path).map(|view| raw_patch(view)).as_ref(),
                Some(patch),
                "{path}"
            );
        }
    }
    // A larger explicit response page carries larger windows: fewer hops.
    assert!(
        explicit_calls < default_calls,
        "explicit {explicit_calls} vs default {default_calls} calls"
    );
    runtime.close().await;
}

/// A literal search covers every changed file in one call (not 30
/// files a page) and returns hit lines only, so its bytes track the hits.
#[tokio::test]
async fn match_string_covers_every_file_page_in_one_call() {
    let server = MockServer::start().await;
    mount_pr(&server, 136).await;
    let hit = |i: usize| {
        rest_file(
            &format!("tests/t{i}.rs"),
            Some(&format!(
                "@@ -1,5 +1,5 @@\n a\n b\n-#![cfg(not(miri))]\n+#![cfg(not(miri))] // {i}\n c\n d\n"
            )),
            1,
            1,
        )
    };
    let mut first = numbered(1, 100);
    for (slot, i) in (0..18).map(|i| (i * 5, i)) {
        first[slot] = hit(i);
    }
    let second = (18..36).map(hit).collect::<Vec<_>>();
    mount_file_batches(&server, vec![first, second], Duration::ZERO).await;
    let data = run(&server, json!({"matchString": "miri", "debug": false})).await;
    let files = data["pullRequests"][0]["files"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(files.len(), 36, "{data}");
    for file in &files {
        let patch = file["patch"].as_str().unwrap_or("");
        assert!(!patch.contains("\n a\n"), "context leaked: {patch}");
        assert!(
            file.get("status").is_none() && file["stat"] == "M +1 -1",
            "{file}"
        );
    }
    assert!(
        data.get("next")
            .and_then(|n| n.get("nextFilePage"))
            .is_none(),
        "{data}"
    );
}

/// An `include` scope narrows a body + patch read to the matching files
/// through the full runtime.
#[tokio::test]
async fn include_scopes_a_body_and_patch_read() {
    let server = MockServer::start().await;
    mount_pr(&server, 3).await;
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/a.rs", Some("@@ -1 +1 @@\n-a\n+needle"), 1, 1),
            rest_file("docs/b.md", Some("@@ -1 +1 @@\n-a\n+b"), 1, 1),
            rest_file("src/c.rs", Some("@@ -1 +1 @@\n-a\n+c"), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let flat = run(
        &server,
        json!({"sections":["body","patches"],
               "include":["src/**"], "minify": "none", "debug": false}),
    )
    .await;
    assert!(flat["pullRequests"][0].get("body").is_some(), "{flat}");
    let paths = flat["pullRequests"][0]["files"]
        .as_array()
        .map(|files| {
            files
                .iter()
                .filter_map(|f| f["path"].as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert_eq!(paths, ["src/a.rs", "src/c.rs"], "{flat}");
}

/// An `include` scope that matches no changed file says so (with a
/// wrong path and got a silent empty row).
#[tokio::test]
async fn files_scope_matching_nothing_is_an_empty_row_with_a_hint() {
    let server = MockServer::start().await;
    mount_pr(&server, 1).await;
    mount_file_batches(
        &server,
        vec![vec![rest_file(
            "fastapi/telemetry/_runtime.py",
            Some("@@ -1 +1 @@\n+a"),
            1,
            0,
        )]],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 9,
               "sections": ["patches"], "include": ["fastapi/_runtime.py"], "debug": false}),
    )
    .await
    .expect("read");
    let data = row_data(&outcome);
    assert!(
        data.get("errorCode").is_none(),
        "an empty row carries no errorCode: {data}"
    );
    assert!(
        data["hints"]["text"][0]
            .as_str()
            .unwrap_or("")
            .contains("include"),
        "{data}"
    );
    runtime.close().await;
}

/// cli/cli#13541: the first patch window holds only docs, a test, and a
/// test too large for a patch, so its code file sits in a later window.
/// `readAtMerge` still names the most-changed code file of the whole PR
/// file list, not only of the window shown.
#[tokio::test]
async fn read_at_merge_picks_the_code_file_of_the_whole_pr_not_the_first_window() {
    let server = MockServer::start().await;
    mount_pr(&server, 5).await;
    let test_patch = format!(
        "@@ -0,0 +1,3000 @@\n{}",
        (1..=3000)
            .map(|line| format!("+\tassert(t, line{line}, expected{line})"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let added = |name: &str, patch: Option<&str>, additions: u64| {
        let mut file = rest_file(name, patch, additions, 0);
        file["status"] = json!("added");
        file
    };
    let files = vec![
        added(
            "acceptance/README.md",
            Some("@@ -0,0 +1 @@\n+Run the suite"),
            1,
        ),
        added("acceptance/acceptance_test.go", Some(&test_patch), 3000),
        added("pkg/client/client_test.go", None, 3842),
        added(
            "pkg/client/client.go",
            Some("@@ -0,0 +1,3 @@\n+package client\n+func Alpha() {}\n+func Beta() {}"),
            3,
        ),
        added("pkg/client/types.go", Some("@@ -0,0 +1 @@\n+type T int"), 1),
    ];
    mount_file_batches(&server, vec![files], Duration::ZERO).await;
    let data = run(&server, json!({"sections":["patches"], "debug": false})).await;
    let shown = data["pullRequests"][0]["files"]
        .as_array()
        .expect("patch rows")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect::<Vec<_>>();
    assert!(!shown.contains(&"pkg/client/client.go"), "{data}");
    let read = &data["hints"]["readAtMerge"]["query"]["queries"][0];
    for (field, value) in [
        ("ref", json!("fedcba9876543210fedcba9876543210fedcba98")),
        ("path", json!("pkg/client/client.go")),
        // A whole new file ends at its last line: no range past it.
        ("ranges", json!(["1-3"])),
    ] {
        assert_eq!(read[field], value, "{field}: {data}");
    }
}

/// A `matchString` that hits no patch line says so: alone it is an empty
/// row with a recovery tip; beside a body read it is a warning on the row,
/// never a silent summary.
#[tokio::test]
async fn match_string_with_zero_hits_says_so() {
    let server = MockServer::start().await;
    mount_pr(&server, 2).await;
    mount_file_batches(
        &server,
        vec![vec![
            rest_file("src/a.ts", Some("@@ -1 +1 @@\n-x\n+y"), 1, 1),
            rest_file("src/b.ts", Some("@@ -1 +1 @@\n-x\n+z"), 1, 1),
        ]],
        Duration::ZERO,
    )
    .await;
    let (status, alone) = run_status(
        &server,
        json!({"matchString": "absentNeedle", "debug": false}),
    )
    .await;
    assert_eq!(status, "empty", "{alone}");
    let tip = alone["hints"]["text"].to_string();
    assert!(tip.contains("matchString"), "{alone}");
    let with_body = run(
        &server,
        json!({"sections": ["body", "patches"], "matchString": "absentNeedle", "debug": false}),
    )
    .await;
    assert!(
        with_body["pullRequests"][0].get("body").is_some(),
        "{with_body}"
    );
    let warnings = with_body["warnings"].to_string();
    assert!(
        warnings.contains("matchString") && warnings.contains("absentNeedle"),
        "{with_body}"
    );
}

/// A later comment page of a large PR does not re-scan every provider batch
/// one round trip at a time: with the PR's comment count in hand, the
/// batches load at once, and the page holds exactly its comments.
#[tokio::test]
async fn later_review_comment_pages_load_provider_batches_concurrently() {
    let server = MockServer::start().await;
    let mut raw = pr(1);
    raw["review_comments"] = json!(600);
    // The clock starts at the first provider request: building the HTTP
    // client (system proxy lookup) is a one-time cost, not a batch scan.
    let first_request = std::sync::Arc::new(std::sync::OnceLock::<Instant>::new());
    let pr_template = ResponseTemplate::new(200).set_body_json(raw);
    let arrival = first_request.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(move |_: &wiremock::Request| {
            arrival.get_or_init(Instant::now);
            pr_template.clone()
        })
        .mount(&server)
        .await;
    for page in 1..=6usize {
        let comments = (0..100)
            .map(|i| {
                let n = (page - 1) * 100 + i;
                json!({"id": n, "user": {"login": "reviewer"}, "body": format!("note {n}"),
                       "created_at": "2024-01-01T00:00:00Z", "path": "a.rs", "line": 1})
            })
            .collect::<Vec<_>>();
        let mut response = ResponseTemplate::new(200)
            .set_delay(Duration::from_millis(500))
            .set_body_json(Value::Array(comments));
        if page < 6 {
            response = response.insert_header(
                "link",
                format!(
                    "<{}/api/v3/repos/a/b/pulls/9/comments?per_page=100&page={}>; rel=\"next\"",
                    server.uri(),
                    page + 1
                )
                .as_str(),
            );
        }
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/pulls/9/comments"))
            .and(query_param("page", page.to_string()))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let data = run(
        &server,
        json!({"sections":["reviewComments"], "commentPage": 15, "pageSize": 30, "debug": false}),
    )
    .await;
    let elapsed = first_request.get().expect("the PR was read").elapsed();
    let comments = data["pullRequests"][0]["comments"]
        .as_array()
        .expect("comments");
    assert_eq!(comments.len(), 30, "{data}");
    assert_eq!(comments[0]["body"], "note 420", "{data}");
    assert!(
        // A sequential re-scan of the five batches before the page takes 2.5 s.
        elapsed < Duration::from_millis(1_800),
        "sequential re-scan: {elapsed:?}"
    );
}

/// A hunk with GitHub's 3 context lines around a hit at line 53 of a
/// 100-line file, and that file at the head.
fn hunk_at_53() -> (String, String) {
    let patch = "@@ -50,7 +50,7 @@ fn f\n line 50\n line 51\n line 52\n-old 53\n+new needle 53\n line 54\n line 55\n line 56\n".to_owned();
    let head = (1..=100)
        .map(|n| {
            if n == 53 {
                "new needle 53\n".to_owned()
            } else {
                format!("line {n}\n")
            }
        })
        .collect();
    (patch, head)
}

/// contextLines past GitHub's hunk context: the read widens the run from
/// the file at the PR head (`sourceSha`), so the default 10 lines are not
/// silently cut to the hunk's 3.
#[tokio::test]
async fn match_context_past_the_hunk_reads_the_head_text() {
    use base64::Engine as _;
    let server = MockServer::start().await;
    mount_pr(&server, 1).await;
    let (patch, head) = hunk_at_53();
    mount_file_batches(
        &server,
        vec![vec![rest_file("src/a.rs", Some(&patch), 1, 1)]],
        Duration::ZERO,
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src/a.rs"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "type":"file","encoding":"base64","path":"src/a.rs",
            "content": base64::engine::general_purpose::STANDARD.encode(&head)
        })))
        .mount(&server)
        .await;
    let data = run(
        &server,
        json!({"sections":["patches"], "matchString": "needle", "minify": "none", "debug": false}),
    )
    .await;
    let file = &data["pullRequests"][0]["files"][0];
    let text = file["patch"].as_str().expect("patch");
    assert!(
        text.starts_with("@@ -44,20 +44,20 @@ fn f\n44\t line 44\n"),
        "{text}"
    );
    assert!(text.ends_with("63\t line 63\n"), "{text}");
    assert!(file.get("contextClipped").is_none(), "{data}");
    assert!(
        data.get("next")
            .is_none_or(|next| next.get("expandContext").is_none()),
        "{data}"
    );
}

/// Without the head text (the read failed), the clip is explicit: the file
/// is flagged and `next.expandContext` reads the wanted lines at sourceSha.
#[tokio::test]
async fn match_context_clipped_by_the_hunk_is_flagged_with_a_head_read() {
    let server = MockServer::start().await;
    mount_pr(&server, 1).await;
    let (patch, _) = hunk_at_53();
    mount_file_batches(
        &server,
        vec![vec![rest_file("src/a.rs", Some(&patch), 1, 1)]],
        Duration::ZERO,
    )
    .await;
    let (_, data) = run_status(
        &server,
        json!({"sections":["patches"], "matchString": "needle", "minify": "none", "debug": false}),
    )
    .await;
    let file = &data["pullRequests"][0]["files"][0];
    assert_eq!(file["contextClipped"], true, "{data}");
    let read = &data["next"]["expandContext"];
    assert_eq!(read["tool"], "ghGetFileContent", "{data}");
    let row = &read["query"]["queries"][0];
    assert_eq!(row["ref"], SHA, "{data}");
    assert_eq!(row["path"], "src/a.rs", "{data}");
    assert_eq!(row["ranges"], json!(["43-63"]), "{data}");
    assert!(
        data["partialReasons"]
            .to_string()
            .contains("contextClipped"),
        "{data}"
    );
}

/// A 115-file pull request (one 100-file provider batch, then 15 files)
/// read with the default `sections:["patches"]`: following only the
/// returned continuations (`responsePagination.next` first, then
/// `next.continuePatch` / `next.nextFilePage`) every call is a
/// contract-valid success and the walk delivers every patch exactly once.
/// The first window of a file page that does not fit one response also
/// lists the page's `fileSummary`: that list must ride inside the page
/// budget, or the row splits into a part with an empty `files` array.
#[tokio::test]
async fn default_large_pr_patch_walk_reaches_every_patch_once() {
    let server = MockServer::start().await;
    mount_pr(&server, 115).await;
    let file = |index: usize| {
        let dir = [
            "packages/react-devtools-timeline/src/content-views/utils",
            "packages/react-devtools-shared/src/devtools/views/Profiler",
            "packages/react-devtools-shell/src/app/InspectableElements",
        ][index % 3];
        let name = format!("{dir}/ComponentMeasuresView{index}.js");
        let lines = 6 + (index * 37) % 140;
        let body = (0..lines)
            .map(|line| {
                format!("-  const value{line} = \"timeline\" + computeLane({index}, {line});\n")
            })
            .collect::<String>();
        let patch = format!(
            "@@ -1,{lines} +0,0 @@ module.exports = {{\n{}",
            body.trim_end()
        );
        rest_file(&name, Some(&patch), 0, lines as u64)
    };
    let files = (0..115).map(file).collect::<Vec<_>>();
    let expected = files
        .iter()
        .map(|file| {
            (
                file["filename"].as_str().expect("name").to_owned(),
                file["patch"].as_str().expect("patch").to_owned(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    mount_file_batches(
        &server,
        vec![files[..100].to_vec(), files[100..].to_vec()],
        Duration::ZERO,
    )
    .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[
        ("GITHUB_API_URL", format!("{}/api/v3", server.uri())),
        ("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH", "20000".into()),
    ]);
    let mut envelope = json!({"queries": [{"operation": "pullRequest", "owner": "a",
        "repo": "b", "number": 9, "sections": ["patches"]}]});
    let mut read = std::collections::BTreeMap::<String, String>::new();
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls <= 80, "walk did not finish");
        let outcome = runtime
            .execute(
                format!("walk-{calls}"),
                "ghGetHistoryItem".into(),
                envelope.clone(),
            )
            .await
            .unwrap_or_else(|error| panic!("call {calls} failed: {error:?}\n{envelope}"));
        let content = &outcome.structured_content;
        assert_eq!(
            content["results"][0]["status"]
                .as_str()
                .unwrap_or("success"),
            "success",
            "{content}"
        );
        // A default patch window fits its response page: the row never
        // splits into response parts.
        assert!(
            content.get("responsePagination").is_none(),
            "call {calls}: {}",
            content["responsePagination"]
        );
        let data = &content["results"][0]["data"];
        for file in data["pullRequests"][0]["files"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let path = file["path"].as_str().expect("path").to_owned();
            let text = read.entry(path).or_default();
            let offset = file["patchPagination"]["offset"].as_u64().unwrap_or(0);
            assert_eq!(
                offset as usize,
                text.chars().count(),
                "call {calls}: {file}"
            );
            text.push_str(file["patch"].as_str().unwrap_or(""));
        }
        let next = content["responsePagination"]["next"]["query"]
            .as_object()
            .or_else(|| data["next"]["continuePatch"]["query"].as_object())
            .or_else(|| data["next"]["nextFilePage"]["query"].as_object());
        match next {
            Some(next) => envelope = Value::Object(next.clone()),
            None => break,
        }
    }
    assert_eq!(read.len(), 115, "patches reached: {}", read.len());
    for (path, patch) in &expected {
        assert_eq!(
            read.get(path).map(|view| raw_patch(view)).as_ref(),
            Some(patch),
            "{path}"
        );
    }
    runtime.close().await;
}
