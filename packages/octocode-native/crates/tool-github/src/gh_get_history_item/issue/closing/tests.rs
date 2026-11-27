//! What closed an issue and the fix read its closers lead to.

use super::*;
use octocode_test_support::json_fixture;

fn issue_query() -> HistoryItemRequest {
    HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "issue",
            "owner": "o",
            "repo": "r",
            "number": 1
        }"#,
    ))
    .expect("issue query")
}

fn leads(closing: &ClosingReferences, closed: bool) -> Value {
    let mut out = json!({});
    attach_fix_leads(&mut out, &issue_query(), Some(closing), closed);
    out
}

fn out_for(prs: Vec<ClosingPr>) -> Value {
    leads(&ClosingReferences::linked(prs), true)
}

/// A small merged fix is read with its patches in the same call; a large
/// one (or one GitHub sized nothing for) reads its file inventory first.
#[test]
fn read_fix_pr_carries_the_patches_of_a_small_fix() {
    let prs = || {
        map_closing_pull_requests(&[
            json_fixture(
                r#"{
                    "number": 7,
                    "state": "CLOSED",
                    "mergedAt": null,
                    "additions": 1,
                    "deletions": 0,
                    "changedFiles": 1
                }"#,
            ),
            json_fixture(
                r#"{
                    "number": 8,
                    "state": "MERGED",
                    "mergedAt": "2026-09-11T15:55:40Z",
                    "additions": 63,
                    "deletions": 4,
                    "changedFiles": 3
                }"#,
            ),
        ])
    };
    assert_eq!(
        prs().iter().map(|pr| pr.row.clone()).collect::<Vec<_>>(),
        [
            json_fixture(
                r#"{
                    "number": 8,
                    "state": "merged",
                    "mergedAt": "2026-09-11T15:55:40Z"
                }"#
            ),
            json_fixture(r#"{"number": 7, "state": "closed"}"#),
        ]
    );
    let out = out_for(prs());
    let read = &out["calls"]["readPullRequest"]["input"]["queries"][0];
    assert_eq!(read["number"], 8);
    assert_eq!(read["sections"], json_fixture(r#"["patches"]"#));
    assert!(read.get("include").is_none(), "{out}");
    for node in [
        json_fixture(
            r#"{
                "number": 9,
                "state": "MERGED",
                "mergedAt": "x",
                "additions": 900,
                "deletions": 4,
                "changedFiles": 3
            }"#,
        ),
        json_fixture(r#"{"number": 9, "state": "MERGED", "mergedAt": "x"}"#),
    ] {
        let out = out_for(map_closing_pull_requests(&[node]));
        assert_eq!(
            out["calls"]["readPullRequest"]["input"]["queries"][0]["sections"],
            json_fixture(r#"["body", "files"]"#)
        );
    }
}

/// GitHub numbers start at 1: a node numbered 0, or one GitHub nulled, is
/// no pull request, so it is neither listed nor made the fix lead.
#[test]
fn a_node_with_an_invalid_number_is_dropped() {
    let prs = map_closing_pull_requests(&[
        json_fixture(
            r#"{
                "number": 0,
                "state": "MERGED",
                "mergedAt": "2026-09-11T15:55:40Z"
            }"#,
        ),
        Value::Null,
        json_fixture(r#"{"number": 5, "state": "CLOSED"}"#),
    ]);
    assert_eq!(
        prs.iter()
            .map(|pr| pr.row["number"].clone())
            .collect::<Vec<_>>(),
        [json!(5)]
    );
    assert_eq!(
        out_for(prs)["calls"]["readPullRequest"]["input"]["queries"][0]["number"],
        5
    );
    let none = out_for(map_closing_pull_requests(&[json_fixture(
        r#"{"number": 0}"#,
    )]));
    assert!(none["calls"]["readPullRequest"].is_null(), "{none}");
}

/// A pull request closed without merging only declared a fix: it is no
/// closer, nor is an open one once the issue was closed some other way. A
/// closed issue that no listed pull request closed leads to the search.
#[test]
fn only_a_merged_pull_request_closed_the_issue() {
    let linked = || {
        ClosingReferences::linked(map_closing_pull_requests(&[
            json_fixture(r#"{"number": 21, "state": "CLOSED", "mergedAt": null}"#),
            json_fixture(r#"{"number": 22, "state": "OPEN", "mergedAt": null}"#),
        ]))
    };
    let numbers = |closing: &ClosingReferences| {
        closing
            .rows()
            .iter()
            .map(|row| row["number"].clone())
            .collect::<Vec<_>>()
    };
    let closed = linked().closers(true);
    assert!(numbers(&closed).is_empty());
    assert_eq!((closed.listed, closed.settled().is_none()), (0, true));
    let lead = leads(&closed, true);
    assert!(lead["calls"]["readPullRequest"].is_null(), "{lead}");
    assert_eq!(lead["calls"]["findPullRequest"]["tool"], "ghSearchHistory");
    assert_eq!(numbers(&linked().closers(false)), [json!(22)]);
    let merged = ClosingReferences::linked(map_closing_pull_requests(&[
        json_fixture(r#"{"number": 21, "state": "CLOSED", "mergedAt": null}"#),
        json_fixture(
            r#"{
                "number": 23,
                "state": "MERGED",
                "mergedAt": "2026-09-11T15:55:40Z"
            }"#,
        ),
    ]))
    .closers(true);
    assert_eq!(numbers(&merged), [json!(23)]);
    assert_eq!(
        leads(&merged, true)["calls"]["readPullRequest"]["input"]["queries"][0]["number"],
        23
    );
}

fn closed_event(closer: Value, reason: &str) -> Value {
    json!({"__typename":"ClosedEvent","createdAt":"2026-11-10T18:28:25Z",
        "stateReason":reason,"actor":{"login":"maintainer"},"closer":closer})
}

/// The last close names its commit closer, in its own repository when that
/// is not the issue's; a pull-request closer or none names no commit, and
/// an issue reopened since has no closer. A known commit settles the fix
/// even when the linked pull requests closed nothing.
#[test]
fn the_latest_close_names_a_commit_closer() {
    let commit = |name: &str| json!({"__typename":"Commit","oid":"c".repeat(40),"repository":{"nameWithOwner":name}});
    let row = |event: &Value| commit_closer(event, "o", "r").map(|closer| closer.row);
    assert_eq!(
        row(&closed_event(commit("O/R"), "COMPLETED")),
        Some(json!({"commitSha":"c".repeat(40)}))
    );
    assert_eq!(
        row(&closed_event(commit("fork/r"), "COMPLETED")),
        Some(json!({"commitSha":"c".repeat(40),"owner":"fork","repo":"r"}))
    );
    let pull = json_fixture(r#"{"__typename": "PullRequest", "number": 4}"#);
    assert_eq!(row(&closed_event(pull, "COMPLETED")), None);
    assert_eq!(row(&closed_event(Value::Null, "NOT_PLANNED")), None);
    assert_eq!(
        row(&json_fixture(
            r#"{"__typename": "ReopenedEvent", "createdAt": "t"}"#
        )),
        None
    );

    let with_commit = || {
        let mut closing = ClosingReferences::linked(map_closing_pull_requests(&[json_fixture(
            r#"{"number": 21, "state": "CLOSED", "mergedAt": null}"#,
        )]));
        closing.commit = commit_closer(&closed_event(commit("o/r"), "COMPLETED"), "o", "r");
        closing
    };
    let closed = with_commit().closers(true);
    assert_eq!(closed.rows(), [json!({"commitSha":"c".repeat(40)})]);
    assert!(closed.settled().is_some());
    let lead = leads(&closed, true);
    assert!(lead.get("calls").is_none(), "{lead}");
    assert!(with_commit().closers(false).rows().is_empty());
}

/// Each close and reopen is one row: GitHub's reason lowercased as REST
/// spells it, and the commit or pull request that closed it.
#[test]
fn state_events_name_each_transition_and_its_closer() {
    let pull = json_fixture(r#"{"__typename": "PullRequest", "number": 4}"#);
    let commit = json!({"__typename":"Commit","oid":"d".repeat(40)});
    let events = [
        closed_event(pull, "COMPLETED"),
        json_fixture(
            r#"{
                "__typename": "ReopenedEvent",
                "createdAt": "2026-11-11T00:00:00Z",
                "actor": {"login": "amy"}
            }"#,
        ),
        closed_event(commit, "NOT_PLANNED"),
        Value::Null,
    ];
    let rows = events.iter().filter_map(state_event).collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            json_fixture(
                r#"{
                    "event": "closed",
                    "at": "2026-11-10T18:28:25Z",
                    "by": "maintainer",
                    "stateReason": "completed",
                    "pullRequest": 4
                }"#
            ),
            json_fixture(
                r#"{
                    "event": "reopened",
                    "at": "2026-11-11T00:00:00Z",
                    "by": "amy"
                }"#
            ),
            json!({"event":"closed","at":"2026-11-10T18:28:25Z","by":"maintainer",
                "stateReason":"not_planned","commitSha":"d".repeat(40)}),
        ]
    );
    let mut closing = ClosingReferences::linked(Vec::new());
    closing.events = rows[..1].to_vec();
    assert!(
        closing.state_events().is_none(),
        "one close lists no events"
    );
    closing.events = rows;
    assert_eq!(
        closing
            .state_events()
            .map(|events| events.as_array().map(Vec::len)),
        Some(Some(3))
    );
}

/// The first page reads the events beside the references in one request;
/// a later page of references reads the references alone.
#[test]
fn the_first_page_reads_the_events_in_the_same_request() {
    assert!(
        CLOSING_FIRST_DOCUMENT.contains("timelineItems(itemTypes:[CLOSED_EVENT,REOPENED_EVENT]")
    );
    assert!(!CLOSING_REFERENCES_DOCUMENT.contains("timelineItems"));
    let references = CLOSING_REFERENCES_DOCUMENT
        .split_once("closedByPullRequestsReferences")
        .map(|(_, rest)| rest);
    assert_eq!(
        CLOSING_FIRST_DOCUMENT
            .split_once("closedByPullRequestsReferences")
            .map(|(_, rest)| rest),
        references
    );
}

/// What GitHub withheld is one warning per kind, and a withheld closer
/// leads to the search beside the read of the closer it did show.
#[test]
fn a_withheld_closer_is_disclosed_and_searched_for() {
    let mut closing = ClosingReferences::linked(map_closing_pull_requests(&[json_fixture(
        r#"{
            "number": 5,
            "state": "MERGED",
            "mergedAt": "2026-11-10T18:28:24Z"
        }"#,
    )]));
    assert!(closing.withheld_warnings().is_empty());
    closing.withheld.pull_requests = 1;
    closing.withheld.events = true;
    let warnings = closing.withheld_warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(warnings[0].contains("related"), "{warnings:?}");
    let lead = leads(&closing.closers(true), true);
    let names = lead["calls"]
        .as_object()
        .map(|calls| calls.keys().cloned().collect::<Vec<_>>());
    assert_eq!(
        names,
        Some(vec![
            "readPullRequest".to_owned(),
            "findPullRequest".to_owned()
        ])
    );
}
