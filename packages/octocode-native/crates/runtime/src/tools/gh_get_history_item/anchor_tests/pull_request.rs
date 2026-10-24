//! Pull request rows: patch-hit filtering, base-side reads of removed files, miss leads, verbatim comments and shared commit rows.
use super::*;

async fn mount_pr(server: &MockServer, files: Value) {
    mount_json(
        server,
        "/api/v3/repos/o/r/pulls/1",
        200,
        json!({"number":1,"title":"Fix","state":"open","user":{"login":"ada"},
            "head":{"sha":HEAD,"ref":"fix"},"base":{"sha":BASE,"ref":"main"},
            "changed_files":files.as_array().map_or(0, Vec::len),
            "additions":1,"deletions":1,"created_at":"2024-01-01T00:00:00Z"}),
    )
    .await;
    mount_json(server, "/api/v3/repos/o/r/pulls/1/files", 200, files).await;
}

/// `matchString` matches patch lines only: a file whose path alone holds
/// it is not kept (its whole patch would answer nothing), and the files the
/// filter dropped are counted on the call.
#[tokio::test]
async fn pr_match_string_keeps_patch_hits_only_and_counts_dropped_files() {
    let server = MockServer::builder().start().await;
    mount_pr(
        &server,
        json!([
            {"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,"sha":"1",
                "patch":"@@ -1,2 +1,2 @@\n keep\n-old\n+new Needle"},
            {"filename":"src/needle.rs","status":"modified","additions":1,"deletions":1,"sha":"2",
                "patch":"@@ -1 +1 @@\n-x\n+y"},
            {"filename":"src/b.rs","status":"modified","additions":1,"deletions":1,"sha":"3",
                "patch":"@@ -1 +1 @@\n-p\n+q"}
        ]),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["patches"],"matchString":["needle"]}),
    )
    .await;
    let paths = out["pullRequests"][0]["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|file| file["path"].clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, [json!("src/a.rs")], "{out}");
    let warnings = out["warnings"].to_string();
    assert!(
        warnings.contains("matchString dropped 2 of 3 changed files"),
        "{out}"
    );
}

/// A patchless file a `matchString` could not search is read at the side
/// that holds it: a removed file at the base, never at the head (a 404).
/// Its path holding the literal does not make it searched.
#[tokio::test]
async fn pr_unpatched_removed_file_is_searched_at_the_base() {
    let server = MockServer::builder().start().await;
    mount_pr(
        &server,
        json!([
            {"filename":"src/needle_gone.rs","status":"removed","additions":0,"deletions":900,
                "sha":"1"},
            {"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,"sha":"2",
                "patch":"@@ -1,2 +1,2 @@\n keep\n-old\n+new needle"}
        ]),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["patches"],"matchString":["needle"]}),
    )
    .await;
    assert_eq!(
        out["pullRequests"][0]["unsearchedFiles"],
        json!(["!tooLarge src/needle_gone.rs"]),
        "{out}"
    );
    let search = &out["next"]["searchUnpatchedFile"]["input"]["queries"][0];
    assert_eq!(search["path"], "src/needle_gone.rs", "{out}");
    assert_eq!(search["ref"], BASE, "{out}");
}

/// A PR `matchString` that hit no patch line is an empty row whose leads run
/// the fix, with no prose naming them: the same read without the literal (its
/// scope kept, its `contextLines` dropped), then the whole file inventory. The
/// published row keeps both ahead of the section menu.
#[tokio::test]
async fn pr_match_string_miss_offers_the_unfiltered_read() {
    let server = MockServer::builder().start().await;
    mount_pr(
        &server,
        json!([{"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,
            "sha":"1","patch":"@@ -1,2 +1,2 @@\n keep\n-old\n+new"}]),
    )
    .await;
    let query = json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
        "include":["src/**"],"matchString":["zzqx"],"contextLines":{"before":2,"after":2}});
    let out = read(&server, query.clone()).await;
    assert_eq!(out["status"], "empty", "{out}");
    assert!(out.get("hints").is_none(), "{out}");
    let lead =
        crate::tools::test_support::checks::related_call(&out, "ghGetHistoryItem", |input| {
            input["queries"][0].get("matchString").is_none()
        });
    assert_eq!(lead["tool"], "ghGetHistoryItem", "{out}");
    assert!(lead.get("confidence").is_none(), "{out}");
    let unfiltered = &lead["input"]["queries"][0];
    for absent in [
        "matchString",
        "contextLines",
        "regexEngine",
        "caseMode",
        "wholeWord",
    ] {
        assert!(unfiltered.get(absent).is_none(), "{absent}: {unfiltered}");
    }
    assert_eq!(unfiltered["number"], 1, "{unfiltered}");
    assert_eq!(unfiltered["include"], json!(["src/**"]), "{unfiltered}");
    assert_eq!(unfiltered["sections"], json!(["patches"]), "{unfiltered}");
    let inventory =
        &crate::tools::test_support::checks::related_call(&out, "ghGetHistoryItem", |input| {
            input["queries"][0]["sections"] == json!(["files"])
        })["input"]["queries"][0];
    assert_eq!(inventory["number"], 1, "{inventory}");
    assert_eq!(inventory["sections"], json!(["files"]), "{inventory}");
    for absent in ["include", "matchString", "contextLines"] {
        assert!(inventory.get(absent).is_none(), "{absent}: {inventory}");
    }
    checks::assert_leads_valid(&out);
    let published = published(ToolId::GhGetHistoryItem, &query, &out);
    let calls = checks::leads(&published);
    assert!(!calls.is_empty());
    assert!(calls.iter().all(|call| call.kind == "related"));
}

/// A PR `include` that matched no changed file is an empty row whose lead
/// reads the whole file inventory to copy a path from; there is no literal
/// to drop.
#[tokio::test]
async fn pr_include_miss_offers_the_file_inventory() {
    let server = MockServer::builder().start().await;
    mount_pr(
        &server,
        json!([{"filename":"src/a.rs","status":"modified","additions":1,"deletions":1,
            "sha":"1","patch":"@@ -1,2 +1,2 @@\n keep\n-old\n+new"}]),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["files"],"include":["docs/"]}),
    )
    .await;
    assert_eq!(out["status"], "empty", "{out}");
    assert!(out.get("hints").is_none(), "{out}");
    assert!(out["next"].get("withoutMatchString").is_none(), "{out}");
    let inventory =
        &crate::tools::test_support::checks::related_call(&out, "ghGetHistoryItem", |input| {
            input["queries"][0]["sections"] == json!(["files"])
        })["input"]["queries"][0];
    assert_eq!(inventory["number"], 1, "{inventory}");
    assert_eq!(inventory["sections"], json!(["files"]), "{inventory}");
    assert!(inventory.get("include").is_none(), "{inventory}");
    checks::assert_leads_valid(&out);
}

/// Comment text is GitHub's verbatim text by default: an HTML comment in a
/// discussion or inline review comment stays, and no raw re-read is needed.
#[tokio::test]
async fn pr_comment_bodies_are_verbatim_by_default() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, json!([])).await;
    let comment = |id: u64| {
        json!({"id":id,"user":{"login":"rev"},"body":"See <!-- hidden note --> this \u{1f6a8}",
            "created_at":"2024-01-01T00:00:00Z"})
    };
    mount_json(
        &server,
        "/api/v3/repos/o/r/issues/1/comments",
        200,
        json!([comment(1)]),
    )
    .await;
    let mut inline = comment(2);
    inline["path"] = json!("src/a.rs");
    inline["line"] = json!(3);
    inline["side"] = json!("RIGHT");
    inline["commit_id"] = json!(HEAD);
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1/comments",
        200,
        json!([inline]),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,
            "sections":["comments","reviewComments"]}),
    )
    .await;
    let row = &out["pullRequests"][0];
    for section in ["comments", "reviewComments"] {
        assert_eq!(
            row[section][0]["body"], "See <!-- hidden note --> this \u{1f6a8}",
            "{section}: {out}"
        );
    }
    assert!(row.get("bodyView").is_none(), "{out}");
    assert!(out.pointer("/next/readRawBody").is_none(), "{out}");
}

/// A pull request's commits are the one commit row every history list
/// shows (headline, login author, committer date in UTC), and keep the
/// message past its headline as `messageBody`.
#[tokio::test]
async fn pr_commit_rows_are_the_shared_commit_row_with_their_body() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1",
        200,
        json!({"number":1,"title":"t","state":"open","user":{"login":"ada"},
            "head":{"sha":HEAD},"base":{"sha":BASE},"commits":2,"changed_files":0}),
    )
    .await;
    let mut rebased = commit_raw(HEAD, "Fix parser\n\nWhy it broke.\n");
    rebased["commit"]["committer"] = json!({"name":"Bot","date":"2024-02-01T02:00:00+02:00"});
    let unlinked = json!({"sha":BASE,"author":null,"commit":{"message":"one line",
        "author":{"name":"Ada","date":"2024-01-02T00:00:00Z"}}});
    mount_json(
        &server,
        "/api/v3/repos/o/r/pulls/1/commits",
        200,
        json!([rebased, unlinked]),
    )
    .await;
    let data = read(
        &server,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":1,"sections":["commits"]}),
    )
    .await;
    assert_eq!(
        data["pullRequests"][0]["commits"],
        json!([
            {"commitSha":HEAD,"date":"2024-02-01T00:00:00Z","messageHeadline":"Fix parser",
                "author":"ada","messageBody":"Why it broke."},
            {"commitSha":BASE,"date":"2024-01-02T00:00:00Z","messageHeadline":"one line",
                "author":"Ada"}
        ]),
        "{data}"
    );
}
