//! The fix read a closed issue's linked pull requests lead to.

use super::*;

/// A small merged fix is read with its patches in the same call; a large
/// one (or one GitHub sized nothing for) reads its file inventory first.
#[test]
fn read_fix_pr_carries_the_patches_of_a_small_fix() {
    let query = HistoryItemRequest::from_row(json!({
        "operation":"issue","mainGoal":"g","reasoning":"r","owner":"o","repo":"r","number":1
    }))
    .expect("issue query");
    let prs = map_closing_pull_requests(&[
        json!({"number":7,"state":"CLOSED","mergedAt":null,"additions":1,"deletions":0,"changedFiles":1}),
        json!({"number":8,"state":"MERGED","mergedAt":"2026-09-11T15:55:40Z",
            "additions":63,"deletions":4,"changedFiles":3}),
    ]);
    assert_eq!(
        prs.iter().map(|pr| pr.row.clone()).collect::<Vec<_>>(),
        [
            json!({"number":8,"state":"merged","mergedAt":"2026-09-11T15:55:40Z"}),
            json!({"number":7,"state":"closed"}),
        ]
    );
    let mut out = json!({});
    attach_fix_pr(&mut out, &query, Some(&prs), true);
    assert_eq!(
        out["next"]["readPullRequest"]["query"]["queries"][0]["number"],
        8
    );
    assert_eq!(
        out["next"]["readPullRequest"]["query"]["queries"][0]["sections"],
        json!(["patches"])
    );
    for node in [
        json!({"number":9,"state":"MERGED","mergedAt":"x","additions":900,"deletions":4,"changedFiles":3}),
        json!({"number":9,"state":"MERGED","mergedAt":"x"}),
    ] {
        let mut out = json!({});
        attach_fix_pr(
            &mut out,
            &query,
            Some(&map_closing_pull_requests(&[node])),
            true,
        );
        assert_eq!(
            out["next"]["readPullRequest"]["query"]["queries"][0]["sections"],
            json!(["body", "files"])
        );
    }
    assert!(
        out_for(&query, &prs)["next"]["readPullRequest"]["query"]["queries"][0]
            .get("include")
            .is_none()
    );
}

fn out_for(query: &HistoryItemRequest, prs: &[ClosingPr]) -> Value {
    let mut out = json!({});
    attach_fix_pr(&mut out, query, Some(prs), true);
    out
}

/// A goal that names changed files reads only those files' patches of
/// the fix; dotted words that are not file names (`res.redirect`,
/// versions) never narrow it.
#[test]
fn read_fix_pr_narrows_to_files_the_goal_names() {
    let issue = |goal: &str| {
        HistoryItemRequest::from_row(json!({
            "operation":"issue","mainGoal":goal,"reasoning":"r","owner":"o","repo":"r","number":1
        }))
        .expect("issue query")
    };
    let large = map_closing_pull_requests(&[
        json!({"number":9,"state":"MERGED","mergedAt":"x","additions":900,"deletions":4,"changedFiles":40}),
    ]);
    let out = out_for(
        &issue("Which PR fixed cli/cli#14404 and what did it change in merge.go?"),
        &large,
    );
    let read = &out["next"]["readPullRequest"]["query"]["queries"][0];
    assert_eq!(read["include"], json!(["**/merge.go"]), "{read}");
    assert_eq!(read["sections"], json!(["patches"]), "{read}");
    let out = out_for(
        &issue("Check pkg/cmd/pr/merge/merge.go and README.md (v2.31.0)."),
        &large,
    );
    assert_eq!(
        out["next"]["readPullRequest"]["query"]["queries"][0]["include"],
        json!(["**/pkg/cmd/pr/merge/merge.go", "**/README.md"])
    );
    for goal in [
        "What does res.redirect default to in express@4.21.2?",
        "Which PR fixed this issue?",
    ] {
        let read = &out_for(&issue(goal), &large)["next"]["readPullRequest"]["query"]["queries"][0];
        assert!(read.get("include").is_none(), "{goal}: {read}");
        assert_eq!(read["sections"], json!(["body", "files"]), "{goal}");
    }
}
