//! An issue read states what closed the issue (linked pull requests, then
//! the commit whose push closed it), why it closed, each close and reopen
//! when there was more than one, and its discussion in GitHub's order.
use super::{data, related, served};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const FIRST_CLOSER: &str = "97cef84f54f848b84afc3dabb6b8eaba5abc11cd";
const LAST_CLOSER: &str = "1111111111111111111111111111111111111111";

/// The REST issue #42 of a/b in `state`, closed for `reason` when closed.
async fn mount_issue(server: &MockServer, state: &str, reason: Option<&str>, comments: u64) {
    let closed = state == "closed";
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "number": 42, "title": "t", "state": state, "state_reason": reason,
            "body": "repro", "user": {"login": "alice"}, "labels": [], "comments": comments,
            "created_at": "2026-11-01T00:00:00Z", "updated_at": "2026-11-10T18:28:25Z",
            "closed_at": closed.then_some("2026-11-10T18:28:25Z")
        })))
        .mount(server)
        .await;
}

/// GitHub's GraphQL answer for the issue: its close/reopen timeline and
/// its closing references, with `errors` beside the data.
async fn mount_graphql(server: &MockServer, issue: Value, errors: Option<Value>) {
    let mut answer = json!({"data": {"repository": {"issue": issue}}});
    if let Some(errors) = errors {
        answer["errors"] = errors;
    }
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .mount(server)
        .await;
}

fn references(nodes: Value) -> Value {
    json!({"totalCount": nodes.as_array().map_or(0, Vec::len),
        "pageInfo": {"hasNextPage": false, "endCursor": null}, "nodes": nodes})
}

fn closed_by(commit: &str, reason: &str, at: &str) -> Value {
    json!({"__typename": "ClosedEvent", "createdAt": at, "stateReason": reason,
        "actor": {"login": "maintainer"},
        "closer": {"__typename": "Commit", "oid": commit,
            "repository": {"nameWithOwner": "a/b"}}})
}

fn issue_read() -> Value {
    json_fixture(
        r#"{
            "operation": "issue",
            "owner": "a",
            "repo": "b",
            "number": 42
        }"#,
    )
}

fn issue_row(served: &Value) -> &Value {
    &data(served)["issues"][0]
}

/// Closed by a commit, reopened, then closed again by another commit as not
/// planned: `closedBy` names the last closing commit, `stateReason` the
/// last reason, and `stateEvents` lists the three transitions in order.
#[tokio::test]
async fn an_issue_closed_by_a_commit_names_the_commit_the_reason_and_each_transition() {
    let server = MockServer::builder().start().await;
    mount_issue(&server, "closed", Some("not_planned"), 0).await;
    let timeline = json!({"totalCount": 3, "nodes": [
        closed_by(FIRST_CLOSER, "COMPLETED", "2026-11-02T00:00:00Z"),
        {"__typename": "ReopenedEvent", "createdAt": "2026-11-05T00:00:00Z",
            "actor": {"login": "alice"}},
        closed_by(LAST_CLOSER, "NOT_PLANNED", "2026-11-10T18:28:25Z")
    ]});
    mount_graphql(
        &server,
        json!({"timelineItems": timeline, "closedByPullRequestsReferences": references(json!([]))}),
        None,
    )
    .await;
    let served = served(&server, issue_read()).await;
    let issue = issue_row(&served);
    assert_eq!(
        issue["closedBy"],
        json!([{"commitSha": LAST_CLOSER}]),
        "{served}"
    );
    assert_eq!(issue["stateReason"], "not_planned", "{served}");
    let events = issue["stateEvents"].as_array().expect("stateEvents");
    assert_eq!(
        events
            .iter()
            .map(|event| event["event"].clone())
            .collect::<Vec<_>>(),
        [json!("closed"), json!("reopened"), json!("closed")],
        "{served}"
    );
    assert_eq!(events[0]["commitSha"], FIRST_CLOSER, "{served}");
    assert_eq!(events[0]["stateReason"], "completed", "{served}");
    assert_eq!(events[1]["by"], "alice", "{served}");
    assert_eq!(events[2]["stateReason"], "not_planned", "{served}");
}

/// One close by a merged pull request: `closedBy` lists the pull request,
/// the reason is stated, and a single transition lists no events.
#[tokio::test]
async fn one_transition_lists_no_events() {
    let server = MockServer::builder().start().await;
    mount_issue(&server, "closed", Some("completed"), 0).await;
    let timeline = json_fixture(
        r#"{
            "totalCount": 1,
            "nodes": [
                {
                    "__typename": "ClosedEvent",
                    "createdAt": "2026-11-10T18:28:25Z",
                    "stateReason": "COMPLETED",
                    "actor": {"login": "maintainer"},
                    "closer": {
                        "__typename": "PullRequest",
                        "number": 9,
                        "repository": {"nameWithOwner": "a/b"}
                    }
                }
            ]
        }"#,
    );
    let nodes = json_fixture(
        r#"[
            {
                "number": 9,
                "state": "MERGED",
                "mergedAt": "2026-11-10T18:28:24Z"
            }
        ]"#,
    );
    mount_graphql(
        &server,
        json!({"timelineItems": timeline, "closedByPullRequestsReferences": references(nodes)}),
        None,
    )
    .await;
    let served = served(&server, issue_read()).await;
    let issue = issue_row(&served);
    assert_eq!(
        issue["closedBy"],
        json_fixture(
            r#"[
                {
                    "number": 9,
                    "state": "merged",
                    "mergedAt": "2026-11-10T18:28:24Z"
                }
            ]"#
        ),
        "{served}"
    );
    assert_eq!(issue["stateReason"], "completed", "{served}");
    assert!(issue.get("stateEvents").is_none(), "{served}");
}

/// Without GraphQL the REST issue still says why it closed.
#[tokio::test]
async fn a_closed_issue_states_its_reason_without_graphql() {
    let server = MockServer::builder().start().await;
    mount_issue(&server, "closed", Some("not_planned"), 0).await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let served = served(&server, issue_read()).await;
    let issue = issue_row(&served);
    assert_eq!(issue["stateReason"], "not_planned", "{served}");
    assert!(issue.get("closedBy").is_none(), "{served}");
}

/// GitHub nulled a linked pull request the token may not read: the read
/// lists the closer it could read, says it is partial, and searches for
/// the pull requests that name the issue.
#[tokio::test]
async fn a_withheld_closing_pull_request_makes_the_read_partial() {
    let server = MockServer::builder().start().await;
    mount_issue(&server, "closed", Some("completed"), 0).await;
    let nodes = json_fixture(
        r#"[
            null,
            {
                "number": 5,
                "state": "MERGED",
                "mergedAt": "2026-11-10T18:28:24Z"
            }
        ]"#,
    );
    mount_graphql(
        &server,
        json!({"closedByPullRequestsReferences": references(nodes)}),
        Some(json_fixture(
            r#"[
                {
                    "type": "FORBIDDEN",
                    "message": "Resource protected by organization SAML enforcement.",
                    "path": [
                        "repository",
                        "issue",
                        "closedByPullRequestsReferences",
                        "nodes",
                        0
                    ]
                }
            ]"#,
        )),
    )
    .await;
    let served = served(&server, issue_read()).await;
    let data = data(&served);
    let numbers = data["issues"][0]["closedBy"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|closer| closer["number"].clone())
        .collect::<Vec<_>>();
    assert_eq!(numbers, [json!(5)], "{served}");
    assert!(
        data["partialReasons"]
            .as_array()
            .is_some_and(|reasons| !reasons.is_empty()),
        "the read says it is partial: {served}"
    );
    assert!(
        related(data, "ghSearchHistory")
            .iter()
            .any(|call| call["input"]["queries"][0]["operation"] == "pullRequest"),
        "a search for the other closers: {served}"
    );
}

/// The same answer without errors is the whole closing record.
#[tokio::test]
async fn a_complete_closing_answer_is_not_partial() {
    let server = MockServer::builder().start().await;
    mount_issue(&server, "closed", Some("completed"), 0).await;
    let nodes = json_fixture(
        r#"[
            {
                "number": 5,
                "state": "MERGED",
                "mergedAt": "2026-11-10T18:28:24Z"
            }
        ]"#,
    );
    mount_graphql(
        &server,
        json!({"closedByPullRequestsReferences": references(nodes)}),
        None,
    )
    .await;
    let served = served(&server, issue_read()).await;
    let data = data(&served);
    assert_eq!(data["issues"][0]["closedBy"][0]["number"], 5, "{served}");
    for partial in ["isPartial", "partialReasons", "terminalLimit"] {
        assert!(data.get(partial).is_none(), "{partial}: {served}");
    }
}

/// A discussion page keeps GitHub's order: a bot comment between two human
/// ones stays between them, marked `bot`.
#[tokio::test]
async fn issue_comments_keep_chronological_order_with_bots_marked() {
    let server = MockServer::builder().start().await;
    mount_issue(&server, "open", None, 3).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/issues/42/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(discussion()))
        .mount(&server)
        .await;
    let mut read = issue_read();
    read["sections"] = json_fixture(r#"["comments"]"#);
    let served = served(&server, read).await;
    assert_chronological(&issue_row(&served)["comments"], &served);
}

/// Three comments a day apart: a human, a bot, a human.
pub(super) fn discussion() -> Value {
    json_fixture(
        r#"[
            {
                "id": 1,
                "user": {"login": "reporter", "type": "User"},
                "body": "comment 1",
                "created_at": "2026-11-01T00:00:00Z"
            },
            {
                "id": 2,
                "user": {"login": "ci[bot]", "type": "Bot"},
                "body": "comment 2",
                "created_at": "2026-11-02T00:00:00Z"
            },
            {
                "id": 3,
                "user": {"login": "reporter", "type": "User"},
                "body": "comment 3",
                "created_at": "2026-11-03T00:00:00Z"
            }
        ]"#,
    )
}

/// `comments` lists [`discussion`] in its order, only the bot's marked.
pub(super) fn assert_chronological(comments: &Value, served: &Value) {
    let order = comments
        .as_array()
        .into_iter()
        .flatten()
        .map(|comment| (comment["id"].clone(), comment.get("bot").cloned()))
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        [
            (json!("1"), None),
            (json!("2"), Some(json!(true))),
            (json!("3"), None)
        ],
        "{served}"
    );
}
