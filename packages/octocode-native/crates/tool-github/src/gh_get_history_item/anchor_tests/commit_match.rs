//! Commit `matchString` and path scopes: hit pages, dropped path-only hits, patchless files listed as unsearched.
use super::*;
use octocode_test_support::json_fixture;

/// `matchString` with no section named searches the patches (as on a pull
/// request): it is never silently ignored.
#[tokio::test]
async fn commit_match_string_without_sections_reads_its_patches() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["files"] = json!([changed_file("src/a.rs"), {
        "filename":"src/b.rs","status":"modified","additions":1,"deletions":1,
        "patch":"@@ -1 +1 @@\n-x\n+y"}]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":["new"]}),
    )
    .await;
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    assert_eq!(files[0]["path"], "src/a.rs", "{out}");
    assert!(
        files[0]["patch"]
            .as_str()
            .is_some_and(|patch| patch.contains("+new"))
    );
    assert!(out["next"].get("readPatches").is_none(), "{out}");
}

/// A `matchString` that removes every changed file of the page says so
/// (how many files, what it searches) and leads to the same read without it;
/// it is not the silent `"files":[]` of a file the commit never touched. A
/// match inside a hunk header (the function context after `@@`) keeps the
/// file: the header is part of the diff that is shown.
#[tokio::test]
async fn commit_match_string_that_removes_every_file_says_so() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["files"] = json!([changed_file("src/a.rs"), {
        "filename":"src/b.rs","status":"modified","additions":1,"deletions":1,
        "patch":"@@ -40,2 +40,2 @@ def url_for(\n keep\n-x\n+y"}]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,
            "include":["src/**"],"matchString":["zzqx"]}),
    )
    .await;
    assert_eq!(out["files"], json!([]), "{out}");
    assert_eq!(out["status"], "empty", "{out}");
    let warnings = out["warnings"].to_string();
    assert!(
        warnings.contains("matchString matched 0 of 2 changed files")
            && warnings.contains("hunk-header"),
        "{out}"
    );
    // The related read without the literal is the recovery: no sentence
    // restates it.
    assert!(!warnings.contains("drop matchString"), "{out}");
    let lead = octocode_runtime_base::test_support::checks::related_call(
        &out,
        "ghGetHistoryItem",
        |input| input["queries"][0].get("matchString").is_none(),
    );
    assert_eq!(lead["tool"], "ghGetHistoryItem", "{out}");
    let query = &lead["input"]["queries"][0];
    assert!(query.get("matchString").is_none(), "{query}");
    assert_eq!(query["include"], json_fixture(r#"["src/**"]"#), "{query}");
    assert_eq!(query["ref"], HEAD, "{query}");
    assert_eq!(query["sections"], json_fixture(r#"["files"]"#), "{query}");

    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":["url_for"]}),
    )
    .await;
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    assert_eq!(files[0]["path"], "src/b.rs", "{out}");
    // Only a filter that removes every file is an empty answer; one that
    // removes some counts them.
    assert!(out.get("status").is_none(), "{out}");
    assert_eq!(
        out["warnings"],
        json!([
            "matchString dropped 1 of 2 changed files: no patch line holds it.",
            crate::gh_get_history_item::commit::NO_PULL_REQUEST_NAMED
        ]),
        "{out}"
    );
    assert!(out["next"].get("withoutMatchString").is_none(), "{out}");
}

/// A file kept only for a hit in a hunk header's function context shows its
/// whole patch and counts that header as its hit, like any other match.
#[tokio::test]
async fn commit_match_string_header_only_hit_counts_as_a_match() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["files"] = json_fixture(
        r#"[
            {
                "filename": "src/b.rs",
                "status": "modified",
                "additions": 1,
                "deletions": 1,
                "patch": "@@ -40,2 +40,2 @@ def url_for(\n keep\n-x\n+y"
            }
        ]"#,
    );
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":["url_for"]}),
    )
    .await;
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    assert_eq!(files[0]["matchCount"], 1, "{out}");
    assert!(
        files[0]["patch"].as_str().is_some_and(|patch| patch
            .contains("@@ -40,2 +40,2 @@ def url_for(")
            && patch.contains("+y")),
        "{out}"
    );
}

/// A commit's `matchString` keeps the files whose patch holds it (not a
/// path-only hit) and counts the files it dropped. A lowercase literal
/// matches in any case (smart case).
#[tokio::test]
async fn commit_match_string_drops_path_only_hits_and_counts_them() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["files"] = json!([changed_file("src/a.rs"), {
        "filename":"src/new_thing.rs","status":"modified","additions":1,"deletions":1,
        "patch":"@@ -1 +1 @@\n-x\n+y"}]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":["new"]}),
    )
    .await;
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "{out}");
    assert_eq!(files[0]["path"], "src/a.rs", "{out}");
    assert!(
        out["warnings"]
            .to_string()
            .contains("matchString dropped 1 of 2 changed files"),
        "{out}"
    );
}

/// A `matchString` filters a commit's and a comparison's changed files
/// before they are paged, as on a pull request: page N is the N-th page of
/// files whose patch holds the literal, never a page of all files with the
/// misses cut out (which reads as "matched 0" while later pages hit).
#[tokio::test]
async fn commit_and_compare_match_pages_are_pages_of_hits() {
    let server = MockServer::builder().start().await;
    let miss = |name: &str| {
        json!({"filename":name,"status":"modified","additions":1,"deletions":1,
            "patch":"@@ -1 +1 @@\n-x\n+y"})
    };
    let files = json!([
        miss("src/a.rs"),
        changed_file("src/b.rs"),
        miss("src/c.rs"),
        changed_file("src/d.rs")
    ]);
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["files"] = files.clone();
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/compare/{BASE}...{HEAD}"),
        200,
        json!({"ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE},"merge_base_commit":{"sha":MERGE_BASE},
            "commits":[commit_raw(HEAD, "Fix parser")],"files":files}),
    )
    .await;
    for row in [
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD}),
        json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD,
            "sections":["patches"]}),
    ] {
        let mut paths = Vec::new();
        for page in [1, 2] {
            let mut query = row.clone();
            query["matchString"] = json_fixture(r#"["new"]"#);
            query["pageSize"] = json!(1);
            query["page"] = json!(page);
            let out = read(&server, query).await;
            assert!(out.get("status").is_none(), "page {page}: {out}");
            let files = out["files"].as_array().cloned().unwrap_or_default();
            paths.extend(files.iter().map(|file| file["path"].clone()));
            let pages = &out["sectionPagination"]["files"];
            assert_eq!(pages["total"], 2, "page {page}: {out}");
            assert_eq!(pages["hasMore"], page == 1, "page {page}: {out}");
        }
        assert_eq!(paths, [json!("src/b.rs"), json!("src/d.rs")], "{row}");
    }
}

/// A changed file GitHub sent without a patch (too large to diff) was never
/// searched by `matchString`: it is listed as unsearched, never counted as
/// a file no patch line holds the literal in, and the row is partial with
/// the read that searches its source at the side that holds it.
#[tokio::test]
async fn commit_match_string_lists_patchless_files_as_unsearched() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "Fix parser");
    raw["parents"] = json!([{"sha":BASE}]);
    raw["files"] = json!([changed_file("src/a.rs"), {
        "filename":"src/b.rs","status":"modified","additions":1,"deletions":1,
        "patch":"@@ -1 +1 @@\n-x\n+y"}, {
        "filename":"src/big.rs","status":"modified","additions":900,"deletions":3,"sha":"9"}]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":["new"]}),
    )
    .await;
    let paths = out["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| file["path"].clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, [json!("src/a.rs")], "{out}");
    assert_eq!(
        out["warnings"],
        json!([
            "matchString dropped 1 of 3 changed files: no patch line holds it.",
            crate::gh_get_history_item::commit::NO_PULL_REQUEST_NAMED
        ]),
        "{out}"
    );
    assert_eq!(
        out["unsearchedFiles"],
        json_fixture(r#"["!tooLarge src/big.rs"]"#),
        "{out}"
    );
    assert_eq!(out["isPartial"], true, "{out}");
    assert_eq!(
        out["partialReasons"],
        json_fixture(r#"["patchUnavailable"]"#),
        "{out}"
    );
    let search = &out["next"]["searchUnpatchedFile"]["input"]["queries"][0];
    assert_eq!(search["path"], "src/big.rs", "{out}");
    assert_eq!(search["ref"], HEAD, "{out}");
    assert_eq!(search["matchString"], json_fixture(r#"["new"]"#), "{out}");

    // A literal no searched patch holds, beside an unsearched file, is no
    // empty answer: the unsearched file may hold it.
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":["zzqx"]}),
    )
    .await;
    assert_eq!(out["files"], json!([]), "{out}");
    assert!(out.get("status").is_none(), "{out}");
    assert_eq!(
        out["warnings"],
        json!([
            "matchString dropped 2 of 3 changed files: no patch line holds it.",
            crate::gh_get_history_item::commit::NO_PULL_REQUEST_NAMED
        ]),
        "{out}"
    );
    assert_eq!(
        out["unsearchedFiles"],
        json_fixture(r#"["!tooLarge src/big.rs"]"#),
        "{out}"
    );
    assert_eq!(
        out["partialReasons"],
        json_fixture(r#"["patchUnavailable"]"#),
        "{out}"
    );
}

/// A credential file the `matchString` could not search is listed as
/// withheld, and it is no gap and no lead: the path policy refuses a read of
/// it, so the row is not partial and offers no source search.
#[tokio::test]
async fn commit_match_string_withheld_file_is_no_partial_and_no_lead() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "with a credential file");
    raw["files"] = json_fixture(
        r#"[
            {
                "filename": ".env",
                "status": "modified",
                "additions": 1,
                "deletions": 1,
                "sha": "7",
                "patch": "@@ -1 +1 @@\n-A=1\n+needle=2"
            },
            {
                "filename": "src/a.rs",
                "status": "modified",
                "additions": 1,
                "deletions": 1,
                "sha": "8",
                "patch": "@@ -1 +1 @@\n-x\n+needle"
            }
        ]"#,
    );
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,"matchString":["needle"]}),
    )
    .await;
    assert_eq!(
        out["unsearchedFiles"],
        json_fixture(r#"["!withheld .env"]"#),
        "{out}"
    );
    assert!(out.get("isPartial").is_none(), "{out}");
    assert!(out.get("partialReasons").is_none(), "{out}");
    assert!(out["next"].get("searchUnpatchedFile").is_none(), "{out}");
    assert!(!out.to_string().contains("needle=2"), "{out}");
}

#[tokio::test]
async fn commit_path_scope_keeps_the_unfiltered_observed_count_separate() {
    let server = MockServer::builder().start().await;
    let mut raw = commit_raw(HEAD, "scoped files");
    raw["files"] = json!([
        changed_file("src/a.rs"),
        changed_file("docs/b.md"),
        changed_file("tests/c.rs")
    ]);
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/commits/{HEAD}"),
        200,
        raw,
    )
    .await;
    let query = json!({"operation":"commit","owner":"o","repo":"r","ref":HEAD,
        "sections":["files"],"include":["src/**"]});
    let data = read(&server, query.clone()).await;
    assert_history_page_contract(&data);
    assert_eq!(
        data["files"],
        json_fixture(r#"["M +1 -1 src/a.rs"]"#),
        "{data}"
    );
    assert_eq!(data["changedFileCount"], 3, "unfiltered observed files");
    assert_eq!(
        data["sectionPagination"]["files"]["countScope"], "complete",
        "{data}"
    );
    assert_eq!(
        data["sectionPagination"]["files"]["total"], 1,
        "scoped files"
    );
    assert_eq!(
        data["sectionPagination"]["files"]["countScope"], "complete",
        "{data}"
    );
}
