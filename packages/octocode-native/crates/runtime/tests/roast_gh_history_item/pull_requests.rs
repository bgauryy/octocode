//! A pull-request read states its update time only while it is open, keeps
//! its discussion in GitHub's order, and folds a body-less COMMENTED review
//! into the inline comments it batched.
use super::issues::{assert_chronological, discussion};
use super::{data, served};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const HEAD: &str = "2222222222222222222222222222222222222222";
const BASE: &str = "3333333333333333333333333333333333333333";

/// Pull request #7 of a/b, its REST fields overridden by `fields`.
async fn mount_pr(server: &MockServer, fields: Value) {
    let mut pr = json!({
        "number": 7, "title": "t", "state": "open", "body": "",
        "user": {"login": "ann"}, "merged_at": null, "closed_at": null,
        "head": {"sha": HEAD, "ref": "topic"}, "base": {"sha": BASE, "ref": "main"},
        "created_at": "2026-11-01T00:00:00Z", "updated_at": "2026-11-09T00:00:00Z",
        "comments": 3, "review_comments": 2, "commits": 1, "changed_files": 1,
        "additions": 1, "deletions": 0
    });
    if let (Some(pr), Some(fields)) = (pr.as_object_mut(), fields.as_object()) {
        pr.extend(fields.clone());
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7"))
        .respond_with(ResponseTemplate::new(200).set_body_json(pr))
        .mount(server)
        .await;
}

async fn mount(server: &MockServer, route: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/{route}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

fn pr_read(sections: &[&str]) -> Value {
    let mut read = json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 7
        }"#,
    );
    if !sections.is_empty() {
        read["sections"] = json!(sections);
    }
    read
}

fn pr_row(served: &Value) -> &Value {
    &data(served)["pullRequests"][0]
}

/// `updatedAt` says whether an open pull request is still moving; a merged
/// or closed one states when it merged or closed instead.
#[tokio::test]
async fn only_an_open_pull_request_states_its_update_time() {
    for (fields, stated) in [
        (json_fixture(r#"{"state": "open"}"#), true),
        (
            json_fixture(
                r#"{
                    "state": "closed",
                    "merged_at": "2026-11-08T00:00:00Z",
                    "closed_at": "2026-11-08T00:00:00Z"
                }"#,
            ),
            false,
        ),
        (
            json_fixture(r#"{"state": "closed", "closed_at": "2026-11-08T00:00:00Z"}"#),
            false,
        ),
    ] {
        let server = MockServer::builder().start().await;
        mount_pr(&server, fields.clone()).await;
        let served = served(&server, pr_read(&[])).await;
        let row = pr_row(&served);
        assert_eq!(row.get("updatedAt").is_some(), stated, "{fields}: {served}");
        if !stated {
            assert!(
                row.get("mergedAt").is_some() || row.get("closedAt").is_some(),
                "{fields}: {served}"
            );
        }
    }
}

/// The discussion keeps GitHub's order: a bot comment between two human
/// ones stays between them, marked `bot`.
#[tokio::test]
async fn pull_request_comments_keep_chronological_order_with_bots_marked() {
    let server = MockServer::builder().start().await;
    mount_pr(&server, json!({})).await;
    mount(&server, "issues/7/comments", discussion()).await;
    let served = served(&server, pr_read(&["comments"])).await;
    assert_chronological(&pr_row(&served)["comments"], &served);
}

/// Three reviews by two reviewers: an approval with a body, a body-less
/// COMMENTED review whose two inline comments are in this read, and a
/// body-less COMMENTED review whose comments are not.
async fn mount_reviews(server: &MockServer) {
    mount_pr(server, json!({})).await;
    let review = |id: u64, author: &str, state: &str, body: &str| {
        json!({"id": id, "user": {"login": author}, "state": state, "body": body,
            "submitted_at": "2026-11-03T00:00:00Z", "commit_id": HEAD})
    };
    mount(
        server,
        "pulls/7/reviews",
        json!([
            review(1, "rev", "APPROVED", "ship it"),
            review(2, "rev", "COMMENTED", ""),
            review(3, "other", "COMMENTED", "")
        ]),
    )
    .await;
    let inline = |id: u64, line: u64| {
        json!({"id": id, "pull_request_review_id": 2, "user": {"login": "rev"},
            "body": format!("note {id}"), "path": "src/a.rs", "line": line, "side": "RIGHT",
            "commit_id": HEAD, "created_at": "2026-11-03T00:00:01Z"})
    };
    mount(
        server,
        "pulls/7/comments",
        json!([inline(11, 3), inline(12, 9)]),
    )
    .await;
}

fn ids(rows: &Value) -> Vec<Value> {
    rows.as_array()
        .into_iter()
        .flatten()
        .map(|row| row["id"].clone())
        .collect()
}

/// With its inline comments in the same read, the body-less review is not
/// listed again: each comment names it (`reviewId`) and keeps its author,
/// time and commit. The approval and the review whose comments are not in
/// this read stay. A comment's commit equal to the PR's `headCommitSha` is
/// omitted and read from it (hoist H2).
#[tokio::test]
async fn a_review_folded_into_its_comments_is_not_listed_again() {
    let server = MockServer::builder().start().await;
    mount_reviews(&server).await;
    let served = served(&server, pr_read(&["reviews", "reviewComments"])).await;
    let row = pr_row(&served);
    assert_eq!(ids(&row["reviews"]), [json!("1"), json!("3")], "{served}");
    let comments = row["reviewComments"].as_array().expect("reviewComments");
    assert_eq!(comments.len(), 2, "{served}");
    for comment in comments {
        assert_eq!(comment["reviewId"], "2", "{served}");
        assert_eq!(comment["author"], "rev", "{served}");
        assert!(comment.get("commitSha").is_none(), "{served}");
        assert_eq!(
            row["headCommitSha"], HEAD,
            "the commit reads from the PR: {served}"
        );
        assert!(comment["createdAt"].is_string(), "{served}");
    }
}

/// Reviews read alone list every review; inline comments read alone name
/// no review (no review row was folded into them).
#[tokio::test]
async fn reviews_alone_stay_whole_and_comments_alone_name_no_review() {
    let server = MockServer::builder().start().await;
    mount_reviews(&server).await;
    let reviews = served(&server, pr_read(&["reviews"])).await;
    assert_eq!(
        ids(&pr_row(&reviews)["reviews"]),
        [json!("1"), json!("2"), json!("3")],
        "{reviews}"
    );
    let comments = served(&server, pr_read(&["reviewComments"])).await;
    let rows = pr_row(&comments)["reviewComments"]
        .as_array()
        .expect("reviewComments");
    assert_eq!(rows.len(), 2, "{comments}");
    assert!(
        rows.iter().all(|comment| comment.get("reviewId").is_none()),
        "{comments}"
    );
}
