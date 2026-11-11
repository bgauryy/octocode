//! The fix read a closed issue's linked pull requests lead to.

use super::*;

/// A small merged fix is read with its patches in the same call; a large
/// one (or one GitHub sized nothing for) reads its file inventory first.
#[test]
fn read_fix_pr_carries_the_patches_of_a_small_fix() {
    let query = HistoryItemRequest::from_row(json!({
        "operation":"issue","owner":"o","repo":"r","number":1
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
        out["calls"]["readPullRequest"]["input"]["queries"][0]["number"],
        8
    );
    assert_eq!(
        out["calls"]["readPullRequest"]["input"]["queries"][0]["sections"],
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
            out["calls"]["readPullRequest"]["input"]["queries"][0]["sections"],
            json!(["body", "files"])
        );
    }
    assert!(
        out_for(&query, &prs)["calls"]["readPullRequest"]["input"]["queries"][0]
            .get("include")
            .is_none()
    );
}

/// GitHub numbers start at 1: a node numbered 0 is no pull request, so it
/// is neither listed nor made the fix lead.
#[test]
fn a_node_with_an_invalid_number_is_dropped() {
    let query = HistoryItemRequest::from_row(json!({
        "operation":"issue","owner":"o","repo":"r","number":1
    }))
    .expect("issue query");
    let prs = map_closing_pull_requests(&[
        json!({"number":0,"state":"MERGED","mergedAt":"2026-09-11T15:55:40Z"}),
        json!({"number":5,"state":"CLOSED"}),
    ]);
    assert_eq!(
        prs.iter()
            .map(|pr| pr.row["number"].clone())
            .collect::<Vec<_>>(),
        [json!(5)]
    );
    let lead = out_for(&query, &prs);
    assert_eq!(
        lead["calls"]["readPullRequest"]["input"]["queries"][0]["number"],
        5
    );
    assert!(out_for(&query, &map_closing_pull_requests(&[json!({"number":0})]))["calls"]["readPullRequest"].is_null());
}

fn out_for(query: &HistoryItemRequest, prs: &[ClosingPr]) -> Value {
    let mut out = json!({});
    attach_fix_pr(&mut out, query, Some(prs), true);
    out
}

/// A pull request closed without merging only declared a fix: it is no
/// closer, nor is an open one once the issue was closed some other way. A
/// closed issue that no listed pull request closed leads to the search.
#[test]
fn only_a_merged_pull_request_closed_the_issue() {
    let query = HistoryItemRequest::from_row(json!({
        "operation":"issue","owner":"o","repo":"r","number":1
    }))
    .expect("issue query");
    let linked = || {
        let nodes = [
            json!({"number":21,"state":"CLOSED","mergedAt":null}),
            json!({"number":22,"state":"OPEN","mergedAt":null}),
        ];
        ClosingReferences {
            listed: nodes.len(),
            prs: map_closing_pull_requests(&nodes),
            bounded_total: None,
            unsettled: false,
        }
    };
    let numbers = |references: &ClosingReferences| {
        references
            .prs
            .iter()
            .map(|pr| pr.row["number"].clone())
            .collect::<Vec<_>>()
    };
    let closed = linked().closers(true);
    assert!(numbers(&closed).is_empty());
    assert_eq!((closed.listed, closed.settled().is_none()), (0, true));
    let mut lead = json!({});
    attach_fix_pr(&mut lead, &query, closed.settled(), true);
    assert!(lead["calls"]["readPullRequest"].is_null(), "{lead}");
    assert_eq!(lead["calls"]["findPullRequest"]["tool"], "ghSearchHistory");
    assert_eq!(numbers(&linked().closers(false)), [json!(22)]);
    let merged = ClosingReferences {
        listed: 2,
        prs: map_closing_pull_requests(&[
            json!({"number":21,"state":"CLOSED","mergedAt":null}),
            json!({"number":23,"state":"MERGED","mergedAt":"2026-09-11T15:55:40Z"}),
        ]),
        bounded_total: None,
        unsettled: false,
    }
    .closers(true);
    assert_eq!(numbers(&merged), [json!(23)]);
    assert_eq!(
        out_for(&query, &merged.prs)["calls"]["readPullRequest"]["input"]["queries"][0]["number"],
        23
    );
}
