//! Change reads: merged pull requests lead to the changed source at the merge commit, each side numbered from its own hunk side.
use super::*;

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
