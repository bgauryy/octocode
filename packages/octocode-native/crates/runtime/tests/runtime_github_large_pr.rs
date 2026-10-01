// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
//! Large pull-request review: changed-file inventory, patch availability,
//! provider file-list cap, continuation payload size, and scan latency.

mod support;

use serde_json::{Value, json};
use std::time::{Duration, Instant};
use support::{Workspace, call, row_data, row_status};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome).clone();
    runtime.close().await;
    data
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
    let data = run(
        &server,
        json!({"content": {"changedFiles": true}, "debug": false}),
    )
    .await;
    // Compact rows: consecutive files of one directory share a group.
    assert_eq!(
        data["pullRequests"][0]["changedFiles"],
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
    let data = run(
        &server,
        json!({"content": {"changedFiles": true}, "debug": false}),
    )
    .await;
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
    for dropped in ["bodyPreview", "updatedAt", "sourceBranch"] {
        assert!(row.get(dropped).is_none(), "{dropped} kept: {row}");
    }
    // Merge state and labels ride the first page of every read.
    for kept in ["mergedAt", "closedAt", "targetBranch", "labels"] {
        assert!(row.get(kept).is_some(), "{kept} missing: {row}");
    }
    let menu = row["next"].as_object().expect("next");
    // 250 files: no every-patch read, and no placeholder literal search
    // (only the caller knows the literal).
    assert_eq!(
        menu.keys().collect::<Vec<_>>(),
        ["reviewPatches", "getMergeCommit"],
        "{row}"
    );
    // The review read names up to one patch page of source files.
    let review = &menu["reviewPatches"]["query"]["files"];
    assert_eq!(review.as_array().map(Vec::len), Some(30), "{row}");
    // An omitted pageSize reads the whole 250-file inventory in one page.
    let page = row.get("contentPagination").cloned().unwrap_or_default();
    assert!(page.get("changedFiles").is_none(), "{page}");
    let groups = row["changedFiles"].as_array().expect("changedFiles");
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

/// D2/S7: the review pick is every source file, most changed first (not the
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
    let data = run(
        &server,
        json!({"content": {"changedFiles": true}, "debug": false}),
    )
    .await;
    let menu = &data["pullRequests"][0]["next"];
    let selected = &menu["reviewPatches"];
    assert_eq!(
        selected["query"]["files"],
        json!([
            "src/server/webStandardStreamableHttp.ts",
            "src/server/sseKeepAlive.ts"
        ]),
        "{menu}"
    );
    assert_eq!(selected["confidence"], "high", "{menu}");
    assert_eq!(
        menu["getAllPatches"]["query"]["include"],
        json!(["patches"]),
        "{menu}"
    );
    assert!(data.get("hints").is_none(), "{data}");
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
        json!({"content": {"changedFiles": true}, "debug": true,
               "fileFilter": {"paths": ["*.md", "src/b3/"], "status": ["added", "modified"], "minChanges": 3}}),
    )
    .await;
    let row = &data["pullRequests"][0];
    assert_eq!(
        row["changedFiles"],
        json!(["M +30 -2 docs/guide.md"]),
        "{row}"
    );
    assert_eq!(
        row["contentPagination"]["changedFiles"]["totalItems"], 1,
        "{row}"
    );
    let data = run(
        &server,
        json!({"content": {"changedFiles": true}, "debug": false,
               "fileFilter": {"paths": ["src/b3/"], "status": ["added"]}}),
    )
    .await;
    assert_eq!(
        data["pullRequests"][0]["changedFiles"],
        json!(["A +1 -1 src/b3/f3.rs"]),
        "{data}"
    );
    assert!(
        data["pullRequests"][0].get("next").is_none(),
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
        json!({"content": {"patches": {"mode": "all"}}, "debug": false, "minify": "none"}),
    )
    .await;
    let files = &data["pullRequests"][0]["changedFiles"];
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
        json!({"content": {"changedFiles": true}, "pageSize": 100, "debug": false}),
    )
    .await;
    let page = &data["pullRequests"][0]["contentPagination"]["changedFiles"];
    assert_ne!(page["countScope"], "complete", "{page}");
    assert_eq!(page["terminalLimit"], true, "{page}");
    assert_eq!(
        page["providerLimit"]["reason"], "providerFileListLimit",
        "{page}"
    );
    assert_eq!(page["providerLimit"]["listed"], 100, "{page}");
    assert_eq!(page["providerLimit"]["changedFiles"], 3500, "{page}");
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
        json!({"content": {"changedFiles": true}, "pageSize": 100, "debug": false}),
    )
    .await;
    let page = &data["pullRequests"][0]["contentPagination"]["changedFiles"];
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
    let query = json!({"content": {"changedFiles": true}, "pageSize": 100, "debug": false});
    let follow =
        json!({"content": {"changedFiles": true}, "pageSize": 100, "filePage": 2, "debug": false});
    let first = run(&server, query).await;
    let first_row = &first["pullRequests"][0];
    assert_eq!(first_row["title"], "Large refactor", "{first_row}");
    assert!(first_row.get("next").is_some(), "{first_row}");
    // A literal search of every patch needs the caller's literal: it is
    // never offered as an executable placeholder.
    assert!(
        first_row["next"].get("findInPatches").is_none(),
        "{first_row}"
    );

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
    for kept in ["mergedAt", "closedAt", "targetBranch"] {
        assert!(row.get(kept).is_some(), "{kept} missing: {row}");
    }
    for dropped in [
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
    let group = &row["changedFiles"][0]["src/b2/"];
    assert_eq!(group.as_array().map(Vec::len), Some(100), "{row}");
    assert!(data["next"]["nextChangedFilesPage"].is_object(), "{data}");

    // debug keeps the full header.
    let debug =
        json!({"content": {"changedFiles": true}, "pageSize": 100, "filePage": 2, "debug": true});
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
        json!({"content": {"patches": {"mode": "selected", "files": ["src/big.rs"]}},
               "minify": "none", "debug": false}),
    )
    .await;
    let row = &data["pullRequests"][0];
    let file = &row["changedFiles"][0];
    assert_eq!(file["patchPagination"]["hasMore"], true, "{row}");
    let pages = &row["contentPagination"];
    // The finished single-page file list and the cursor copy add nothing.
    assert!(pages.get("changedFiles").is_none(), "{pages}");
    assert!(pages["patches"].get("charOffset").is_none(), "{pages}");
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
        json!({"content": {"patches": {"mode": "selected", "files": ["src/late.rs"]}},
               "minify": "none", "debug": false}),
    )
    .await;
    let elapsed = started.elapsed();
    let file = &data["pullRequests"][0]["changedFiles"][0];
    assert_eq!(file["path"], "src/late.rs", "{data}");
    assert_eq!(file["patch"], "@@ -1 +1 @@\n-a\n+late");
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
        json!({"content": {"patches": {"mode": "all"}}, "matchString": "esbuild",
               "minify": "none", "debug": false}),
    )
    .await;
    let files = &data["pullRequests"][0]["changedFiles"];
    assert_eq!(files.as_array().map(Vec::len), Some(1), "{data}");
    let patch = files[0]["patch"].as_str().expect("patch");
    // S4: hit lines only by default (matchContext 0).
    assert_eq!(patch, "@@ -201,1 +201,1 @@\n-old esbuild\n+new esbuild\n");
    assert!(patch.len() < 200, "{patch}");
    // The narrowed-view marker selects the continuation; rows omit it.
    assert!(files[0].get("fullPatchChars").is_none(), "{data}");
    // A patch read carries the identity it re-proves (number, state, the
    // head it read) and no follow-up menu; the metadata read names the PR.
    let row = &data["pullRequests"][0];
    assert!(row.get("next").is_none(), "{row}");
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
    let none = run(
        &server,
        json!({"content": {"patches": {"mode": "all"}}, "matchString": "absent-term",
               "minify": "none", "debug": false}),
    )
    .await;
    assert_eq!(none["pullRequests"][0]["title"], "Large refactor", "{none}");
    let only_hits = run(
        &server,
        json!({"content": {"patches": {"mode": "all"}}, "matchString": "esbuild",
               "matchContext": 0, "minify": "none", "debug": false}),
    )
    .await;
    assert_eq!(
        only_hits["pullRequests"][0]["changedFiles"][0]["patch"],
        "@@ -201,1 +201,1 @@\n-old esbuild\n+new esbuild\n"
    );
    assert!(only_hits.get("next").is_none(), "{only_hits}");
    assert_eq!(
        data["next"]["readFullPatches"]["query"]["content"]["patches"]["files"],
        json!(["src/big.rs"]),
        "{data}"
    );
}

/// D1: `matchString` searches patches, so a pull-request read with a
/// literal and no content selection searches every patch (within
/// `fileFilter`) instead of silently returning only the summary.
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
        json!({"matchString": "import", "matchContext": 0,
               "fileFilter": {"paths": ["src/mod/**"]}, "debug": false}),
    )
    .await;
    let files = data["pullRequests"][0]["changedFiles"]
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
        json!({"content": {"patches": {"mode": "all"}}, "matchString": "import",
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
    assert_eq!(read["query"]["branch"], SHA, "{data}");
    assert_eq!(read["query"]["matchString"], "import", "{data}");
    assert_eq!(read["query"]["path"], "src/module.ts", "{data}");
}

/// S1 (G01): a merged PR's merge state rides every PR row, whatever the
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
            json!({"content": {"body": true, "changedFiles": true}, "debug": false}),
            true,
        ),
        (
            "inventory",
            json!({"content": {"changedFiles": true}, "debug": false}),
            true,
        ),
        (
            "patches",
            json!({"content": {"patches": {"mode": "selected", "files": ["src/hit.rs"]}},
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
            json!({"content": {"changedFiles": true}, "pageSize": 100, "filePage": 2,
                   "debug": false}),
            false,
        ),
    ] {
        let data = run(&server, query).await;
        let row = &data["pullRequests"][0];
        assert_eq!(row["mergedAt"], "2024-01-03T00:00:00Z", "{label}: {row}");
        assert_eq!(row["closedAt"], "2024-01-03T00:00:00Z", "{label}: {row}");
        assert_eq!(row["targetBranch"], "main", "{label}: {row}");
        assert_eq!(row["state"], "merged", "{label}: {row}");
        if first_page {
            assert_eq!(row["labels"], json!(["refactor"]), "{label}: {row}");
        }
    }
}

/// S3 (G01): rows of one call share one patch budget, so two large patch
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
               "goal": "g", "reasoning": "r", "minify": "none",
               "content": {"patches": {"mode": "selected", "files": [file]}}})
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
        let file = &data["pullRequests"][0]["changedFiles"][0];
        let taken = file["patchPagination"]["charLength"].as_u64().unwrap_or(0);
        // Half the default window each (one budget), not a whole window each.
        assert!((4_000..=20_000).contains(&taken), "row {index}: {taken}");
        assert!(
            data["next"]["continuePatch"].is_object(),
            "row {index}: {data}"
        );
    }
    runtime.close().await;
}

/// S4 (G10): a literal search covers every changed file in one call (not 30
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
    let files = data["pullRequests"][0]["changedFiles"]
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
            .and_then(|n| n.get("nextChangedFilesPage"))
            .is_none(),
        "{data}"
    );
}

/// A8: the flat `include`/`files` read and the nested `content`/`fileFilter`
/// read it aliases return the same rows through the full runtime.
#[tokio::test]
async fn flat_and_nested_read_shapes_return_identical_rows() {
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
        json!({"include": ["body", "patches"], "files": ["src/**"], "minify": "none", "debug": false}),
    )
    .await;
    let nested = run(
        &server,
        json!({"content": {"body": true, "patches": {"mode": "all"}},
               "fileFilter": {"paths": ["src/**"]}, "minify": "none", "debug": false}),
    )
    .await;
    assert_eq!(
        flat["pullRequests"][0]["changedFiles"],
        nested["pullRequests"][0]["changedFiles"]
    );
    assert_eq!(
        flat["pullRequests"][0]["body"],
        nested["pullRequests"][0]["body"]
    );
    let paths = flat["pullRequests"][0]["changedFiles"]
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

/// A `files` scope that matches no changed file says so (G01 re-asked with a
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
               "include": ["patches"], "files": ["fastapi/_runtime.py"], "debug": false}),
    )
    .await
    .expect("read");
    let data = row_data(&outcome);
    assert_eq!(data["errorCode"], "noSelectedFilesMatched", "{data}");
    assert!(
        data["hints"][0].as_str().unwrap_or("").contains("include"),
        "{data}"
    );
    runtime.close().await;
}
