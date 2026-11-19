//! A repeat of a multi-section pull-request read (GraphQL) within the lifetime
//! GitHub states for that pull request over REST: no request, disclosed by the
//! time the answer was observed. With no stated lifetime, the chosen
//! `PULL_REQUEST_REUSE_WINDOW` stands in.
use crate::support::{Workspace, call, row_data, row_status};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const BASE: &str = "1111111111111111111111111111111111111111";
const HEAD: &str = "2222222222222222222222222222222222222222";

fn graphql_answer() -> Value {
    json!({"data": {"repository": {"pullRequest": {
        "number": 7, "title": "Fix parser", "url": "https://x", "state": "OPEN",
        "body": "Fixes the parser.", "isDraft": false, "author": {"login": "bob"},
        "labels": {"pageInfo": {"hasNextPage": false}, "nodes": []},
        "baseRefName": "main", "baseRefOid": BASE, "headRefName": "feat", "headRefOid": HEAD,
        "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z",
        "closedAt": null, "mergedAt": null, "mergeCommit": null,
        "comments": {"totalCount": 0}, "changedFiles": 1, "additions": 1, "deletions": 0,
        "commitsCount": {"totalCount": 1}, "reviewThreads": {"totalCount": 0},
        "files": {"pageInfo": {"hasNextPage": false}, "nodes": [
            {"path": "src/a.rs", "additions": 1, "deletions": 0, "changeType": "MODIFIED"}
        ]}
    }}}})
}

/// `a/b#7`: GraphQL with `graphql` as its answer, and REST stating `cache_control`.
async fn server(graphql: Value, cache_control: &str) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(graphql))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/pulls/7"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("cache-control", cache_control)
                .set_body_json(json!({
                    "number": 7, "title": "Fix parser", "state": "open", "body": "b",
                    "user": {"login": "bob"}, "head": {"sha": HEAD, "ref": "feat"},
                    "base": {"sha": BASE, "ref": "main"}
                })),
        )
        .mount(&server)
        .await;
    server
}

/// The GraphQL reads of the pull request's sections: a summary read's own
/// whole-comment-count read (`totalCommentsCount`, TB6) is not one of them.
async fn graphql_sent(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("recorded")
        .iter()
        .filter(|request| {
            request.url.path() == "/api/graphql"
                && !String::from_utf8_lossy(&request.body).contains("totalCommentsCount")
        })
        .count()
}

fn summary() -> Value {
    json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 7
        }"#,
    )
}

fn sections() -> Value {
    json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 7,
            "sections": ["body", "files"]
        }"#,
    )
}

/// The stated lifetime learned from any REST read of a pull request stands for
/// the GraphQL read of one: its repeat sends nothing and says when it was
/// observed; the evidence is the same.
#[tokio::test]
async fn a_repeat_inside_the_stated_lifetime_sends_nothing_and_says_so() {
    let server = server(graphql_answer(), "private, max-age=60, s-maxage=60").await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    call(&runtime, "ghGetHistoryItem", summary())
        .await
        .expect("summary");
    let first = call(&runtime, "ghGetHistoryItem", sections())
        .await
        .expect("first");
    assert_eq!(row_status(&first), "success");
    assert_eq!(graphql_sent(&server).await, 1);
    let repeat = call(&runtime, "ghGetHistoryItem", sections())
        .await
        .expect("repeat");
    assert_eq!(graphql_sent(&server).await, 1, "the repeat asked again");
    let mut shown = row_data(&repeat).clone();
    let observed = shown
        .as_object_mut()
        .unwrap()
        .remove("observedAt")
        .expect("disclosed");
    assert!(observed.is_string(), "{observed}");
    assert_eq!(shown, row_data(&first).clone());
    // Another read of the same pull request asks for itself.
    let other = json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 7,
            "sections": ["body", "commits"]
        }"#,
    );
    call(&runtime, "ghGetHistoryItem", other)
        .await
        .expect("other");
    assert_eq!(graphql_sent(&server).await, 2);
    runtime.close().await;

    // A CLI call is its own process: the fact and the answer outlive it.
    let later = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let again = call(&later, "ghGetHistoryItem", sections())
        .await
        .expect("next process");
    assert_eq!(row_status(&again), "success");
    assert_eq!(
        graphql_sent(&server).await,
        2,
        "the next process asked again"
    );
    later.close().await;
}

/// Past the stated lifetime a read asks again.
#[tokio::test]
async fn a_repeat_past_the_stated_lifetime_asks_again() {
    let server = server(graphql_answer(), "private, max-age=1").await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    call(&runtime, "ghGetHistoryItem", summary())
        .await
        .expect("summary");
    call(&runtime, "ghGetHistoryItem", sections())
        .await
        .expect("first");
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let late = call(&runtime, "ghGetHistoryItem", sections())
        .await
        .expect("late");
    assert_eq!(graphql_sent(&server).await, 2);
    assert!(row_data(&late).get("observedAt").is_none());
    runtime.close().await;
}

/// GitHub stating no lifetime, or none seen yet, still lets a repeat inside the
/// chosen window stand for asking again, disclosed by when it was observed.
#[tokio::test]
async fn with_no_stated_lifetime_a_repeat_stands_for_the_chosen_window() {
    let unseen = server(graphql_answer(), "private, max-age=60").await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", unseen.uri()))]);
    call(&runtime, "ghGetHistoryItem", sections())
        .await
        .expect("read");
    let repeat = call(&runtime, "ghGetHistoryItem", sections())
        .await
        .expect("repeat");
    assert_eq!(graphql_sent(&unseen).await, 1, "the repeat asked again");
    assert!(row_data(&repeat)["observedAt"].is_string());
    runtime.close().await;

    let refused = server(graphql_answer(), "no-cache").await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", refused.uri()))]);
    call(&runtime, "ghGetHistoryItem", summary())
        .await
        .expect("summary");
    for _ in 0..2 {
        call(&runtime, "ghGetHistoryItem", sections())
            .await
            .expect("read");
    }
    assert_eq!(graphql_sent(&refused).await, 1);
    runtime.close().await;
}

/// An answer that reported errors is a failure a later ask may not repeat: it is
/// never reused, however long GitHub says a pull request stands.
#[tokio::test]
async fn an_answer_with_errors_is_never_reused() {
    let mut partial = graphql_answer();
    partial["errors"] = json_fixture(r#"[{"message": "field denied", "path": ["repository"]}]"#);
    let server = server(partial, "private, max-age=60").await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    call(&runtime, "ghGetHistoryItem", summary())
        .await
        .expect("summary");
    for _ in 0..2 {
        call(&runtime, "ghGetHistoryItem", sections())
            .await
            .expect("read");
    }
    assert_eq!(graphql_sent(&server).await, 2);
    runtime.close().await;
}
