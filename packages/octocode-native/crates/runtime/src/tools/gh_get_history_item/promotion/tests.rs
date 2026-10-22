use super::super::remaining::Remainder;
use super::*;

/// A commit's `continuePatch` copies the page-stream cursor named on
/// `contentPagination.files`, never a file's own per-file `nextOffset`.
#[test]
fn commit_continue_patch_copies_the_page_stream_cursor() {
    let query = HistoryItemRequest::from_row(json!({
        "operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
        "ref":"abc","sections":["patches"],"length":10
    }))
    .expect("commit query");
    let mut out = json!({
        "files":[{"filename":"a.rs","patch":"aaa"},{"filename":"b.rs","patch":"bbbbbbb",
            "patchPagination":{"unit":"chars","offset":0,"length":7,"total":20,"hasMore":true,"nextOffset":7}}],
        "contentPagination":{"files":{"currentPage":1,"hasMore":false}}
    });
    let cursors = DiffCursors::of_file_page(&mut out["contentPagination"]["files"], Some(10), true);
    attach_diff_continuations(
        &mut out,
        &query,
        ItemOperation::Commit,
        Some("abc"),
        cursors,
    );
    let next = &out["next"]["continuePatch"]["query"]["queries"][0];
    assert_eq!(next["offset"], 10, "{out}");
    assert_eq!(next["length"], 10);
}

/// A patch-walk hop (`continuePatch`, the next file page) runs
/// at the same automatic page as the first call: it carries no
/// `responseLength`/`responseScope` knob (automatic paging already
/// splits an oversized row by whole rows), and needs no offset marker.
#[test]
fn history_b_patch_walk_hops_carry_no_response_page_knobs() {
    let lead = |fields: Value, page: Option<usize>| {
        let mut row = json!({
            "operation":"pullRequest","mainGoal":"test","reasoning":"test",
            "owner":"a","repo":"b","number":1,"sections":["patches"]
        });
        for (key, value) in fields.as_object().into_iter().flatten() {
            row[key] = value.clone();
        }
        let mut query = HistoryItemRequest::from_row(row).expect("query");
        query.auto_page_chars = page;
        let mut out = json!({"pullRequests":[{"contentPagination":{
            "patches":{"hasMore":true,"nextOffset":900}
        }}]});
        promote_pr_continuations(&mut out, &query, &Remainder::default());
        out["next"]["continuePatch"]["query"].clone()
    };
    for hop in [
        lead(json!({}), Some(20_000)),
        lead(json!({"offset":5}), Some(40_000)),
        lead(json!({}), None),
    ] {
        assert!(hop.get("responseLength").is_none(), "{hop}");
        assert!(hop.get("responseScope").is_none(), "{hop}");
        assert_eq!(hop["queries"][0]["offset"], 900, "{hop}");
    }
    let hop = lead(json!({}), Some(5_000));
    assert!(hop["queries"][0].get("length").is_none(), "{hop}");
    crate::contracts::validate("ghGetHistoryItem", hop).expect("a valid hop");

    let mut commit = HistoryItemRequest::from_row(json!({
        "operation":"commit","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
        "ref":"abc","sections":["patches"]
    }))
    .expect("commit query");
    commit.auto_page_chars = Some(20_000);
    let mut out = json!({"contentPagination":{"files":{"currentPage":1,"hasMore":false}}});
    let cursors = DiffCursors::of_file_page(&mut out["contentPagination"]["files"], Some(10), true);
    attach_diff_continuations(
        &mut out,
        &commit,
        ItemOperation::Commit,
        Some("abc"),
        cursors,
    );
    let hop = &out["next"]["continuePatch"]["query"];
    assert!(hop.get("responseLength").is_none(), "{out}");
    assert!(hop.get("responseScope").is_none(), "{out}");

    let file_page = |sections: Value, offset: Option<u64>| {
        let mut row = json!({
            "operation":"pullRequest","mainGoal":"test","reasoning":"test",
            "owner":"a","repo":"b","number":1,"sections":sections
        });
        if let Some(offset) = offset {
            row["offset"] = json!(offset);
        }
        let mut query = HistoryItemRequest::from_row(row).expect("query");
        query.auto_page_chars = Some(40_000);
        let mut out = json!({"pullRequests":[{"contentPagination":{
            "files":{"hasMore":true,"nextPage":2}
        }}]});
        promote_pr_continuations(&mut out, &query, &Remainder::default());
        out["next"]["nextFilePage"]["query"].clone()
    };
    let walk = file_page(json!(["patches"]), Some(31_000));
    assert!(walk.get("responseLength").is_none(), "{walk}");
    assert!(walk.get("responseScope").is_none(), "{walk}");
    assert!(walk["queries"][0].get("offset").is_none(), "{walk}");
    assert_eq!(walk["queries"][0]["page"], 2, "{walk}");
    crate::contracts::validate("ghGetHistoryItem", walk).expect("a valid hop");
    let inventory = file_page(json!(["files"]), None);
    assert!(inventory.get("responseLength").is_none(), "{inventory}");
    assert!(inventory.get("responseScope").is_none(), "{inventory}");
    assert!(
        inventory["queries"][0].get("offset").is_none(),
        "{inventory}"
    );
}

#[test]
fn match_string_views_offer_the_whole_patches_they_narrowed() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
        "sections":["patches"],"matchString":"needle","offset":10,"page":2
    }))
    .expect("match query");
    let mut out = json!({"pullRequests":[{"files":[
        {"path":"src/a.rs","patch":"@@ -1,1 +1,1 @@\n+needle","fullPatchChars":900},
        {"path":"src/b.rs","patch":"+needle"}
    ]}]});
    attach_full_patch_continuation(&mut out, &query);
    let next = &out["next"]["readFullPatches"]["query"]["queries"][0];
    // The published spelling; patch hunks are never minified, so no
    // `minify` rides along.
    assert_eq!(next["sections"], json!(["patches"]), "{out}");
    assert_eq!(next["include"], json!(["src/a.rs"]), "{out}");
    for key in ["matchString", "offset", "page", "content", "minify"] {
        assert!(next.get(key).is_none(), "{key} kept: {next}");
    }
    // An explicit contextLines still clips long lines: the whole patch
    // stays reachable.
    let explicit: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","number":1,
        "sections":["patches"],"matchString":"needle","contextLines":0
    }))
    .expect("match query");
    let mut out = json!({"pullRequests":[{"files":[
        {"path":"src/a.rs","patch":"+needle","fullPatchChars":900}
    ]}]});
    attach_full_patch_continuation(&mut out, &explicit);
    let next = &out["next"]["readFullPatches"]["query"]["queries"][0];
    assert_eq!(next["include"], json!(["src/a.rs"]), "{out}");
    assert!(next.get("contextLines").is_none(), "{next}");
    assert!(out["next"].get("widenContext").is_none(), "{out}");
}

/// A cut comment body continues with the one text offset (`offset`),
/// on the same comment page, reading only comments: every pull-request
/// text surface windows with `offset`, as an issue's do.
#[test]
fn a_cut_comment_body_continues_with_offset_on_its_comment_page() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"a","repo":"b","number":1,
        "sections":["body","comments"],"page":2
    }))
    .expect("pr query");
    let mut out = json!({"pullRequests":[{"contentPagination":{
        "commentBody":{"hasMore":true,"nextOffset":500}
    }}]});
    promote_pr_continuations(&mut out, &query, &Remainder::default());
    let next = &out["next"]["continueCommentBody"]["query"]["queries"][0];
    assert_eq!(next["offset"], 500, "{out}");
    assert_eq!(next["page"], 2, "{out}");
    assert_eq!(next["sections"], json!(["comments"]), "{next}");
    assert!(next.get("commentOffset").is_none(), "{next}");
}

/// More reshaped files than one selection holds continue in numbered
/// reads that reach each file exactly once.
#[test]
fn reshaped_files_past_one_selection_continue_in_numbered_reads() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"a","repo":"b","number":1,
        "sections":["patches"],"matchString":"x"
    }))
    .expect("pr query");
    let rows = (0..selected_patch_files() + 3)
        .map(|i| json!({"path":format!("src/{i}.rs"),"patch":"...","fullPatchChars":90}))
        .collect::<Vec<_>>();
    let mut out = json!({"pullRequests":[{"files":rows}]});
    attach_full_patch_continuation(&mut out, &query);
    let files = |name: &str| {
        out["next"][name]["query"]["queries"][0]["include"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(
        files("readFullPatches").len(),
        selected_patch_files(),
        "{out}"
    );
    assert_eq!(
        files("readFullPatches2"),
        (selected_patch_files()..selected_patch_files() + 3)
            .map(|i| json!(format!("src/{i}.rs")))
            .collect::<Vec<_>>()
    );
    assert!(out["next"].get("readFullPatches3").is_none(), "{out}");
}

/// Many narrowed files get the whole-patch re-read; the default
/// context already holds 10 lines, so no narrower `widenContext` view is
/// offered, and the marker never reaches default rows.
#[test]
fn many_narrowed_files_read_full_patches_without_a_narrower_view() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"a","repo":"b","number":1,
        "matchString":"miri"
    }))
    .expect("match query");
    let rows = (0..6)
        .map(|i| json!({"path":format!("src/{i}.rs"),"patch":"+miri","fullPatchChars":90}))
        .collect::<Vec<_>>();
    let mut out = json!({"pullRequests":[{"files":rows}]});
    attach_full_patch_continuation(&mut out, &query);
    assert!(out["next"].get("widenContext").is_none(), "{out}");
    assert_eq!(
        out["next"]["readFullPatches"]["query"]["queries"][0]["include"]
            .as_array()
            .map(Vec::len),
        Some(6),
        "{out}"
    );
    crate::tools::test_support::checks::assert_absent(&out, "fullPatchChars");
}

/// The row already shows a merged PR's `mergeCommitSha`, so the menu
/// offers no separate merge-commit read beside it.
#[test]
fn merged_pull_requests_offer_no_merge_commit_read_beside_the_shown_sha() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"o","repo":"r","number":5
    }))
    .expect("pr query");
    for raw in [
        json!({"merged_at":"2026-09-26T15:24:18Z","merge_commit_sha":"facc6fc"}),
        json!({"merged_at":"2026-09-26T15:24:18Z","merge_commit_sha":"facc6fc",
            "changed_files":3,"additions":63,"deletions":4}),
        json!({"merged_at":null,"merge_commit_sha":"deadbee"}),
    ] {
        let menu = pr_next_menu(&query, &[], &raw);
        assert!(menu.get("readMergeCommit").is_none(), "{menu}");
    }
}

/// A merged PR whose patches were read offers its most-changed source
/// file at the merge commit: one read of every hunk's new-side lines,
/// padded and merged.
#[test]
fn merged_patch_reads_offer_the_changed_source_at_the_merge_commit() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"cli","repo":"cli","number":14429
    }))
    .expect("pr query");
    let merged = json!({"merged_at":"2026-09-11T15:55:40Z","merge_commit_sha":"8fcd6a6"});
    let file = |name: &str, additions: u64, deletions: u64, patch: &str| json!({"filename":name,"additions":additions,"deletions":deletions,"patch":patch});
    let files = vec![
        file(
            "acceptance/testdata/pr/merge.txtar",
            6,
            2,
            "@@ -33,2 +33,2 @@\n-# Merge\n+# Merge and delete",
        ),
        file(
            "pkg/cmd/pr/view.go",
            1,
            0,
            "@@ -9,1 +9,2 @@\n x\n+count := countItems(pr)",
        ),
        file(
            "pkg/cmd/pr/merge/merge.go",
            2,
            2,
            "@@ -589,7 +589,7 @@ func New() {\n \t\tdeleteBranch: opts.DeleteBranch,\n-\t\tcrossRepoPR: x,\n+\t\tcrossRepoPR:        pr.IsCrossRepository,\n@@ -600,3 +600,3 @@\n+\tok := true\n@@ -900,1 +900,1 @@\n+\tdone()",
        ),
        file(
            "pkg/cmd/pr/merge/merge_test.go",
            55,
            0,
            "@@ -1 +1 @@\n+func TestX() {}",
        ),
    ];
    let all = |_: &Value| true;
    let read = read_at_merge(&query, &merged, &files, all).expect("offer");
    assert_eq!(read["tool"], "ghGetFileContent");
    // The diff numbers `headSha`; the merge commit is searched
    // for one distinctive added line per hunk, never read by number.
    assert_eq!(
        read["query"],
        json!({"queries":[{"owner":"cli","repo":"cli","ref":"8fcd6a6",
            "path":"pkg/cmd/pr/merge/merge.go",
            "matchString":["crossRepoPR:        pr.IsCrossRepository,"]}]})
    );
    // An `include` read offers only a file it selected.
    let view = |file: &Value| file["filename"] == "pkg/cmd/pr/view.go";
    let read = read_at_merge(&query, &merged, &files, view).expect("selected offer");
    assert_eq!(read["query"]["queries"][0]["path"], "pkg/cmd/pr/view.go");
    let docs = |file: &Value| {
        file["filename"]
            .as_str()
            .is_some_and(|name| name.ends_with(".txtar"))
    };
    assert!(read_at_merge(&query, &merged, &files, docs).is_none());
    // Open PRs, files without patches, and patches with nothing added
    // offer nothing.
    let open = json!({"merged_at":null,"merge_commit_sha":"deadbee"});
    assert!(read_at_merge(&query, &open, &files, all).is_none());
    let patchless = [json!({"filename":"pkg/cmd/pr/merge/merge.go","additions":2,"deletions":2})];
    assert!(read_at_merge(&query, &merged, &patchless, all).is_none());
    let removed = [file("a.go", 0, 1, "@@ -1 +0,0 @@\n-x := 1")];
    assert!(read_at_merge(&query, &merged, &removed, all).is_none());
    // A fix of short lines only still locates them.
    let short = [file("a.go", 1, 0, "@@ -1 +1 @@\n+\tok := true")];
    let read = read_at_merge(&query, &merged, &short, all).expect("short offer");
    assert_eq!(
        read["query"]["queries"][0]["matchString"],
        json!(["ok := true"])
    );
    // Docs and tests are not the shipped fix: no code file, no offer.
    let no_code = [
        file(
            "acceptance/README.md",
            1,
            0,
            "@@ -1 +1 @@\n+Run the acceptance suite",
        ),
        file(
            "pkg/cmd/pr/merge/merge_test.go",
            1,
            0,
            "@@ -1 +1 @@\n+func TestX() {}",
        ),
    ];
    assert!(read_at_merge(&query, &merged, &no_code, all).is_none());
}

/// A change's top file reads whole at its new side (`readAtCommit`)
/// and at its old side over the hunks' old-side windows
/// (`readParent`): gutters number `-` lines on the old side, so parent
/// ranges come from `-a,b`, never from `+c,d`.
#[test]
fn change_reads_number_each_side_from_its_own_hunk_side() {
    let file = |name: &str, status: &str, patch: &str| json!({"filename":name,"status":status,"additions":2,"deletions":1,"patch":patch});
    let modified = file(
        "src/lib.rs",
        "modified",
        "@@ -40,3 +60,4 @@ fn f\n a\n-b\n+c\n+d\n e",
    );
    let sides = ChangeSides {
        new_ref: Some("c0ffee"),
        old_ref: Some("parent1"),
        old_confidence: crate::response::result::Confidence::High,
    };
    let reads = change_reads("o", "r", std::slice::from_ref(&modified), &sides, |_| true);
    let names = reads.iter().map(|(name, _)| *name).collect::<Vec<_>>();
    assert_eq!(names, ["readAtCommit", "readParent"]);
    assert_eq!(
        reads[0].1["query"],
        json!({"queries":[{"owner":"o","repo":"r","path":"src/lib.rs","ref":"c0ffee"}]})
    );
    assert_eq!(
        reads[1].1["query"],
        json!({"queries":[{"owner":"o","repo":"r","path":"src/lib.rs","ref":"parent1","ranges":["30-52"]}]})
    );
    // A rename reads its parent under the old path; an added file has
    // no parent side, a removed one no new side.
    let mut renamed = modified.clone();
    renamed["status"] = json!("renamed");
    renamed["previous_filename"] = json!("src/old.rs");
    let reads = change_reads("o", "r", &[renamed], &sides, |_| true);
    assert_eq!(reads[1].1["query"]["queries"][0]["path"], "src/old.rs");
    let added = file("src/new.rs", "added", "@@ -0,0 +1,2 @@\n+x\n+y");
    let reads = change_reads("o", "r", &[added], &sides, |_| true);
    assert_eq!(
        reads.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        ["readAtCommit"]
    );
    let removed = file("src/gone.rs", "removed", "@@ -1,2 +0,0 @@\n-x\n-y");
    let reads = change_reads("o", "r", &[removed], &sides, |_| true);
    assert_eq!(
        reads.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        ["readParent"]
    );
    assert_eq!(reads[0].1["query"]["queries"][0]["ranges"], json!(["1-2"]));
    // Without a parent ref only the new side is offered.
    let no_parent = ChangeSides {
        old_ref: None,
        ..sides
    };
    let reads = change_reads("o", "r", &[modified], &no_parent, |_| true);
    assert_eq!(
        reads.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        ["readAtCommit"]
    );
}

#[test]
fn pr_next_menu_carries_required_defaults_and_drops_cursors() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"o","repo":"r","number":5,
        "sections":["body"],"offset":100,"page":2,
        "reasoning":"r"
    }))
    .expect("query");
    let menu = pr_next_menu(&query, &["src/a.rs".into()], &json!({}));
    let reviews = &menu["readFiles"]["query"]["queries"][0];
    // pageSize has no contract default; an omitted one stays omitted.
    assert!(reviews.get("pageSize").is_none(), "{reviews}");
    // Menu reads use the flat spelling; text is verbatim, so no `minify`.
    assert!(reviews.get("minify").is_none(), "{reviews}");
    assert_eq!(reviews["sections"], json!(["files"]));
    assert!(reviews.get("content").is_none(), "{reviews}");
    for key in ["offset", "page"] {
        assert!(reviews.get(key).is_none(), "{key} leaked: {reviews}");
    }
    assert_eq!(reviews["mainGoal"], "test");
    assert_eq!(reviews["reasoning"], "r");
    assert!(menu.get("readBody").is_none());
}

#[test]
fn pr_next_menu_omits_entries_the_row_already_answers() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"r","owner":"o","repo":"r","number":1
    }))
    .expect("query");
    let names = |menu: &Value| {
        menu.as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    // Empty body, one small file, comments present: every patch reads in
    // one call, so no separate file list.
    let small = json!({"body":"","changed_files":1,"additions":6,"deletions":1,"comments":3});
    let menu = pr_next_menu(&query, &[], &small);
    assert_eq!(names(&menu), ["readPatches", "readDiscussion"]);
    assert_eq!(
        menu["readPatches"]["query"]["queries"][0]["sections"],
        json!(["patches"])
    );
    // A body rides the file list of a large diff.
    let large = json!({"body":"x","changed_files":40,
        "additions":900,"deletions":50,"comments":0,"review_comments":0});
    let menu = pr_next_menu(&query, &[], &large);
    assert_eq!(names(&menu), ["readFiles", "readDiscussion"]);
    assert_eq!(
        menu["readFiles"]["query"]["queries"][0]["sections"],
        json!(["body", "files"])
    );
    // Provably no comments: the discussion read asks for reviews only.
    assert_eq!(
        menu["readDiscussion"]["query"]["queries"][0]["sections"],
        json!(["reviews"])
    );
    // No changed files and a body: the body read stands alone.
    let empty = json!({"body":"y","changed_files":0,
        "comments":0,"review_comments":0});
    assert_eq!(
        names(&pr_next_menu(&query, &[], &empty)),
        ["readBody", "readDiscussion"]
    );
}

/// A menu holds at most [`MENU_CAP`] entries; an inventory read turns its
/// review pick into `readSelectedPatches` over several files.
#[test]
fn menus_hold_at_most_two_entries_and_inventory_reviews_many_files() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"r","owner":"o","repo":"r","number":1
    }))
    .expect("query");
    let merged = json!({"body":"x".repeat(900),"changed_files":656,"additions":282_700,
        "deletions":284_842,"comments":5,"merged_at":"2026-01-01T00:00:00Z","merge_commit_sha":"abc"});
    let menu = pr_next_menu(&query, &[], &merged);
    let keys = menu
        .as_object()
        .map(|m| m.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    assert_eq!(keys, ["readFiles", "readDiscussion"], "{menu}");
    crate::tools::test_support::checks::assert_absent(&menu, "<literal");
    let review = vec!["src/a.rs".to_owned(), "src/b.rs".to_owned()];
    let inventory = json!({"changed_files":30,"additions":900,"deletions":10});
    let inventory_read: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"r","owner":"o","repo":"r","number":1,
        "sections":["files"]
    }))
    .expect("inventory query");
    let menu = pr_next_menu(&inventory_read, &review, &inventory);
    assert_eq!(
        menu["readSelectedPatches"]["query"]["queries"][0]["sections"],
        json!(["patches"])
    );
    assert_eq!(
        menu["readSelectedPatches"]["query"]["queries"][0]["include"],
        json!(["src/a.rs", "src/b.rs"])
    );
    assert_eq!(menu["readSelectedPatches"]["confidence"], "high");
    assert!(menu.get("readPatches").is_some(), "{menu}");
    // The discussion read is the third candidate: it stays reachable
    // through `include`, not the menu.
    assert_eq!(menu.as_object().map(Map::len), Some(MENU_CAP), "{menu}");
    assert!(menu.get("readDiscussion").is_none(), "{menu}");
    // A pick covering every changed file is the every-patch read.
    let two = json!({"body":"","changed_files":2,"additions":900,"deletions":10});
    let menu = pr_next_menu(&inventory_read, &review, &two);
    assert_eq!(
        menu["readPatches"]["query"]["queries"][0]["sections"],
        json!(["patches"]),
        "{menu}"
    );
    assert!(
        menu["readPatches"]["query"]["queries"][0]
            .get("include")
            .is_none(),
        "{menu}"
    );
}

/// A patch hop is one row query (the documented `{queries:[next.query]}`
/// form), in the published spelling: the same file page at the
/// page-stream cursor. It carries no path list and no hidden window, so
/// every hop has the page budget of the first window.
#[test]
fn patch_hops_are_the_same_file_page_at_the_stream_cursor() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","owner":"o","repo":"r","number":5,"sections":["patches"],
        "page":2,"include":["src/**"]
    }))
    .expect("query");
    let mut out = json!({"pullRequests":[{"contentPagination":{
        "patches":{"hasMore":true,"nextOffset":900}
    }}]});
    let mut remainder = Remainder::default();
    remainder.unfinished_patches(2);
    let notes = promote_pr_continuations(&mut out, &query, &remainder);
    let hop = &out["next"]["continuePatch"]["query"]["queries"][0];
    assert!(hop.get("queries").is_none(), "{hop}");
    assert!(hop.get("content").is_none(), "{hop}");
    assert!(hop.get("minify").is_none(), "minify: {hop}");
    assert!(hop.get("length").is_none(), "{hop}");
    assert_eq!(hop["operation"], "pullRequest");
    assert_eq!(hop["sections"], json!(["patches"]), "{hop}");
    assert_eq!(hop["include"], json!(["src/**"]), "{hop}");
    assert_eq!(hop["page"], 2, "{hop}");
    assert_eq!(hop["offset"], 900, "{hop}");
    assert_eq!(
        notes,
        [crate::response::pages::Remaining {
            page: "continuePatch".into(),
            left: "2 unfinished patches".into()
        }]
    );
    crate::contracts::validate_query("ghGetHistoryItem", hop.clone())
        .expect("continuePatch is a valid ghGetHistoryItem query");
}

/// One canonical query per page: a file page with unread patches offers
/// only `continuePatch`; the window that finishes the page's patches
/// offers the next file page at the patch stream's start.
#[test]
fn the_next_file_page_follows_the_last_patch_window() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","owner":"o","repo":"r","number":5,"sections":["patches"]
    }))
    .expect("query");
    let mut open = json!({"pullRequests":[{"contentPagination":{
        "patches":{"hasMore":true,"nextOffset":900},
        "files":{"hasMore":true,"nextPage":2}
    }}]});
    promote_pr_continuations(&mut open, &query, &Remainder::default());
    let names = open["next"]
        .as_object()
        .map(|next| next.keys().cloned().collect::<Vec<_>>());
    assert_eq!(names, Some(vec!["continuePatch".to_owned()]), "{open}");

    let hop: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","owner":"o","repo":"r","number":5,"sections":["patches"],
        "offset":900
    }))
    .expect("hop");
    let mut last = json!({"pullRequests":[{"contentPagination":{
        "files":{"hasMore":true,"nextPage":2}
    }}]});
    promote_pr_continuations(&mut last, &hop, &Remainder::default());
    let names = last["next"]
        .as_object()
        .map(|next| next.keys().cloned().collect::<Vec<_>>());
    assert_eq!(names, Some(vec!["nextFilePage".to_owned()]), "{last}");
    let page = &last["next"]["nextFilePage"]["query"]["queries"][0];
    assert_eq!(page["page"], 2, "{page}");
    // The walk's next file page opens at the stream start, never at the
    // old cursor (no offset marker keeps the walk's page).
    assert!(page.get("offset").is_none(), "{page}");
}

/// Compare: each page has one canonical query. The commit page drops
/// every file cursor; the file page restarts the patch offset and follows
/// the last patch window; a patch hop re-offers neither.
#[test]
fn compare_pages_are_offered_once_with_their_own_cursor_only() {
    let compare = |fields: Value| {
        HistoryItemRequest::from_row(super::super::util::merge(
            json!({"operation":"compare","owner":"a","repo":"b","base":"x","head":"y",
                "sections":["patches"],"length":5000}),
            fields,
        ))
        .expect("compare query")
    };
    let mut first = json!({
        "commits":[{"sha":"1"}],
        "files":[{"path":"a.rs","patch":"x"}],
        "contentPagination":{"commits":{"currentPage":1,"hasMore":true,"nextPage":2},
            "files":{"currentPage":1,"hasMore":true,"nextPage":2}}
    });
    let cursors = DiffCursors {
        commit_page: Some(2),
        ..DiffCursors::of_file_page(&mut first["contentPagination"]["files"], Some(5000), true)
    };
    attach_diff_continuations(
        &mut first,
        &compare(json!({})),
        ItemOperation::Compare,
        None,
        cursors,
    );
    let next = first["next"].as_object().expect("next");
    assert!(next.get("nextFilePage").is_none(), "{first}");
    let commits = &first["next"]["nextPage"]["query"]["queries"][0];
    for key in ["page", "offset", "length"] {
        assert!(commits.get(key).is_none(), "{key}: {commits}");
    }
    assert_eq!(
        first["next"]["continuePatch"]["query"]["queries"][0]["offset"],
        5000
    );

    let mut last = json!({
        "files":[{"path":"b.rs","patch":"y"}],
        "contentPagination":{"files":{"currentPage":1,"hasMore":true,"nextPage":2}}
    });
    let cursors = DiffCursors::of_file_page(&mut last["contentPagination"]["files"], None, true);
    attach_diff_continuations(
        &mut last,
        &compare(json!({"offset":5000})),
        ItemOperation::Compare,
        None,
        cursors,
    );
    let names = last["next"]
        .as_object()
        .map(|next| next.keys().cloned().collect::<Vec<_>>());
    assert_eq!(names, Some(vec!["nextFilePage".to_owned()]), "{last}");
    let files = &last["next"]["nextFilePage"]["query"]["queries"][0];
    assert_eq!(files["page"], 2, "{files}");
    assert!(files.get("offset").is_none(), "{files}");
}

#[test]
fn char_offset_continuations_narrow_content_to_their_own_surface() {
    // `offset` is one shared field: continuing the body must not skew
    // review bodies or patches by the body offset (and vice versa).
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","mainGoal": "test", "reasoning":"test","owner":"o","repo":"r","number":5,
        "sections":["body","patches","comments","reviews"],
        "offset":0
    }))
    .expect("query");
    let mut out = json!({"pullRequests":[{"contentPagination":{
        "body":{"hasMore":true,"nextOffset":12000},
        "reviewBody":{"hasMore":true,"nextOffset":300},
        "patches":{"hasMore":true,"nextOffset":900},
        "comments":{"hasMore":true,"nextPage":2}
    }}]});
    promote_pr_continuations(&mut out, &query, &Remainder::default());
    let next = &out["next"];
    assert_eq!(
        next["continueBody"]["query"]["queries"][0]["sections"],
        json!(["body"])
    );
    assert_eq!(next["continueBody"]["query"]["queries"][0]["offset"], 12000);
    assert_eq!(
        next["continueReviewBody"]["query"]["queries"][0]["sections"],
        json!(["reviews"])
    );
    assert_eq!(
        next["continuePatch"]["query"]["queries"][0]["sections"],
        json!(["patches"])
    );
    assert!(
        next["continuePatch"]["query"]["queries"][0]
            .get("content")
            .is_none()
    );
    let comments = &next["nextCommentPage"]["query"]["queries"][0];
    assert!(comments.get("offset").is_none(), "{comments}");
    assert_eq!(comments["page"], 2);
}

/// Issue pages use the published `include` spelling.
#[test]
fn issue_pages_use_the_published_include_spelling() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"issue","owner":"o","repo":"r","number":3,
        "sections":["body","comments"]
    }))
    .expect("issue query");
    let mut out = json!({"issues":[{"number":3,"contentPagination":{
        "body":{"hasMore":true,"nextOffset":12000},
        "comments":{"hasMore":true,"nextPage":2}
    }}]});
    promote_issue_continuations(&mut out, &query, &Remainder::default());
    let body = &out["next"]["continueBody"]["query"]["queries"][0];
    assert_eq!(body["sections"], json!(["body"]), "{out}");
    assert!(body.get("content").is_none(), "{out}");
    let comments = &out["next"]["nextCommentPage"]["query"]["queries"][0];
    assert_eq!(comments["sections"], json!(["comments"]), "{out}");
    assert!(comments.get("content").is_none(), "{out}");
}

/// A budget-sized file page (no `pageSize`, no patches) carries the page
/// size its grid was cut on in `nextFilePage`, so the continuation reads
/// the same grid under any batch share; a caller's `pageSize` and a patch
/// page (fixed batch) carry nothing new. `readPatches` from an inventory
/// page opens the patch page that holds the page's first file.
#[test]
fn budget_sized_file_pages_pin_their_grid_on_the_continuation() {
    let request = |row: Value, share: usize| {
        let mut query = HistoryItemRequest::from_row(row).expect("query");
        query.auto_page_chars = Some(share);
        query
    };
    let pr_next = |query: &HistoryItemRequest| {
        let mut out = json!({"pullRequests":[{"contentPagination":{
            "files":{"hasMore":true,"nextPage":2}
        }}]});
        promote_pr_continuations(&mut out, query, &Remainder::default());
        out["next"]["nextFilePage"]["query"]["queries"][0].clone()
    };
    let pr = |fields: Value| {
        super::super::util::merge(
            json!({"operation":"pullRequest","owner":"a","repo":"b","number":1}),
            fields,
        )
    };
    let alone = pr_next(&request(pr(json!({"sections":["files"]})), 20_000));
    let batched = pr_next(&request(pr(json!({"sections":["files"]})), 10_000));
    assert_eq!(alone["pageSize"], 333, "{alone}");
    assert_eq!(batched["pageSize"], 166, "{batched}");
    // Replayed in another batch, the pinned size is the caller's now.
    let replay = request(batched.clone(), 20_000);
    assert_eq!(super::super::inventory::file_page_size(&replay, false), 166);
    assert_eq!(pr_next(&replay)["pageSize"], 166);
    let patches = pr_next(&request(pr(json!({"sections":["patches"]})), 10_000));
    assert!(patches.get("pageSize").is_none(), "{patches}");
    let sized = pr_next(&request(
        pr(json!({"sections":["files"],"pageSize":50})),
        10_000,
    ));
    assert_eq!(sized["pageSize"], 50, "{sized}");

    let compare = |page: u64, share: usize| {
        let query = request(
            json!({"operation":"compare","owner":"a","repo":"b","base":"x","head":"y","page":page}),
            share,
        );
        let mut out = json!({"files":["M +1 -1 a.rs"],
            "contentPagination":{"files":{"currentPage":page,"hasMore":true,"nextPage":page + 1}}});
        let cursors = DiffCursors::of_file_page(&mut out["contentPagination"]["files"], None, true);
        attach_diff_continuations(&mut out, &query, ItemOperation::Compare, None, cursors);
        out
    };
    let out = compare(2, 10_000);
    let files = &out["next"]["nextFilePage"]["query"]["queries"][0];
    assert_eq!(
        (&files["page"], &files["pageSize"]),
        (&json!(3), &json!(166)),
        "{out}"
    );
    // Inventory page 2 (files 166..332) starts on patch page 2 (100..200).
    let patches = &out["next"]["readPatches"]["query"]["queries"][0];
    assert_eq!(patches["page"], 2, "{out}");
    assert!(patches.get("pageSize").is_none(), "{out}");
    let first = compare(1, 10_000);
    let patches = &first["next"]["readPatches"]["query"]["queries"][0];
    assert!(patches.get("page").is_none(), "{first}");
}

/// A page block's cursor rides the `next` continuation it was read into,
/// never the block, and `hasMore` beside that continuation already says the
/// rest is unread: `isPartial` is set only for an axis no continuation
/// reaches.
#[test]
fn cursors_ride_next_and_is_partial_marks_only_unreached_axes() {
    let pr = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","owner":"a","repo":"b","number":1,
        "sections":["body","files"]
    }))
    .expect("query");
    let mut out = json!({"pullRequests":[{"contentPagination":{
        "body":{"offset":0,"length":10,"total":30,"hasMore":true,"nextOffset":10},
        "files":{"currentPage":1,"hasMore":true,"nextPage":2}
    }}]});
    promote_pr_continuations(&mut out, &pr, &Remainder::default());
    assert!(out.get("isPartial").is_none(), "{out}");
    assert_eq!(
        out["next"]["nextFilePage"]["query"]["queries"][0]["page"],
        2
    );
    assert_eq!(
        out["next"]["continueBody"]["query"]["queries"][0]["offset"],
        10
    );
    let pages = &out["pullRequests"][0]["contentPagination"];
    assert!(pages["body"].get("nextOffset").is_none(), "{out}");
    assert!(pages["files"].get("nextPage").is_none(), "{out}");
    assert_eq!(pages["files"]["hasMore"], true, "{out}");
    // More to read and no cursor: nothing reaches it.
    let mut out = json!({"pullRequests":[{"contentPagination":{
        "files":{"currentPage":1,"hasMore":true}
    }}]});
    promote_pr_continuations(&mut out, &pr, &Remainder::default());
    assert_eq!(out["isPartial"], true, "{out}");
    assert!(out.get("next").is_none(), "{out}");

    let issue = HistoryItemRequest::from_row(json!({
        "operation":"issue","owner":"a","repo":"b","number":1,"sections":["comments"]
    }))
    .expect("query");
    let mut out = json!({"issues":[{"contentPagination":{
        "comments":{"currentPage":1,"hasMore":true,"nextPage":2}
    }}]});
    promote_issue_continuations(&mut out, &issue, &Remainder::default());
    assert!(out.get("isPartial").is_none(), "{out}");
    assert_eq!(
        out["next"]["nextCommentPage"]["query"]["queries"][0]["page"],
        2
    );
    assert!(
        out["issues"][0]["contentPagination"]["comments"]
            .get("nextPage")
            .is_none(),
        "{out}"
    );
}
