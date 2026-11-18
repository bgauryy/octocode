//! The finish step and the metadata row of a pull-request read.
use super::super::util::merge;
use super::*;
use octocode_test_support::json_fixture;

fn summary(raw: Value) -> Value {
    let row = json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "o",
            "repo": "r",
            "number": 1
        }"#,
    );
    let query = HistoryItemRequest::from_row(row).expect("query");
    pr_metadata(&raw, &query)
}

#[test]
fn summary_rows_state_each_time_once_and_preview_the_body() {
    let base = json_fixture(
        r#"{
            "number": 1,
            "title": "t",
            "user": {"login": "a"},
            "body": "Long description",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-05T00:00:00Z"
        }"#,
    );
    let merged = summary(merge(
        base.clone(),
        json_fixture(
            r#"{
                "state": "closed",
                "merged_at": "2026-01-04T00:00:00Z",
                "closed_at": "2026-01-04T00:00:01Z",
                "merge_commit_sha": "abc"
            }"#,
        ),
    ));
    assert_eq!(merged["mergedAt"], "2026-01-04T00:00:00Z", "{merged}");
    assert!(merged.get("body").is_none(), "{merged}");
    // The summary previews the body; the menu's body read holds it whole.
    assert_eq!(merged["bodyPreview"], "Long description", "{merged}");
    let closed = summary(merge(
        base.clone(),
        json_fixture(
            r#"{
                "state": "closed",
                "merged_at": null,
                "closed_at": "2026-01-04T00:00:00Z"
            }"#,
        ),
    ));
    assert_eq!(closed["closedAt"], "2026-01-04T00:00:00Z", "{closed}");
    // Only an open pull request states its last update.
    assert!(closed.get("updatedAt").is_none(), "{closed}");
    assert!(merged.get("updatedAt").is_none(), "{merged}");
    let open = summary(merge(base.clone(), json_fixture(r#"{"state": "open"}"#)));
    assert_eq!(open["updatedAt"], "2026-01-05T00:00:00Z", "{open}");
    // details keep the provider's timestamps whole.
    let details = summary(merge(
        base,
        json_fixture(
            r#"{
                "state": "closed",
                "merged_at": "2026-01-04T00:00:00Z",
                "closed_at": "2026-01-04T00:00:01Z"
            }"#,
        ),
    ));
    assert_eq!(details["closedAt"], "2026-01-04T00:00:01Z", "{details}");
    assert!(details.get("updatedAt").is_none(), "{details}");
}

/// A merged pull request closes when it merges: the one time is `mergedAt`,
/// and `closedAt` is stated only when GitHub recorded another.
#[test]
fn a_merge_time_is_not_restated_as_the_close_time() {
    let raw = json_fixture(
        r#"{
            "number": 1,
            "title": "t",
            "user": {"login": "a"},
            "state": "closed",
            "created_at": "2026-01-01T00:00:00Z",
            "merged_at": "2026-01-04T00:00:00Z",
            "closed_at": "2026-01-04T00:00:00Z"
        }"#,
    );
    let merged = summary(raw);
    assert_eq!(merged["mergedAt"], "2026-01-04T00:00:00Z", "{merged}");
    assert!(merged.get("closedAt").is_none(), "{merged}");
}

/// The summary names the change's three commits: the head
/// (`headCommitSha`), the recorded target-branch tip (`baseCommitSha`) and a
/// merged PR's merge commit. A section read keeps what names the PR
/// (number, title, author) and the commits code is read at (`headCommitSha`,
/// `mergeCommitSha`), nothing else.
#[test]
fn every_pr_view_keeps_source_target_and_merge_commits() {
    let raw = json_fixture(
        r#"{
            "number": 1,
            "title": "t",
            "user": {"login": "a"},
            "state": "closed",
            "merged_at": "2026-01-04T00:00:00Z",
            "merge_commit_sha": "m1",
            "head": {"sha": "h1", "ref": "feat"},
            "base": {"sha": "b1", "ref": "main"},
            "created_at": "2026-01-01T00:00:00Z",
            "changed_files": 3,
            "additions": 5,
            "deletions": 1,
            "commits": 4,
            "review_threads": 2
        }"#,
    );
    let row = summary(raw.clone());
    assert_eq!(
        (
            &row["headCommitSha"],
            &row["baseCommitSha"],
            &row["mergeCommitSha"]
        ),
        (&json!("h1"), &json!("b1"), &json!("m1")),
        "{row}"
    );
    assert_eq!(row["commitCount"], 4, "{row}");
    assert_eq!(row["reviewThreadsCount"], 2, "{row}");
    for fields in [
        json_fixture(r#"{"sections": ["patches"]}"#),
        json_fixture(r#"{"sections": ["files"]}"#),
        json_fixture(r#"{"sections": ["patches"], "matchString": ["x"]}"#),
    ] {
        let query = HistoryItemRequest::from_row(merge(
            json_fixture(
                r#"{
                    "operation": "pullRequest",
                    "owner": "o",
                    "repo": "r",
                    "number": 1
                }"#,
            ),
            fields.clone(),
        ))
        .expect("query");
        let row = pr_metadata(&raw, &query);
        for (field, value) in [
            ("headCommitSha", "h1"),
            ("baseCommitSha", "b1"),
            ("mergeCommitSha", "m1"),
        ] {
            assert_eq!(row[field], value, "{fields}: {row}");
        }
        // A file list states how many files the pull request changes; the
        // other totals and the rest of the header are the summary read's.
        assert_eq!(row["changedFileCount"], 3);
        for summary_only in ["commitCount", "title", "author", "createdAt", "additions"] {
            assert!(row.get(summary_only).is_none(), "{fields}: {row}");
        }
    }
    // An open PR has no merge commit; REST has no review-thread total.
    let open = summary(json_fixture(
        r#"{
            "number": 1,
            "state": "open",
            "head": {"sha": "h1"},
            "base": {"sha": "b1"},
            "merge_commit_sha": "test-merge",
            "review_comments": 9
        }"#,
    ));
    assert!(open.get("mergeCommitSha").is_none(), "{open}");
    assert!(open.get("reviewThreadsCount").is_none(), "{open}");
    assert_eq!(open["baseCommitSha"], "b1", "{open}");
}

/// A later page (`page` past 1, or a char `offset`) continues a row whose
/// first page already delivered the header: it names the pull request
/// (`number`) and the state the contract requires (`state`, `headCommitSha`), and
/// carries no empty placeholder for the rest.
#[test]
fn a_later_page_restates_only_the_pull_request_identity() {
    let raw = json_fixture(
        r#"{
            "number": 1,
            "title": "t",
            "user": {"login": "a"},
            "state": "closed",
            "merged_at": "2026-01-04T00:00:00Z",
            "merge_commit_sha": "m1",
            "head": {"sha": "h1", "ref": "feat"},
            "base": {"sha": "b1", "ref": "main"},
            "created_at": "2026-01-01T00:00:00Z",
            "changed_files": 3,
            "additions": 5,
            "deletions": 1,
            "commits": 4
        }"#,
    );
    for fields in [
        json_fixture(r#"{"page": 2}"#),
        json_fixture(r#"{"offset": 480}"#),
    ] {
        let query = HistoryItemRequest::from_row(merge(
            json_fixture(
                r#"{
                    "operation": "pullRequest",
                    "owner": "o",
                    "repo": "r",
                    "number": 1,
                    "sections": ["patches"]
                }"#,
            ),
            fields.clone(),
        ))
        .expect("query");
        let row = pr_metadata(&raw, &query);
        let mut keys = row
            .as_object()
            .expect("object")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        keys.sort();
        assert_eq!(
            keys,
            ["headCommitSha", "number", "state"],
            "{fields}: {row}"
        );
        octocode_runtime_base::test_support::assert_row_contract(
            octocode_runtime_base::id::ToolId::GhGetHistoryItem,
            &json!({"pullRequests": [row]}),
        );
    }
}

/// More labels than the GraphQL read holds: the row says so in the field
/// core declares, so the published row validates.
#[test]
fn a_clipped_label_list_is_disclosed_as_the_contract_field() {
    let row = summary(json_fixture(
        r#"{
            "number": 1,
            "title": "t",
            "user": {"login": "a"},
            "state": "open",
            "labels": ["bug"],
            "labels_truncated": true,
            "created_at": "2026-01-01T00:00:00Z"
        }"#,
    ));
    assert_eq!(row["labelsClipped"], true, "{row}");
    octocode_runtime_base::test_support::assert_row_contract(
        octocode_runtime_base::id::ToolId::GhGetHistoryItem,
        &json!({"pullRequests": [row]}),
    );
}
