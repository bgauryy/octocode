//! Change reads: merged pull requests lead to the changed source at the merge commit, each side numbered from its own hunk side.
use super::*;

/// The row already shows a merged PR's `mergeCommitSha`, so the menu
/// offers no separate merge-commit read beside it.
#[test]
fn merged_pull_requests_offer_no_merge_commit_read_beside_the_shown_sha() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest", "owner":"o","repo":"r","number":5
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

/// A change's top file reads, in one call, over the hunks' new-side windows
/// at its new side (from `+c,d`) and over the old-side windows at its old
/// side (from `-a,b`): gutters number `-` lines on the old
/// side. A side with no lines in the diff (a pure addition has no old side,
/// a pure removal no new side) has no read: a patched change never leads to
/// a whole-file read.
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
        needle: None,
    };
    let reads = change_reads("o", "r", std::slice::from_ref(&modified), &sides, |_| true);
    let names = reads
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["readAtCommit"], "one call reads both sides");
    assert_eq!(
        reads[0].1["input"],
        json!({"queries":[
            {"owner":"o","repo":"r","path":"src/lib.rs","ref":"c0ffee","ranges":[{"startLine": 50, "endLine": 73}]},
            {"owner":"o","repo":"r","path":"src/lib.rs","ref":"parent1","ranges":[{"startLine": 30, "endLine": 52}]}]})
    );
    for row in reads[0].1["input"]["queries"].as_array().expect("rows") {
        assert_eq!(
            crate::response::result::read_view(crate::id::ToolId::GhGetFileContent, row),
            octocode_contract_types::ReadView::Content
        );
    }
    // A rename reads its parent under the old path; an added file has
    // no parent side, a removed one no new side.
    let mut renamed = modified.clone();
    renamed["status"] = json!("renamed");
    renamed["previous_filename"] = json!("src/old.rs");
    let reads = change_reads("o", "r", &[renamed], &sides, |_| true);
    assert_eq!(reads[0].1["input"]["queries"][1]["path"], "src/old.rs");
    let added = file("src/new.rs", "added", "@@ -0,0 +1,2 @@\n+x\n+y");
    let reads = change_reads("o", "r", &[added], &sides, |_| true);
    assert_eq!(
        reads
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["readAtCommit"]
    );
    assert_eq!(
        reads[0].1["input"]["queries"][0]["ranges"],
        json!([{"startLine":1,"endLine":2}])
    );
    // A modified file that only adds lines has no old side to read; one
    // that only removes lines has no new side.
    let adds = file("src/adds.rs", "modified", "@@ -5,0 +6,2 @@\n+x\n+y");
    let reads = change_reads("o", "r", &[adds], &sides, |_| true);
    assert_eq!(
        reads
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["readAtCommit"]
    );
    let drops = file("src/drops.rs", "modified", "@@ -5,2 +4,0 @@\n-x\n-y");
    let reads = change_reads("o", "r", &[drops], &sides, |_| true);
    assert_eq!(
        reads
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["readParent"]
    );
    let removed = file("src/gone.rs", "removed", "@@ -1,2 +0,0 @@\n-x\n-y");
    let reads = change_reads("o", "r", &[removed], &sides, |_| true);
    assert_eq!(
        reads
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["readParent"]
    );
    assert_eq!(
        reads[0].1["input"]["queries"][0]["ranges"],
        json!([{"startLine":1,"endLine":2}])
    );
    // Without a parent ref only the new side is offered.
    let no_parent = ChangeSides {
        old_ref: None,
        ..sides
    };
    let reads = change_reads("o", "r", &[modified], &no_parent, |_| true);
    assert_eq!(
        reads
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["readAtCommit"]
    );
}

/// After a `matchString` the pick is the file with the most hits (not the
/// most churn), and its windows cover the hunks that hold the literal only.
#[test]
fn a_literal_picks_the_most_hit_file_and_reads_its_hit_hunks() {
    let request = HistoryItemRequest::from_row(json!({
        "operation":"pullRequest","owner":"o","repo":"r","number":5,"matchString":["needle"]
    }))
    .expect("pr query");
    let needle = request.matcher.as_deref();
    let file = |name: &str, additions: u64, patch: &str| json!({"filename":name,"status":"modified","additions":additions,"deletions":1,"patch":patch});
    // `a.rs` churns most and has one hit; `b.rs` has two hits, in its second hunk.
    let files = [
        file("src/a.rs", 50, "@@ -1,3 +1,3 @@\n a\n-x\n+needle\n c"),
        file(
            "src/b.rs",
            2,
            "@@ -1,3 +1,3 @@\n a\n-x\n+y\n c\n@@ -100,3 +100,4 @@\n a\n+needle one\n+needle two\n c",
        ),
    ];
    let sides = ChangeSides {
        new_ref: Some("head"),
        old_ref: None,
        needle,
    };
    let reads = change_reads("o", "r", &files, &sides, |_| true);
    let row = &reads[0].1["input"]["queries"][0];
    assert_eq!(row["path"], "src/b.rs", "{reads:?}");
    let ranges = row["ranges"].as_array().expect("ranges");
    assert_eq!(ranges.len(), 1, "only the hit hunk is read: {row}");
    assert!(ranges[0]["startLine"].as_u64() >= Some(90), "{row}");
    // Without the literal the most-changed file leads, over both hunks.
    let sides = ChangeSides {
        needle: None,
        ..sides
    };
    let reads = change_reads("o", "r", &files, &sides, |_| true);
    assert_eq!(reads[0].1["input"]["queries"][0]["path"], "src/a.rs");
}
