//! Compare rows: both resolved commits, empty and reversed ranges, file filters and unpatched reads across comparison pages.
use super::*;
use octocode_test_support::json_fixture;

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
    let query = &octocode_runtime_base::test_support::checks::related_call(
        &out,
        "ghGetHistoryItem",
        |input| input["queries"][0].get("matchString").is_none(),
    )["input"]["queries"][0];
    assert_eq!(query["operation"], "compare", "{out}");
    assert!(query.get("matchString").is_none(), "{query}");
    assert_eq!(
        (&query["baseRef"], &query["headRef"]),
        (&json!(BASE), &json!(HEAD))
    );
    // The literal alone chose the patches: without it, the read lists the
    // files (as the warning says).
    assert_eq!(query["sections"], json_fixture(r#"["files"]"#), "{query}");
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
    // The comparison as asked (`v1`) and as its continuations pin it.
    for base in ["v1", BASE] {
        mount_json(
            &server,
            format!("/api/v3/repos/o/r/compare/{base}...{HEAD}"),
            200,
            json!({"ahead_by":1,"behind_by":0,"total_commits":1,
                "permalink_url":"https://github.com/o/r/compare/o:1111111...o:2222222",
                "base_commit":{"sha":BASE},"merge_base_commit":{"sha":MERGE_BASE},
                "commits":[commit_raw(HEAD, "Add feature")],
                "files":[changed_file("src/a.rs")]}),
        )
        .await;
    }
    let out = read(
        &server,
        json_fixture(
            r#"{
                "operation": "compare",
                "owner": "o",
                "repo": "r",
                "baseRef": "v1",
                "headRef": "feature"
            }"#,
        ),
    )
    .await;
    // `base`/`head` carry the resolved SHAs, never the query's ref names.
    assert_eq!(out["baseCommitSha"], BASE, "{out}");
    assert_eq!(out["headCommitSha"], HEAD, "{out}");
    assert!(out.get("commitsScope").is_none(), "{out}");
    // The summary lists neither files nor commits; its files page is read
    // at the same two commits, and the commits section lists each by its SHA.
    assert!(
        out.get("files").is_none() && out.get("commits").is_none(),
        "{out}"
    );
    let files = read(&server, out["next"]["files"]["input"]["queries"][0].clone()).await;
    assert_eq!(
        files["files"],
        json_fixture(r#"["M +1 -1 src/a.rs"]"#),
        "{files}"
    );
    assert_eq!(
        (&files["baseCommitSha"], &files["headCommitSha"]),
        (&out["baseCommitSha"], &out["headCommitSha"]),
        "{files}"
    );
    let listed = read(
        &server,
        json_fixture(
            r#"{
                "operation": "compare",
                "owner": "o",
                "repo": "r",
                "baseRef": "v1",
                "headRef": "feature",
                "sections": ["commits"]
            }"#,
        ),
    )
    .await;
    // A plain commit row is one line that opens with its SHA.
    assert!(
        listed["commits"][0]
            .as_str()
            .is_some_and(|line| line.starts_with(&format!("{HEAD} "))),
        "{listed}"
    );
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
        json_fixture(r#"["!tooLarge src/gone.rs"]"#),
        "{out}"
    );
    assert_eq!(out["isPartial"], true, "{out}");
    assert_eq!(
        out["partialReasons"],
        json_fixture(r#"["patchUnavailable"]"#),
        "{out}"
    );
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
/// with no page number, and `sectionPagination.commits.next.nextPage` lists them from the
/// first page, losslessly. The commits section lists the first commit page,
/// and its continuation names the second.
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
        json_fixture(r#"{"include": ["src/*.rs"]}"#),
        json_fixture(r#"{"matchString": ["new"]}"#),
    ] {
        let mut row =
            json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD});
        for (key, value) in filter.as_object().expect("filter") {
            row[key] = value.clone();
        }
        let out = read(&server, row).await;
        assert!(out.get("commits").is_none(), "{filter}: {out}");
        // `changedFileCount` counts both files; the include says what it kept.
        assert_eq!(
            out["warnings"]
                .to_string()
                .contains("include kept 1 of 2 changed files."),
            filter.get("include").is_some(),
            "{filter}: {out}"
        );
        assert!(out["next"]["commits"].is_object(), "{out}");
        // No commit is shown: `aheadBy` counts the range's commits once, and
        // no empty page block repeats it.
        assert_eq!(out["aheadBy"], 337, "{filter}: {out}");
        assert!(
            out["sectionPagination"].get("commits").is_none(),
            "{filter}: {out}"
        );
        let page = &out["next"]["commits"]["input"]["queries"][0];
        assert_eq!(
            page["sections"],
            json_fixture(r#"["commits"]"#),
            "{filter}: {out}"
        );
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
        assert_eq!(listed["sectionPagination"]["commits"]["offset"], 0);
    }
    let plain = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD,
            "sections":["commits"]}),
    )
    .await;
    assert_eq!(
        plain["commits"].as_array().map(Vec::len),
        Some(100),
        "{plain}"
    );
    assert_eq!(plain["next"]["commits"]["input"]["queries"][0]["page"], 2);
}

/// A comparison read with no `sections` (and no file filter or literal) is
/// the summary row: both commits, how far each side is ahead, and the sizes
/// of its two lists (`aheadBy`, `changedFileCount`, each stated once: no
/// empty page block repeats them), with neither list in the row. Each
/// list's continuation reads it from its first item, so the summary and its
/// two walks hold every file and commit once.
#[tokio::test]
async fn compare_without_sections_is_the_summary_row() {
    let server = MockServer::builder().start().await;
    let files = json!([changed_file("src/a.rs"), changed_file("docs/b.md")]);
    let changed = files.as_array().map_or(0, Vec::len);
    mount_long_compare(&server, files).await;
    let asked = json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD});
    let summary = read(&server, asked.clone()).await;
    assert!(
        summary.get("files").is_none() && summary.get("commits").is_none(),
        "{summary}"
    );
    assert_eq!(
        (&summary["baseCommitSha"], &summary["headCommitSha"]),
        (&json!(BASE), &json!(HEAD)),
        "{summary}"
    );
    assert_eq!(summary["aheadBy"], 337, "{summary}");
    assert_eq!(summary["changedFileCount"], changed, "{summary}");
    assert!(summary.get("sectionPagination").is_none(), "{summary}");
    for section in ["files", "commits"] {
        let first = &summary["next"][section]["input"]["queries"][0];
        assert_eq!(first["sections"], json!([section]), "{summary}");
        assert!(first.get("page").is_none(), "{summary}");
        let listed = read(&server, first.clone()).await;
        assert_eq!(
            listed["sectionPagination"][section]["offset"], 0,
            "{listed}"
        );
        assert!(
            listed[section]
                .as_array()
                .is_some_and(|rows| !rows.is_empty()),
            "{listed}"
        );
    }
    let listed = read(&server, {
        let mut both = asked;
        both["sections"] = json!(["commits", "files"]);
        both
    })
    .await;
    assert!(
        summary.to_string().len() < listed.to_string().len() / 4,
        "the summary holds no list: {summary}"
    );
}

/// At GitHub's file cap the summary's file count is a floor: the files
/// section says so and the row claims no `changedFileCount`.
#[tokio::test]
async fn a_capped_compare_summary_claims_no_file_count() {
    let server = MockServer::builder().start().await;
    let cap = crate::gh_get_history_item::window::COMPARE_FILE_LIMIT;
    let files = (0..cap)
        .map(|n| changed_file(&format!("src/f{n}.rs")))
        .collect::<Vec<_>>();
    mount_long_compare(&server, json!(files)).await;
    let summary = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD}),
    )
    .await;
    assert!(summary.get("changedFileCount").is_none(), "{summary}");
    let page = &summary["sectionPagination"]["files"];
    assert_eq!(page["countScope"], "partial", "{summary}");
    assert_eq!(page["total"], cap, "{summary}");
    assert!(summary["next"]["files"].is_object(), "{summary}");
    assert!(summary.get("files").is_none(), "{summary}");
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
    for sections in [None, Some(json_fixture(r#"["commits", "patches"]"#))] {
        let mut row = json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD,
            "include":["crates/flags/*.rs"],"matchString":["smart_case"],"regexEngine":"literal"});
        if let Some(sections) = &sections {
            row["sections"] = sections.clone();
        }
        let out = read(&server, row).await;
        assert_eq!(
            out["unsearchedFiles"],
            json_fixture(r#"["!tooLarge crates/flags/defs.rs"]"#),
            "{out}"
        );
        let search = &out["next"]["searchUnpatchedFile"]["input"]["queries"][0];
        assert_eq!(
            *search,
            json!({"owner":"o","repo":"r","path":"crates/flags/defs.rs","ref":HEAD,
                "matchString":["smart_case"]}),
            "{sections:?}: {out}"
        );
        assert_eq!(
            octocode_tools_shared::test_support::read_view(
                octocode_runtime_base::id::ToolId::GhGetFileContent,
                search
            ),
            octocode_contract_types::ReadView::Content
        );
        assert!(out["next"]["commits"].is_object(), "{sections:?}: {out}");
    }
}

/// A comparison whose head is behind its base changes nothing that way
/// round: the answer says so (it is not an empty row), tells how to compare
/// the other way, and names no call it does not carry.
#[tokio::test]
async fn a_reversed_compare_says_how_to_compare_the_other_way() {
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
        json_fixture(
            r#"{
                "operation": "compare",
                "owner": "o",
                "repo": "r",
                "baseRef": "v2",
                "headRef": "v1",
                "pageSize": 3
            }"#,
        ),
    )
    .await;
    assert!(out.get("status").is_none(), "{out}");
    let warnings = out["warnings"].as_array().expect("warnings");
    assert_eq!(warnings.len(), 1, "{out}");
    let warning = warnings[0].as_str().unwrap_or_default();
    assert!(warning.contains("behind base by 45"), "{out}");
    assert!(!warning.contains("related"), "{out}");
    assert!(out.get("related").is_none(), "{out}");
    assert_eq!(out["behindBy"], 45, "{out}");
}

/// A scoped path's own history is the lead only while GitHub's compare file
/// cap stands (the trees could not extend it): a whole file list already
/// answers the scope. Each `include` literal prefix gets its own row, once,
/// and a prefix inside another's directory adds none.
#[tokio::test]
async fn a_scope_history_lead_only_beside_the_compare_file_cap() {
    let scoped = json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,
        "headRef":HEAD,"include":["src/**","lib/b.rs","src/x/**","src/**"]});
    let whole = MockServer::builder().start().await;
    mount_long_compare(
        &whole,
        json!([changed_file("src/a.rs"), changed_file("lib/b.rs")]),
    )
    .await;
    let out = read(&whole, scoped.clone()).await;
    assert!(
        checks::find_related_call(&out, "ghSearchHistory", |_| true).is_none(),
        "{out}"
    );
    let capped = MockServer::builder().start().await;
    let cap = crate::gh_get_history_item::window::COMPARE_FILE_LIMIT;
    let files = (0..cap)
        .map(|n| changed_file(&format!("src/f{n}.rs")))
        .collect::<Vec<_>>();
    mount_long_compare(&capped, json!(files)).await;
    let out = read(&capped, scoped).await;
    let lead = checks::related_call(&out, "ghSearchHistory", |_| true);
    let rows = lead["input"]["queries"].as_array().expect("rows");
    let paths = rows
        .iter()
        .map(|row| row["path"].clone())
        .collect::<Vec<_>>();
    assert_eq!(paths, [json!("src/"), json!("lib/b.rs")], "{out}");
    for row in rows {
        assert_eq!(row["ref"], HEAD, "{row}");
        assert_eq!(row["operation"], "commit", "{row}");
    }
}

/// Past GitHub's compare cap the trees name the other changed paths, rows
/// without line counts or patches. The source is said once, when the read's
/// scope keeps such a row: a scope of listed files only gets no word of it.
#[tokio::test]
async fn the_tree_source_is_said_only_when_the_scope_keeps_a_tree_row() {
    let server = MockServer::builder().start().await;
    let cap = crate::gh_get_history_item::window::COMPARE_FILE_LIMIT;
    let files = (0..cap)
        .map(|n| changed_file(&format!("src/f{n}.rs")))
        .collect::<Vec<_>>();
    mount_long_compare(&server, json!(files)).await;
    let entry =
        |path: &str, sha: &str| json!({"path":path,"type":"blob","mode":"100644","sha":sha});
    let tree = |sha: &str, extra: &str| {
        let mut entries = (0..cap)
            .map(|n| entry(&format!("src/f{n}.rs"), sha))
            .collect::<Vec<_>>();
        entries.push(entry(extra, &"5".repeat(40)));
        json!({"sha":sha,"truncated":false,"tree":entries})
    };
    for (sha, extra) in [(MERGE_BASE, "lib/old.rs"), (HEAD, "lib/new.rs")] {
        mount_json(
            &server,
            format!("/api/v3/repos/o/r/git/trees/{sha}"),
            200,
            tree(sha, extra),
        )
        .await;
    }
    let says = |out: &Value| out["warnings"].to_string().contains("commits' trees");
    let read_scope = |include: Value| {
        let server = &server;
        async move {
            read(
                server,
                json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,
                    "headRef":HEAD,"include":include}),
            )
            .await
        }
    };
    let listed = read_scope(json!(["src/f1.rs"])).await;
    assert!(!says(&listed), "{listed}");
    let tree_rows = read_scope(json!(["lib/**"])).await;
    assert!(says(&tree_rows), "{tree_rows}");
    let all = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD,
            "sections":["files"]}),
    )
    .await;
    assert!(says(&all), "{all}");
}

/// A files section that lists every changed file still states how many there
/// are: rows group by directory, so a claim cites `changedFileCount`, never a
/// tally of the rows.
#[tokio::test]
async fn a_compare_listing_every_file_states_its_count() {
    let server = MockServer::builder().start().await;
    mount_long_compare(
        &server,
        json!([
            changed_file("src/a.rs"),
            changed_file("src/b.rs"),
            changed_file("docs/c.md")
        ]),
    )
    .await;
    let listed = read(
        &server,
        json!({"operation":"compare","owner":"o","repo":"r","baseRef":BASE,"headRef":HEAD,
            "sections":["files"]}),
    )
    .await;
    assert_eq!(listed["changedFileCount"], 3, "{listed}");
}
