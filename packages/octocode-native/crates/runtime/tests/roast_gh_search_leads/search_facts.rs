//! What a search page states about itself: coverage only where it
//! qualifies an absence or a partial answer, and whether a PR or issue
//! search total is exact.
use crate::support::{Workspace, call, row_data};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

async fn code_server(total: u64, incomplete: bool, items: Vec<Value>) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"total_count": total, "incomplete_results": incomplete, "items": items}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/o/r"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json_fixture(r#"{"default_branch": "main"}"#)),
        )
        .mount(&server)
        .await;
    server
}

fn hit() -> Value {
    json!({"name": "a.md", "path": "a.md", "sha": "1".repeat(40),
        "html_url": format!("https://github.com/o/r/blob/{SHA}/a.md"),
        "repository": {"full_name": "o/r", "html_url": "https://github.com/o/r",
                       "url": "https://api.github.com/repos/o/r"}})
}

async fn code(server: &MockServer) -> Value {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchCode",
        json_fixture(r#"{"owner": "o", "repo": "r", "keywords": ["needle"]}"#),
    )
    .await
    .expect("search");
    let data = row_data(&outcome).clone();
    runtime.close().await;
    data
}

#[tokio::test]
async fn a_hit_page_states_no_coverage() {
    let data = code(&code_server(1, false, vec![hit()]).await).await;
    assert!(
        data["files"]
            .as_array()
            .is_some_and(|files| !files.is_empty()),
        "{data}"
    );
    assert!(data.get("isPartial").is_none(), "{data}");
    assert!(data["coverage"].get("state").is_none(), "{data}");
}

#[tokio::test]
async fn an_empty_page_states_complete_coverage() {
    let data = code(&code_server(0, false, vec![]).await).await;
    assert_eq!(data["coverage"]["state"], "complete", "{data}");
}

#[tokio::test]
async fn a_partial_page_states_why() {
    let data = code(&code_server(1, true, vec![hit()]).await).await;
    assert!(data.get("partialReasons").is_some(), "{data}");
}

async fn history(total: u64, incomplete: bool, per_page_rows: usize) -> Value {
    let server = MockServer::builder().start().await;
    let items: Vec<Value> = (0..per_page_rows)
        .map(|n| {
            json!({"number": n + 1, "title": "t", "state": "open",
            "user": {"login": "x"}, "labels": [], "pull_request": {"url": "u"}})
        })
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"total_count": total, "incomplete_results": incomplete, "items": items}),
        ))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", format!("{}/api/v3", server.uri()))]);
    let outcome = call(
        &runtime,
        "ghSearchHistory",
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "keywords": ["fix"],
                "limit": 3
            }"#,
        ),
    )
    .await
    .expect("search");
    let data = row_data(&outcome).clone();
    runtime.close().await;
    data
}

#[tokio::test]
async fn an_exact_pr_search_total_says_it_is_complete() {
    let data = history(24, false, 3).await;
    // A stated total is exact, and no scope qualifies it.
    assert_eq!(data["pagination"]["total"], 24, "{data}");
    assert!(data["pagination"].get("countScope").is_none(), "{data}");
}

#[tokio::test]
async fn an_incomplete_pr_search_total_is_not_complete() {
    let data = history(24, true, 3).await;
    // A count GitHub could not finish is a floor: no total is stated, and
    // the row says why it is incomplete.
    assert!(data.pointer("/pagination/total").is_none(), "{data}");
    assert!(
        data["partialReasons"]
            .as_array()
            .is_some_and(|reasons| !reasons.is_empty()),
        "{data}"
    );
}
