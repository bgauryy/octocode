//! Issue reads: REST fetch, closing pull requests, body windows, summaries and the discussion beside the issue.

use crate::support::{self};
use serde_json::json;
use support::{Workspace, call, checks, row_data, row_status};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn gh_get_history_item_issue_fetches_via_rest() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42,
            "title": "Parser OOM on large inputs",
            "state": "open",
            "body": "Detailed reproduction steps here",
            "user": {"login": "alice"},
            "labels": [],
            "comments": 3
        })))
        .mount(&server)
        .await;

    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42}),
    )
    .await
    .expect("gh_get_history_item issue");

    assert_eq!(
        row_status(&outcome),
        "success",
        "{}",
        outcome.structured_content
    );
    let data = row_data(&outcome);
    assert_eq!(
        data["issues"][0]["title"], "Parser OOM on large inputs",
        "{data}"
    );
    runtime.close().await;
}

/// An issue read lists the pull requests that closed it
/// (merged first) and offers the merged fix as `next.readPullRequest`; without
/// GraphQL the read falls back to the keyword search hop.
#[tokio::test]
async fn issue_read_lists_closing_pull_requests_and_reads_the_merged_fix() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42, "title": "model_config mutates", "state": "closed",
            "state_reason": "completed", "body": "repro", "user": {"login": "alice"},
            "labels": [], "comments": 0, "closed_at": "2026-09-25T00:29:37Z",
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:29:37Z"
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"issue": {"closedByPullRequestsReferences": {"nodes": [
                {"number": 13794, "state": "CLOSED", "mergedAt": null},
                {"number": 13825, "state": "MERGED", "mergedAt": "2026-09-25T00:29:36Z"}
            ]}}}
        }})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42}),
    )
    .await
    .expect("issue read");
    let data = row_data(&outcome);
    assert_eq!(row_status(&outcome), "success", "{data}");
    let issue = &data["issues"][0];
    assert_eq!(
        issue["closedBy"],
        json!([
            {"number": 13825, "state": "merged", "mergedAt": "2026-09-25T00:29:36Z"},
            {"number": 13794, "state": "closed"}
        ]),
        "{data}"
    );
    let read = support::checks::related_call(data, "ghGetHistoryItem", |input| {
        input["queries"][0]["operation"] == "pullRequest"
    });
    assert_eq!(read["tool"], "ghGetHistoryItem", "{data}");
    assert_eq!(
        read["input"]["queries"][0]["operation"], "pullRequest",
        "{data}"
    );
    assert_eq!(read["input"]["queries"][0]["number"], 13825, "{data}");
    runtime.close().await;

    // GraphQL unavailable: no closedBy, the search hop instead.
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42, "title": "t", "state": "closed", "state_reason": "completed",
            "body": "", "user": {"login": "alice"}, "labels": [], "comments": 0
        })))
        .mount(&server)
        .await;
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42}),
    )
    .await
    .expect("issue read");
    let data = row_data(&outcome);
    assert!(data["issues"][0].get("closedBy").is_none(), "{data}");
    let find = support::checks::related_call(data, "ghSearchHistory", |input| {
        input["queries"][0]["operation"] == "pullRequest"
    });
    assert_eq!(find["tool"], "ghSearchHistory", "{data}");
    assert_eq!(
        find["input"]["queries"][0]["keywords"],
        json!(["42"]),
        "{data}"
    );
    runtime.close().await;
}

/// A bounded closing-reference set is disclosed once, on the first body
/// window that lists it; later windows list no `closedBy`, so they spend no
/// GraphQL request on it and restate no bound.
#[tokio::test]
async fn issue_body_windows_keep_closing_reference_coverage() {
    let server = MockServer::builder().start().await;
    let body = (0..6000)
        .map(|n| format!("source body {n}\n"))
        .collect::<String>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42, "title": "synthetic", "state": "closed", "state_reason": "completed",
            "body": body, "user": {"login": "audit"}, "labels": [], "comments": 0,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-10-02T00:00:00Z"
        })))
        .mount(&server)
        .await;
    let nodes = (0..25)
        .map(|n| json!({"number": 101 + n, "state": "CLOSED", "mergedAt": null}))
        .collect::<Vec<_>>();
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "repository": {"issue": {"closedByPullRequestsReferences": {
                "totalCount": 31, "pageInfo": {"hasNextPage": true}, "nodes": nodes
            }}}
        }})))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outputs = std::cell::RefCell::new(Vec::new());
    support::walk(&runtime,"ghGetHistoryItem",json!({"queries":[{"operation":"issue","owner":"a","repo":"b","number":42,"sections":["body"]}],"responseChars":2500}),|output| {
        for row in output["results"].as_array().into_iter().flatten() {
            assert_ne!(row["status"],"error","{output}");
        }
        outputs.borrow_mut().push(output.clone());
        Vec::new()
    }).await;
    let outputs = outputs.into_inner();
    let issues = outputs
        .iter()
        .flat_map(|output| output["results"].as_array().into_iter().flatten())
        .flat_map(|row| row["data"]["issues"].as_array().into_iter().flatten())
        .collect::<Vec<_>>();
    assert_eq!(
        issues
            .iter()
            .filter_map(|issue| issue["body"].as_str())
            .collect::<String>(),
        body
    );
    assert!(
        outputs
            .iter()
            .any(|output| output.to_string().contains("closingReferenceLimit"))
    );
    let closed = issues
        .iter()
        .flat_map(|issue| issue["closedBy"].as_array().into_iter().flatten())
        .map(|pr| pr["number"].as_u64().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(closed, (101..126).collect());
    let documents = server
        .received_requests()
        .await
        .expect("recorded")
        .into_iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect::<Vec<_>>();
    assert_eq!(documents.len(), 1, "{documents:?}");
    assert!(documents[0].contains("nodes"), "{documents:?}");
    runtime.close().await;
}

/// An issue summary counts its comments and offers the discussion
/// read; an issue without comments offers neither.
#[tokio::test]
async fn history_a_issue_summary_counts_comments_and_offers_the_discussion() {
    for (comments, offered) in [(1, true), (0, false)] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/issues/42"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "number": 42, "title": "Parser OOM", "state": "open", "body": "repro",
                "user": {"login": "alice"}, "labels": [], "comments": comments,
                "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
            })))
            .mount(&server)
            .await;
        let workspace = Workspace::new();
        let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
        let outcome = call(
            &runtime,
            "ghGetHistoryItem",
            json!({"operation": "issue", "owner": "a", "repo": "b", "number": 42}),
        )
        .await
        .expect("issue read");
        let data = row_data(&outcome);
        assert_eq!(row_status(&outcome), "success", "{data}");
        let issue = &data["issues"][0];
        let read = checks::find_related_call(data, "ghGetHistoryItem", |input| {
            input["queries"][0]["sections"] == json!(["comments"])
        });
        if offered {
            let read = read.expect("optional discussion");
            assert_eq!(issue["commentsCount"], 1, "{data}");
            assert_eq!(read["tool"], "ghGetHistoryItem", "{data}");
            Mock::given(method("GET")).and(path("/api/v3/repos/a/b/issues/42/comments"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id":17,"body":"discussion","user":{"login":"bob"},"created_at":"2026-09-21T00:00:00Z"}])))
                .mount(&server).await;
            let discussion = call(&runtime, "ghGetHistoryItem", read["input"].clone())
                .await
                .expect("discussion cursor");
            assert_eq!(
                row_data(&discussion)["issues"][0]["comments"][0]["id"],
                "17"
            );
        } else {
            assert!(issue.get("commentsCount").is_none(), "{data}");
            assert!(read.is_none(), "{data}");
        }
        runtime.close().await;
    }
}

/// An issue read with its discussion asks for the first comment batch
/// beside the issue itself: the batch depends only on the page asked for,
/// so the read is one round trip, not two.
#[tokio::test]
async fn an_issue_discussion_loads_beside_the_issue() {
    use octocode_github::mock_http::Delayed;
    use std::time::Duration;
    let server = MockServer::builder().start().await;
    let delay = Duration::from_millis(400);
    let issue = Delayed::new(
        delay,
        ResponseTemplate::new(200).set_body_json(json!({
            "number": 7, "title": "Thread", "state": "open", "body": "repro",
            "user": {"login": "alice"}, "labels": [], "comments": 2,
            "created_at": "2026-09-20T00:00:00Z", "updated_at": "2026-09-25T00:00:00Z"
        })),
    );
    let issue_arrivals = issue.arrivals.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7"))
        .respond_with(issue)
        .mount(&server)
        .await;
    let comments = Delayed::new(
        Duration::ZERO,
        ResponseTemplate::new(200).set_body_json(json!([
            {"id": 1, "user": {"login": "bob"}, "body": "one",
             "created_at": "2026-09-21T00:00:00Z", "updated_at": "2026-09-21T00:00:00Z"},
            {"id": 2, "user": {"login": "eve"}, "body": "two",
             "created_at": "2026-09-22T00:00:00Z", "updated_at": "2026-09-22T00:00:00Z"}
        ])),
    );
    let comment_arrivals = comments.arrivals.clone();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/7/comments"))
        .respond_with(comments)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&support::github_api(&server));
    let outcome = call(
        &runtime,
        "ghGetHistoryItem",
        json!({"operation":"issue","owner":"a","repo":"b","number":7,
               "sections":["comments"]}),
    )
    .await
    .expect("issue comments");
    let data = row_data(&outcome);
    assert_eq!(
        data["issues"][0]["comments"].as_array().map(Vec::len),
        Some(2),
        "{data}"
    );
    let (issue_at, comments_at) = (issue_arrivals.times(), comment_arrivals.times());
    assert_eq!((issue_at.len(), comments_at.len()), (1, 1));
    let gap = comments_at[0]
        .checked_duration_since(issue_at[0])
        .unwrap_or_default();
    assert!(gap < delay / 2, "comments waited {gap:?} for the issue");
    runtime.close().await;
}
