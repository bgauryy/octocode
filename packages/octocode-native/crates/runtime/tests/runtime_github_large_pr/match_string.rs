//! matchString over pull request patches: matching hunks, patchless files, every file page and zero hits.

use super::{SHA, mount_file_batches, mount_pr, numbered, raw_patch, rest_file, run, run_status};
use crate::support::{self};
use serde_json::json;
use std::time::Duration;
use wiremock::MockServer;

#[tokio::test]
async fn match_string_returns_matching_hunks_and_offers_the_whole_patch() {
    let server = MockServer::builder().start().await;
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
        json!({"sections":["patches"], "matchString": ["esbuild"]}),
    )
    .await;
    let files = &data["pullRequests"][0]["files"];
    assert_eq!(files.as_array().map(Vec::len), Some(1), "{data}");
    let patch = files[0]["patch"].as_str().expect("patch");
    assert_eq!(
        patch,
        "@@ -201,1 +201,1 @@\n201\t-old esbuild\n201\t+new esbuild\n"
    );
    assert_eq!(files[0]["fullPatchChars"], big.chars().count());
    assert_eq!(data["pullRequests"][0]["headSha"], SHA);
    // Without a patch row the read carries the same slim identity header.
    let (status, none) = run_status(
        &server,
        json!({"sections":["patches"], "matchString": ["absent-term"]}),
    )
    .await;
    let none_row = &none["pullRequests"][0];
    assert_eq!(none_row["headSha"], SHA, "{none}");
    assert_eq!(none_row["state"], "merged", "{none}");
    assert_eq!(none_row["title"], "Large refactor", "{none}");
    assert!(none_row.get("bodyPreview").is_none(), "{none}");
    // A matchString that hits no patch line is an empty read, not a summary
    // that looks like an ignored filter; its leads run the fix: the same
    // read without the literal, then the file inventory.
    assert_eq!(status, "empty", "{none}");
    assert!(none["hints"].is_null(), "{none}");
    let unfiltered = &crate::support::checks::related_call(&none, "ghGetHistoryItem", |input| {
        input["queries"][0].get("matchString").is_none()
    })["input"]["queries"][0];
    assert!(unfiltered.get("matchString").is_none(), "{none}");
    assert_eq!(unfiltered["sections"], json!(["patches"]), "{none}");
    assert!(unfiltered.get("minify").is_none(), "{none}");
    assert_eq!(
        support::checks::related_call(
            &none,
            "ghGetHistoryItem",
            |input| input["queries"][0]["sections"] == json!(["files"])
        )["input"]["queries"][0]["sections"],
        json!(["files"]),
        "{none}"
    );
    support::checks::assert_leads_valid(&none);
    let only_hits = run(
        &server,
        json!({"sections":["patches"], "matchString": ["esbuild"],
               "contextLines": {"before":0,"after":0}}),
    )
    .await;
    assert_eq!(
        only_hits["pullRequests"][0]["files"][0]["patch"],
        "@@ -201,1 +201,1 @@\n201\t-old esbuild\n201\t+new esbuild\n"
    );
    let unfiltered = support::checks::related_call(&only_hits, "ghGetHistoryItem", |input| {
        input["queries"][0]["sections"] == json!(["patches"])
            && input["queries"][0].get("matchString").is_none()
    });
    assert_eq!(
        unfiltered["input"]["queries"][0]["include"],
        json!(["src/big.rs"])
    );
    let hinted = run(
        &server,
        json!({"sections":["patches"], "matchString": ["esbuild"]}),
    )
    .await;
    // Distinct optional reads remain available, even for one file.
    let source = support::checks::related_call(&hinted, "ghGetFileContent", |input| {
        input["queries"][0]["ref"] == SHA && input["queries"][0]["path"] == "src/big.rs"
    });
    assert_eq!(source["kind"], "related");
    let whole = support::checks::related_call(&hinted, "ghGetHistoryItem", |input| {
        input["queries"][0]["sections"] == json!(["patches"])
            && input["queries"][0].get("matchString").is_none()
    });
    assert_eq!(
        whole["input"]["queries"][0]["include"],
        json!(["src/big.rs"])
    );
    // Execute the authored optional expansion and retain every original patch line.
    let full = run(&server, whole["input"]["queries"][0].clone()).await;
    let whole = full["pullRequests"][0]["files"][0]["patch"]
        .as_str()
        .unwrap_or("");
    assert_eq!(raw_patch(whole), big, "{full}");
    assert!(whole.contains("\n201\t+new esbuild\n"), "{full}");
}

/// `matchString` searches patches, so a pull-request read with a
/// literal and no content selection searches every patch (within
/// `include`) instead of silently returning only the summary.
#[tokio::test]
async fn match_string_without_content_searches_the_filtered_patches() {
    let server = MockServer::builder().start().await;
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
        json!({"matchString": ["import"], "contextLines": {"before":0,"after":0},
               "include":["src/mod/**"]}),
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

/// A patch search never implies absence for files GitHub sent without a
/// patch: it lists them, marks the row partial, and offers a source read at
/// the PR head.
#[tokio::test]
async fn match_string_lists_the_patchless_files_it_could_not_search() {
    let server = MockServer::builder().start().await;
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
        json!({"sections":["patches"], "matchString": ["import"]}),
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
    assert_eq!(read["kind"], "continue");
    assert!(read["input"]["cursor"].is_string());
}

/// A literal search covers every changed file in one call (not 30
/// files a page) and returns hit lines only, so its bytes track the hits.
#[tokio::test]
async fn match_string_covers_every_file_page_in_one_call() {
    let server = MockServer::builder().start().await;
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
    let data = run(&server, json!({"matchString": ["miri"]})).await;
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
    let server = MockServer::builder().start().await;
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
               "include":["src/**"]}),
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

/// A `matchString` that hits no patch line says so: alone it is an empty
/// row whose leads read without it and read the file inventory; beside a
/// body read it is a warning on the row, never a silent summary.
#[tokio::test]
async fn match_string_with_zero_hits_says_so() {
    let server = MockServer::builder().start().await;
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
    let (status, alone) = run_status(&server, json!({"matchString": ["absentNeedle"]})).await;
    assert_eq!(status, "empty", "{alone}");
    assert!(alone["hints"].is_null(), "{alone}");
    let unfiltered = &crate::support::checks::related_call(&alone, "ghGetHistoryItem", |input| {
        input["queries"][0].get("matchString").is_none()
    })["input"]["queries"][0];
    assert!(unfiltered.get("matchString").is_none(), "{alone}");
    assert_eq!(unfiltered["number"], 9, "{alone}");
    // The literal alone searched every patch; the read without it reads them.
    assert_eq!(unfiltered["sections"], json!(["patches"]), "{alone}");
    assert_eq!(
        support::checks::related_call(
            &alone,
            "ghGetHistoryItem",
            |input| input["queries"][0]["sections"] == json!(["files"])
        )["input"]["queries"][0]["sections"],
        json!(["files"]),
        "{alone}"
    );
    support::checks::assert_leads_valid(&alone);
    let with_body = run(
        &server,
        json!({"sections": ["body", "patches"], "matchString": ["absentNeedle"]}),
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
