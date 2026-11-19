//! A batch's pull-request rows read their first pages from one aliased
//! GraphQL request ([`super::super::warm_pull_requests`]): each row gets its
//! own part, as its own read would have answered, and a row whose alias
//! failed reads by REST, as it would have alone.
use super::*;
use std::sync::Arc;

/// `a/b#number`'s first page as an alias's answer under `repository`.
fn aliased_pr(number: u64) -> Value {
    let mut pr = graphql_pr(json!({
        "commentsConn": {"pageInfo": {"hasNextPage": false, "endCursor": null}, "nodes": [
            {"databaseId": number, "author": {"login": "ann", "__typename": "User"},
             "body": format!("why {number}"), "createdAt": "2026-01-03T00:00:00Z",
             "updatedAt": "2026-01-03T00:00:00Z", "url": "https://x"}
        ]}
    }));
    pr["data"]["repository"]["pullRequest"]["number"] = json!(number);
    pr["data"]["repository"].take()
}

fn row(number: u64) -> Value {
    json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": number, "sections": ["comments"]})
}

#[tokio::test]
async fn pull_request_rows_read_one_aliased_request_and_a_failed_alias_reads_rest() {
    let server = MockServer::builder().start().await;
    // The batch's aliased read: rows 7 and 8 answered, row 9's alias failed.
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("r0: repository"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"r0": aliased_pr(7), "r1": aliased_pr(8), "r2": null},
            "errors": [{"type": "NOT_FOUND", "message": "gone", "path": ["r2"]}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    // Row 9 alone: REST serves it, as a failed fast path does.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 9, "title": "Other", "state": "open", "body": "",
            "user": {"login": "bob"}, "labels": [], "draft": false,
            "base": {"ref": "main", "sha": BASE}, "head": {"ref": "feat", "sha": HEAD},
            "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z",
            "comments": 1, "review_comments": 0, "commits": 1, "changed_files": 1,
            "additions": 1, "deletions": 0
        })))
        .expect(1)
        .mount(&server)
        .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/issues/9/comments",
        200,
        json!([{"id": 90, "user": {"login": "cy", "type": "User"}, "body": "rest 9",
                "created_at": "2026-01-03T00:00:00Z", "updated_at": "2026-01-03T00:00:00Z"}]),
    )
    .await;
    let provider = one_shot_provider(&server);
    let rows = [7, 8, 9].map(row);
    let warms = rows
        .iter()
        .map(|row| super::super::PullRequestWarm::of(row).expect("GraphQL serves the first page"))
        .collect::<Vec<_>>();
    let memo = Arc::new(octocode_github::RefMemo::new().warming_pull_requests());
    super::super::warm_pull_requests(&provider.transport, &warms, &memo, &default_context()).await;
    memo.refs_known();
    let mut bodies = Vec::new();
    for row in rows {
        let request = HistoryItemRequest::from_row(row).expect("valid row");
        let context = default_context().with_ref_memo(memo.clone());
        let out = execute(&provider, &request, &context, &Passthrough)
            .await
            .expect("history item");
        bodies.push(out.data.pointer("/pullRequests/0/comments/0/body").cloned());
    }
    assert_eq!(
        bodies,
        [
            Some(json!("why 7")),
            Some(json!("why 8")),
            Some(json!("rest 9"))
        ]
    );
    let graphql = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .count();
    assert_eq!(graphql, 1, "rows 7 and 8 sent nothing; row 9 read REST");
}

/// What a batch reads together: a first page GraphQL serves, or a
/// summary's whole comment count; a patches read (REST only) is not.
#[test]
fn first_pages_and_summaries_plan_a_batch_read_and_patches_do_not() {
    assert!(super::super::PullRequestWarm::of(&row(7)).is_some());
    let summary = json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 7});
    assert!(super::super::PullRequestWarm::of(&summary).is_some());
    let patches = json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": 7, "sections": ["patches"]});
    assert!(super::super::PullRequestWarm::of(&patches).is_none());
}

/// Summary rows read their whole comment counts in the batch's one request:
/// each row states GitHub's whole count and REST's inline count beside it.
#[tokio::test]
async fn summary_rows_read_their_whole_counts_in_one_request() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .and(body_string_contains("r1: repository"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {
                "r0": {"pullRequest": {"totalCommentsCount": 222}},
                "r1": {"pullRequest": {"totalCommentsCount": 3}}
            }
        })))
        .expect(1)
        .mount(&server)
        .await;
    for number in [7, 8] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/pulls/{number}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "number": number, "title": "Fix", "state": "open", "body": "",
                "user": {"login": "bob"}, "labels": [], "draft": false,
                "base": {"ref": "main", "sha": BASE}, "head": {"ref": "feat", "sha": HEAD},
                "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-02T00:00:00Z",
                "comments": 55, "review_comments": 120, "commits": 1, "changed_files": 1,
                "additions": 1, "deletions": 0
            })))
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let rows = [7, 8].map(
        |number| json!({"operation": "pullRequest", "owner": "a", "repo": "b", "number": number}),
    );
    let warms = rows
        .iter()
        .map(|row| super::super::PullRequestWarm::of(row).expect("a summary"))
        .collect::<Vec<_>>();
    let memo = Arc::new(octocode_github::RefMemo::new().warming_pull_requests());
    super::super::warm_pull_requests(&provider.transport, &warms, &memo, &default_context()).await;
    memo.refs_known();
    let mut counts = Vec::new();
    for row in rows {
        let request = HistoryItemRequest::from_row(row).expect("valid row");
        let context = default_context().with_ref_memo(memo.clone());
        let out = execute(&provider, &request, &context, &Passthrough)
            .await
            .expect("history item");
        let pr = &out.data["pullRequests"][0];
        counts.push((
            pr["commentsCount"].clone(),
            pr["reviewCommentsCount"].clone(),
        ));
        assert!(out.data.get("warnings").is_none(), "{}", out.data);
    }
    assert_eq!(counts, [(json!(222), json!(120)), (json!(3), json!(120))]);
}
