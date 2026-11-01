//! Compare rows: both resolved commits, empty and reversed ranges, file filters and unpatched reads across comparison pages.
use super::*;

/// A comparison's `matchString` that hits none of its changed files is the
/// same empty answer a commit's is (react v19.0.0...v19.0.1, "zzqqxx"): a
/// warning, `status: "empty"`, and the pinned read without the literal,
/// never a bare `files: []` that reads as "nothing changed".
#[tokio::test]
async fn compare_match_string_that_removes_every_file_says_so() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/compare/{BASE}...{HEAD}"),
        200,
        json!({"ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE},"merge_base_commit":{"sha":MERGE_BASE},
            "commits":[commit_raw(HEAD, "Add feature\n\nWhy: the detail")],
            "files":[changed_file("src/a.rs"), changed_file("src/b.rs")]}),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD,
            "matchString":["zzqqxx_nohit"]}),
    )
    .await;
    assert_eq!(out["files"], json!([]), "{out}");
    assert_eq!(out["status"], "empty", "{out}");
    assert!(
        out["warnings"]
            .to_string()
            .contains("matchString matched 0 of 2 changed files"),
        "{out}"
    );
    let query =
        &crate::tools::test_support::checks::related_call(&out, "ghGetHistoryItem", |input| {
            input["queries"][0].get("matchString").is_none()
        })["input"]["queries"][0];
    assert_eq!(query["operation"], "compare", "{out}");
    assert!(query.get("matchString").is_none(), "{query}");
    assert_eq!(
        (&query["baseRef"], &query["headRef"]),
        (&json!(BASE), &json!(HEAD))
    );
    // The literal alone chose the patches: without it, the read lists the
    // files (as the warning says).
    assert_eq!(query["sections"], json!(["files"]), "{query}");
    // The filter removed every file: no lead reads one of them.
    for kind in ["readAtCommit", "readParent", "readCommit"] {
        assert!(out["next"].get(kind).is_none(), "{kind}: {out}");
    }
}

#[tokio::test]
async fn compare_row_names_both_resolved_commits_and_each_commit_sha() {
    let server = MockServer::builder().start().await;
    // The `application/vnd.github.sha` read answers the bare SHA.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits/feature"))
        .respond_with(ResponseTemplate::new(200).set_body_string(HEAD))
        .mount(&server)
        .await;
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/compare/v1...{HEAD}"),
        200,
        json!({"ahead_by":1,"behind_by":0,"total_commits":1,
            "permalink_url":"https://github.com/o/r/compare/o:1111111...o:2222222",
            "base_commit":{"sha":BASE},"merge_base_commit":{"sha":MERGE_BASE},
            "commits":[commit_raw(HEAD, "Add feature")],
            "files":[changed_file("src/a.rs")]}),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","baseRef":"v1","headRef":"feature"}),
    )
    .await;
    // `base`/`head` carry the resolved SHAs, never the query's ref names.
    assert_eq!(out["baseSha"], BASE, "{out}");
    assert_eq!(out["headSha"], HEAD, "{out}");
    assert!(out.get("commitsScope").is_none(), "{out}");
    assert_eq!(out["commits"][0]["commitSha"], HEAD, "{out}");
    assert_eq!(out["files"], json!(["M +1 -1 src/a.rs"]), "{out}");
}

/// A compare of identical refs changes nothing: an empty answer (exit 1),
/// as a commit or PR whose scope holds nothing is.
#[tokio::test]
async fn compare_of_identical_refs_is_an_empty_answer() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/compare/{HEAD}...{HEAD}"),
        200,
        json!({"ahead_by":0,"behind_by":0,"total_commits":0,
            "base_commit":{"sha":HEAD},"merge_base_commit":{"sha":HEAD},
            "commits":[],"files":[]}),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","baseRef":HEAD,"headRef":HEAD}),
    )
    .await;
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(out["aheadBy"], 0, "{out}");
}

/// A comparison's `matchString` lists patchless files as unsearched too; a
/// removed one is searched at the merge base, where its text is.
#[tokio::test]
async fn compare_match_string_lists_patchless_files_as_unsearched() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/compare/{BASE}...{HEAD}"),
        200,
        json!({"ahead_by":1,"behind_by":0,"total_commits":1,
            "base_commit":{"sha":BASE},"merge_base_commit":{"sha":MERGE_BASE},
            "commits":[commit_raw(HEAD, "Add feature")],
            "files":[changed_file("src/a.rs"), changed_file("src/c.rs"), {
                "filename":"src/gone.rs","status":"removed","additions":0,"deletions":700,
                "sha":"9"}]}),
    )
    .await;
    let out = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD,
            "matchString":["new"]}),
    )
    .await;
    assert!(out.get("warnings").is_none(), "{out}");
    assert_eq!(
        out["unsearchedFiles"],
        json!(["!tooLarge src/gone.rs"]),
        "{out}"
    );
    assert_eq!(out["isPartial"], true, "{out}");
    assert_eq!(out["partialReasons"], json!(["patchUnavailable"]), "{out}");
    let search = &out["next"]["searchUnpatchedFile"]["input"]["queries"][0];
    assert_eq!(search["path"], "src/gone.rs", "{out}");
    assert_eq!(search["ref"], MERGE_BASE, "{out}");
}

/// A comparison whose range spans more commits than one commit page,
/// with 100 commits on its first page.
async fn mount_long_compare(server: &MockServer, files: Value) {
    let commits = (0..100)
        .map(|n| commit_raw(&format!("{n:040x}"), &format!("commit {n}")))
        .collect::<Vec<_>>();
    mount_json(
        server,
        format!("/api/v3/repos/o/r/compare/{BASE}...{HEAD}"),
        200,
        json!({"ahead_by":337,"behind_by":0,"total_commits":337,
            "base_commit":{"sha":BASE},"merge_base_commit":{"sha":MERGE_BASE},
            "commits":commits,"files":files}),
    )
    .await;
}

/// `include` or `matchString` asks about files: the comparison answers with
/// its file page, and the range's commits (not scoped by either filter)
/// are not listed by default. Their section states the count, `unshown`
/// with no page number, and `next.nextCommitPage` lists them from the
/// first page, losslessly. An unfiltered read still lists them.
#[tokio::test]
async fn compare_file_filters_page_the_commits_apart() {
    let server = MockServer::builder().start().await;
    mount_long_compare(
        &server,
        json!([changed_file("src/a.rs"), {"filename":"docs/b.md","status":"modified",
            "additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-x\n+y"}]),
    )
    .await;
    for filter in [
        json!({"include":["src/*.rs"]}),
        json!({"matchString":["new"]}),
    ] {
        let mut row =
            json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD});
        for (key, value) in filter.as_object().expect("filter") {
            row[key] = value.clone();
        }
        let out = read(&server, row).await;
        assert!(out.get("commits").is_none(), "{filter}: {out}");
        // `changedFilesCount` counts both files; the include says what it kept.
        assert_eq!(
            out["warnings"]
                .to_string()
                .contains("include kept 1 of 2 changed files."),
            filter.get("include").is_some(),
            "{filter}: {out}"
        );
        assert_eq!(
            out["contentPagination"]["commits"],
            json!({"totalPages":4,"pageSize":100,"totalItems":337,"hasMore":true,"unshown":true}),
            "{filter}: {out}"
        );
        let page = &out["next"]["nextCommitPage"]["input"]["queries"][0];
        assert_eq!(page["sections"], json!(["commits"]), "{filter}: {out}");
        assert!(page.get("page").is_none(), "{filter}: {out}");
        assert!(page.get("matchString").is_none(), "{filter}: {out}");
        assert_eq!(
            (&page["baseRef"], &page["headRef"]),
            (&json!(BASE), &json!(HEAD))
        );
        assert!(
            !out["files"].to_string().contains("docs/b.md"),
            "{filter}: {out}"
        );
        // The page it names lists the first commits.
        let listed = read(&server, page.clone()).await;
        assert_eq!(listed["commits"].as_array().map(Vec::len), Some(100));
        assert_eq!(listed["contentPagination"]["commits"]["page"], 1);
    }
    let plain = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD}),
    )
    .await;
    assert_eq!(
        plain["commits"].as_array().map(Vec::len),
        Some(100),
        "{plain}"
    );
    assert_eq!(
        plain["next"]["nextCommitPage"]["input"]["queries"][0]["page"],
        2
    );
}

/// A file GitHub sent without a patch is searched at the comparison head
/// with the same literal (`next.searchUnpatchedFile`, no ranges: where the
/// literal sits is unknown), and the comparison's other pages never
/// replace that read.
#[tokio::test]
async fn compare_unpatched_file_read_survives_the_comparison_pages() {
    let server = MockServer::builder().start().await;
    mount_long_compare(
        &server,
        json!([changed_file("crates/flags/lowargs.rs"), {
            "filename":"crates/flags/defs.rs","status":"modified","additions":4000,
            "deletions":3000,"sha":"9"}]),
    )
    .await;
    for sections in [None, Some(json!(["commits", "patches"]))] {
        let mut row = json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD,
            "include":["crates/flags/*.rs"],"matchString":["smart_case"],"caseMode":"smart","regexEngine":"literal","wholeWord":false});
        if let Some(sections) = &sections {
            row["sections"] = sections.clone();
        }
        let out = read(&server, row).await;
        assert_eq!(
            out["unsearchedFiles"],
            json!(["!tooLarge crates/flags/defs.rs"]),
            "{out}"
        );
        assert_eq!(
            out["next"]["searchUnpatchedFile"]["input"]["queries"][0],
            json!({"owner":"o","repo":"r","path":"crates/flags/defs.rs","ref":HEAD,
                "matchString":["smart_case"]}),
            "{sections:?}: {out}"
        );
        assert!(
            out["next"]["nextCommitPage"].is_object(),
            "{sections:?}: {out}"
        );
    }
}

/// A comparison whose head is behind its base changes nothing that way
/// round: the empty answer leads with the swapped comparison.
#[tokio::test]
async fn a_reversed_compare_leads_with_the_swapped_refs() {
    let server = MockServer::builder().start().await;
    // The head resolves to its commit before the comparison.
    mount_json(
        &server,
        format!("/api/v3/repos/o/r/compare/v2...{BASE}"),
        200,
        json!({"ahead_by":0,"behind_by":45,"total_commits":0,
            "base_commit":{"sha":HEAD},"merge_base_commit":{"sha":BASE},
            "commits":[],"files":[]}),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r/commits/v1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(BASE))
        .mount(&server)
        .await;
    let out = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","baseRef":"v2","headRef":"v1","pageSize":3}),
    )
    .await;
    assert_eq!(out["status"], "empty", "{out}");
    assert_eq!(out["behindBy"], 45, "{out}");
    let swap =
        crate::tools::test_support::checks::related_call(&out, "ghGetHistoryItem", |input| {
            input["queries"][0]["operation"] == "compare"
        });
    assert_eq!(swap["tool"], "ghGetHistoryItem", "{out}");
    assert_eq!(
        swap["input"]["queries"][0],
        // `pageSize` is private page state (`x-cursor`): a fresh lead drops it.
        json!({"operation":"compare","owner":"o","repo":"r","baseRef":"v1","headRef":"v2"}),
        "{out}"
    );
}
