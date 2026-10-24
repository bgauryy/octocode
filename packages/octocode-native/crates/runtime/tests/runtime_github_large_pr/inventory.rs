//! Pull request file inventories: patchless and renamed files, identity headers, filters, provider caps and totals.

use super::{mount_file_batches, mount_pr, no_continuation, numbered, raw_patch, rest_file, run};
use crate::support::{self};
use serde_json::{Value, json};
use std::time::Duration;
use support::{Workspace, call, row_data};
use wiremock::MockServer;

#[tokio::test]
async fn large_pr_inventory_flags_patchless_files_and_keeps_rename_origin() {
    let server = MockServer::builder().start().await;
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
    let data = run(&server, json!({"sections":["files"]})).await;
    // Compact rows: every file of one directory shares its first group.
    assert_eq!(
        data["pullRequests"][0]["files"],
        json!([
            {"src/": [
                "M +1 -1 ok.rs",
                "R +0 -0 new_name.rs <- src/old_name.rs",
                "M +39550 -39342 !tooLarge checker.ts",
                "M !omitted omitted.ts"
            ]},
            "M +0 -0 !binary assets/logo.png"
        ]),
        "{data}"
    );
}

#[tokio::test]
async fn pr_inventory_carries_the_identity_header_and_its_own_next_steps_only() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    mount_file_batches(
        &server,
        vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)],
        Duration::ZERO,
    )
    .await;
    let data = run(&server, json!({"sections":["files"]})).await;
    let row = &data["pullRequests"][0];
    // A section read keeps what names the PR and the commits code is read at.
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
    // The first page of a file inventory states how many files it lists.
    assert_eq!(row["changedFilesCount"], 250, "{row}");
    assert_eq!(row["additions"], 5000);
    assert_eq!(row["labels"], json!(["refactor"]));
    assert_eq!(row["contentPagination"]["files"]["totalItems"], 250);
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
    let server = MockServer::builder().start().await;
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
    let data = run(&server, json!({"sections":["files"]})).await;
    let selected = support::checks::related_call(&data, "ghGetHistoryItem", |input| {
        input["queries"][0]["sections"] == json!(["patches"])
            && input["queries"][0]["include"].is_array()
    });
    let scope = selected["input"]["queries"][0]["include"]
        .as_array()
        .expect("selected paths");
    assert_eq!(scope.len(), 2);
    assert!(scope.contains(&json!("src/server/webStandardStreamableHttp.ts")));
    assert!(scope.contains(&json!("src/server/sseKeepAlive.ts")));
    let all = support::checks::related_call(&data, "ghGetHistoryItem", |input| {
        input["queries"][0]["sections"] == json!(["patches"])
            && input["queries"][0].get("include").is_none()
    });
    for (lead, count) in [(selected, 2), (all, 5)] {
        assert_eq!(lead["kind"], "related");
        let patches = run(&server, lead["input"]["queries"][0].clone()).await;
        let files = patches["pullRequests"][0]["files"]
            .as_array()
            .expect("patch files");
        assert_eq!(files.len(), count);
        for (index, file) in files.iter().enumerate() {
            let patch = file["patch"].as_str().or_else(|| {
                let earlier = file["samePatchAs"].as_str()?;
                files[..index].iter().find(|row| row["path"] == earlier)?["patch"].as_str()
            });
            let patch =
                patch.expect("patch text or a direct reference to an earlier delivered patch");
            assert_eq!(
                raw_patch(patch),
                "@@ -1 +1 @@\n-a\n+b",
                "{}: {patch:?}",
                file["path"]
            );
        }
    }
    assert_eq!(support::inventory_len(&data["pullRequests"][0]["files"]), 5);
}

#[tokio::test]
async fn pr_file_filter_narrows_the_inventory_and_its_counts() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    let mut batches = vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)];
    batches[1][7] = rest_file("docs/guide.md", Some("@@ -1 +1 @@\n-a\n+b"), 30, 2);
    batches[2][3]["status"] = json!("added");
    mount_file_batches(&server, batches, Duration::ZERO).await;
    let data = run(
        &server,
        json!({"sections":["files"],
               "include":["**/*.md","src/b3/**"],"fileStatus":["added","modified"],"minChanges":3}),
    )
    .await;
    let row = &data["pullRequests"][0];
    assert_eq!(row["files"], json!(["M +30 -2 docs/guide.md"]), "{row}");
    assert_eq!(row["contentPagination"]["files"]["totalItems"], 1, "{row}");
    let data = run(
        &server,
        json!({"sections":["files"],
               "include":["src/b3/**"],"fileStatus":["added"]}),
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
    let server = MockServer::builder().start().await;
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
    let data = run(&server, json!({"sections":["patches"]})).await;
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
    let server = MockServer::builder().start().await;
    mount_pr(&server, 3500).await;
    mount_file_batches(
        &server,
        (1..=30).map(|batch| numbered(batch, 100)).collect(),
        Duration::ZERO,
    )
    .await;
    let data = run(&server, json!({"sections":["files"]})).await;
    // The section cursors always show; the provider limit is the
    // row's evidence beside them.
    assert!(
        data["pullRequests"][0].get("contentPagination").is_some(),
        "{data}"
    );
    assert_eq!(data["terminalLimit"], true, "{data}");
    assert_eq!(
        data["providerLimit"]["reason"], "providerFileListLimit",
        "{data}"
    );
    assert_eq!(data["providerLimit"]["listed"], 3000, "{data}");
    assert_eq!(data["providerLimit"]["changedFilesCount"], 3500, "{data}");
    assert!(
        data["partialReasons"]
            .as_array()
            .is_some_and(|reasons| reasons.contains(&json!("providerFileListLimit"))),
        "{data}"
    );
}

#[tokio::test]
async fn pr_inventory_page_reports_the_provider_total() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, 250).await;
    mount_file_batches(
        &server,
        vec![numbered(1, 100), numbered(2, 100), numbered(3, 50)],
        Duration::ZERO,
    )
    .await;
    let data = run(&server, json!({"sections":["files"]})).await;
    let page = &data["pullRequests"][0]["contentPagination"]["files"];
    assert_eq!(page["totalItems"], 250, "{page}");
    assert_eq!(page["totalPages"], 1, "{page}");
    assert_eq!(page["hasMore"], false, "{page}");
    // The cursor rides `next`, never the page block.
    assert!(page.get("nextPage").is_none(), "{page}");
    assert_eq!(
        support::inventory_len(&data["pullRequests"][0]["files"]),
        250
    );
    assert!(data["next"].get("nextFilePage").is_none());
}

/// An `include` scope that matches no changed file is an empty row (a
/// wrong path once got a silent one) whose lead reads the whole file
/// inventory to copy a path from.
#[tokio::test]
async fn files_scope_matching_nothing_is_an_empty_row_with_an_inventory_lead() {
    let server = MockServer::builder().start().await;
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
               "sections": ["patches"], "include": ["fastapi/_runtime.py"]}),
    )
    .await
    .expect("read");
    let data = row_data(&outcome);
    assert!(
        data.get("errorCode").is_none(),
        "an empty row carries no errorCode: {data}"
    );
    assert!(data["hints"].is_null(), "{data}");
    let inventory = support::checks::related_call(data, "ghGetHistoryItem", |input| {
        input["queries"][0]["sections"] == json!(["files"])
    });
    assert!(inventory["input"]["queries"][0].get("include").is_none());
    let listed = support::run(&runtime, "ghGetHistoryItem", inventory["input"].clone()).await;
    assert_eq!(
        support::inventory_len(&listed.data()["pullRequests"][0]["files"]),
        1
    );
    support::checks::assert_leads_valid(data);
    runtime.close().await;
}
