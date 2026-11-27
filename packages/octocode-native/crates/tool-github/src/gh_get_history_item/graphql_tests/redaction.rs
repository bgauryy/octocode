//! The GraphQL fast path redacts bodies as REST does: whole, before windows.
use super::*;
use octocode_runtime_base::security::ContentSecurity;
use octocode_test_support::json_fixture;

/// A pull-request body and a discussion comment served by GraphQL: a token
/// across the body's window edge shows in part on no page, and the comment's
/// token is redacted.
#[tokio::test]
async fn graphql_bodies_are_redacted_before_they_are_windowed() {
    let secret = format!("ghp_{}", "Zq7".repeat(12));
    let window = super::super::DEFAULT_TEXT_WINDOW;
    let body = format!(
        "{}{secret}{}",
        "a ".repeat((window - 10) / 2),
        " tail".repeat(10)
    );
    let mut answer = graphql_pr(json!({
        "commentsConn": {"pageInfo": {"hasNextPage": false}, "nodes": [
            {"databaseId": 11, "author": {"login": "amy", "__typename": "User"},
             "body": format!("my token {secret}"), "createdAt": "2026-01-01T00:00:00Z",
             "url": "https://x"}]}
    }));
    answer["data"]["repository"]["pullRequest"]["body"] = json!(body);
    let server = MockServer::builder().start().await;
    mount_graphql(&server, answer).await;
    let provider = one_shot_provider(&server);
    let request = HistoryItemRequest::from_row(json_fixture(
        r#"{
            "operation": "pullRequest",
            "owner": "a",
            "repo": "b",
            "number": 7,
            "sections": ["body", "comments"]
        }"#,
    ))
    .expect("valid row");
    let out = execute(
        &provider,
        &request,
        &default_context(),
        &ContentSecurity::new(),
    )
    .await
    .expect("history item")
    .data;
    let pr = &out["pullRequests"][0];
    let shown = pr.to_string();
    for at in 0..=secret.len() - 12 {
        assert!(!shown.contains(&secret[at..at + 12]), "{pr}");
    }
    assert_eq!(
        pr["body"].as_str().expect("body").chars().count(),
        window - 10,
        "{pr}"
    );
    assert!(pr["comments"].to_string().contains("[REDACTED"), "{pr}");
    let rest = server
        .received_requests()
        .await
        .expect("requests")
        .into_iter()
        .filter(|request| request.url.path().starts_with("/api/v3/"))
        .count();
    assert_eq!(rest, 0, "GraphQL served the page");
}
