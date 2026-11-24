//! Change reads: merged pull requests offer no merge-commit read; a patchless change reads each side it has.
use super::*;
use octocode_test_support::json_fixture;

/// The row already shows a merged PR's `mergeCommitSha`, so the menu
/// offers no separate merge-commit read beside it.
#[test]
fn merged_pull_requests_offer_no_merge_commit_read_beside_the_shown_sha() {
    let query: HistoryItemRequest = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "number": 5
        }"#,
    ))
    .expect("pr query");
    for raw in [
        json_fixture(
            r#"{
                "merged_at": "2026-09-26T15:24:18Z",
                "merge_commit_sha": "facc6fc"
            }"#,
        ),
        json_fixture(
            r#"{
                "merged_at": "2026-09-26T15:24:18Z",
                "merge_commit_sha": "facc6fc",
                "changed_files": 3,
                "additions": 63,
                "deletions": 4
            }"#,
        ),
        json_fixture(r#"{"merged_at": null, "merge_commit_sha": "deadbee"}"#),
    ] {
        let menu = pr_next_menu(&query, &[], &raw);
        assert!(menu.get("readMergeCommit").is_none(), "{menu}");
    }
}

/// A patchless change reads whole at each side it has: the new side at
/// the change's commit, the old side at its parent under the pre-rename
/// path. An added file has no parent side, a removed one no new side, and
/// without a parent ref only the new side is read.
#[test]
fn a_patchless_change_reads_each_side_it_has() {
    let file = |name: &str, status: &str| {
        json!({"filename":name,"status":status,"additions":9000,"deletions":1,
            "sha":"1111111111111111111111111111111111111111"})
    };
    let sides = ChangeSides {
        new_ref: Some("c0ffee"),
        old_ref: Some("parent1"),
    };
    let read = |file: Value, sides: &ChangeSides<'_>| {
        change_reads("o", "r", &[file], sides, |_| true)
            .into_iter()
            .map(|(name, call)| {
                let row = &call["input"]["queries"][0];
                (name, row["path"].clone(), row["ref"].clone())
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        read(file("src/lib.rs", "modified"), &sides),
        [
            (
                "readAtCommit".to_owned(),
                json!("src/lib.rs"),
                json!("c0ffee")
            ),
            (
                "readParent".to_owned(),
                json!("src/lib.rs"),
                json!("parent1")
            ),
        ]
    );
    let mut renamed = file("src/lib.rs", "renamed");
    renamed["previous_filename"] = json!("src/old.rs");
    assert_eq!(read(renamed, &sides)[1].1, "src/old.rs");
    assert_eq!(
        read(file("src/new.rs", "added"), &sides),
        [(
            "readAtCommit".to_owned(),
            json!("src/new.rs"),
            json!("c0ffee")
        )]
    );
    assert_eq!(
        read(file("src/gone.rs", "removed"), &sides),
        [(
            "readParent".to_owned(),
            json!("src/gone.rs"),
            json!("parent1")
        )]
    );
    let no_parent = ChangeSides {
        old_ref: None,
        ..sides
    };
    assert_eq!(
        read(file("src/lib.rs", "modified"), &no_parent),
        [(
            "readAtCommit".to_owned(),
            json!("src/lib.rs"),
            json!("c0ffee")
        )]
    );
}
