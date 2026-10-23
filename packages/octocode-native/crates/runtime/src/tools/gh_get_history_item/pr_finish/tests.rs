//! The finish step and the metadata row of a pull-request read.
use super::super::util::merge;
use super::*;

fn summary(raw: Value, details: bool) -> Value {
    let mut row = json!({
        "operation":"pullRequest","mainGoal":"g","reasoning":"r","owner":"o","repo":"r",
        "number":1
    });
    if details {
        row["extras"] = json!(["details"]);
    }
    let query = HistoryItemRequest::from_row(row).expect("query");
    pr_metadata(&raw, &query)
}

#[test]
fn summary_rows_state_each_time_once_and_preview_the_body() {
    let base = json!({"number":1,"title":"t","user":{"login":"a"},"body":"Long description",
        "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-05T00:00:00Z"});
    let merged = summary(
        merge(
            base.clone(),
            json!({"state":"closed","merged_at":"2026-01-04T00:00:00Z",
            "closed_at":"2026-01-04T00:00:01Z","merge_commit_sha":"abc"}),
        ),
        false,
    );
    assert_eq!(merged["mergedAt"], "2026-01-04T00:00:00Z", "{merged}");
    for absent in ["closedAt", "updatedAt", "body"] {
        assert!(merged.get(absent).is_none(), "{absent}: {merged}");
    }
    // The summary previews the body; the menu's body read holds it whole.
    assert_eq!(merged["bodyPreview"], "Long description", "{merged}");
    let closed = summary(
        merge(
            base.clone(),
            json!({"state":"closed","merged_at":null,
            "closed_at":"2026-01-04T00:00:00Z"}),
        ),
        false,
    );
    assert_eq!(closed["closedAt"], "2026-01-04T00:00:00Z", "{closed}");
    assert!(closed.get("updatedAt").is_none(), "{closed}");
    let open = summary(merge(base.clone(), json!({"state":"open"})), false);
    assert_eq!(open["updatedAt"], "2026-01-05T00:00:00Z", "{open}");
    // details keep the provider's timestamps whole.
    let details = summary(
        merge(
            base,
            json!({"state":"closed","merged_at":"2026-01-04T00:00:00Z",
            "closed_at":"2026-01-04T00:00:01Z"}),
        ),
        true,
    );
    assert_eq!(details["closedAt"], "2026-01-04T00:00:01Z", "{details}");
    assert_eq!(details["updatedAt"], "2026-01-05T00:00:00Z", "{details}");
}

/// The summary names the change's three commits: the head
/// (`headSha`), the recorded target-branch tip (`baseSha`) and a
/// merged PR's merge commit. A section read keeps what names the PR
/// (number, title, author) and the commits code is read at (`headSha`,
/// `mergeCommitSha`), nothing else.
#[test]
fn every_pr_view_keeps_source_target_and_merge_commits() {
    let raw = json!({"number":1,"title":"t","user":{"login":"a"},"state":"closed",
        "merged_at":"2026-01-04T00:00:00Z","merge_commit_sha":"m1",
        "head":{"sha":"h1","ref":"feat"},"base":{"sha":"b1","ref":"main"},
        "created_at":"2026-01-01T00:00:00Z","changed_files":3,"additions":5,"deletions":1,
        "commits":4,"review_threads":2});
    let row = summary(raw.clone(), false);
    assert_eq!(
        (&row["headSha"], &row["baseSha"], &row["mergeCommitSha"]),
        (&json!("h1"), &json!("b1"), &json!("m1")),
        "{row}"
    );
    assert_eq!(row["commitsCount"], 4, "{row}");
    assert_eq!(row["reviewThreadsCount"], 2, "{row}");
    for fields in [
        json!({"sections":["patches"]}),
        json!({"sections":["files"]}),
        json!({"sections":["patches"],"matchString":"x"}),
        json!({"sections":["patches"],"page":2}),
    ] {
        let query = HistoryItemRequest::from_row(merge(
            json!({"operation":"pullRequest","owner":"o","repo":"r","number":1}),
            fields.clone(),
        ))
        .expect("query");
        let mut row = pr_metadata(&raw, &query);
        slim_row(&mut row, &query);
        let mut expected = json!({"number":1,"title":"t","author":"a","state":"merged",
            "headSha":"h1","mergeCommitSha":"m1"});
        // Only the first page of a file inventory keeps the count the
        // list it opens is measured against.
        if fields == json!({"sections":["files"]}) {
            expected["changedFilesCount"] = json!(3);
        }
        assert_eq!(row, expected, "{fields}");
    }
    // An open PR has no merge commit; REST has no review-thread total.
    let open = summary(
        json!({"number":1,"state":"open","head":{"sha":"h1"},"base":{"sha":"b1"},
            "merge_commit_sha":"test-merge","review_comments":9}),
        false,
    );
    assert!(open.get("mergeCommitSha").is_none(), "{open}");
    assert!(open.get("reviewThreadsCount").is_none(), "{open}");
    assert_eq!(open["baseSha"], "b1", "{open}");
}
