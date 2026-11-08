//! GH-G7: an item read whose repository does not resolve answers the
//! shared repository-not-found failure with its ghSearchRepo leads, after
//! one repository read; an existing repository keeps the item's failure.
use super::*;
use crate::security::scan::Passthrough;
use crate::tools::gh_get_history_item::OR_REPOSITORY;
use crate::tools::gh_shared::test_support::{default_context, one_shot_provider};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A server that answers 404 to every request, except `/repos/a/b` when
/// `repository` holds its metadata.
async fn server(repository: Option<Value>) -> MockServer {
    let server = MockServer::builder().start().await;
    if let Some(metadata) = repository {
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b"))
            .respond_with(ResponseTemplate::new(200).set_body_json(metadata))
            .mount(&server)
            .await;
    }
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    server
}

async fn fail(server: &MockServer, row: Value) -> GhFailure {
    let provider = one_shot_provider(server);
    let request = super::super::HistoryItemRequest::from_row(&row).expect("row");
    super::super::run(&provider, &request, &row, &default_context(), &Passthrough)
        .await
        .expect_err("not found")
}

async fn sent(server: &MockServer) -> usize {
    server.received_requests().await.expect("recorded").len()
}

#[tokio::test]
async fn a_missing_repository_leads_to_a_repository_search() {
    for row in [
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":1}),
        json!({"operation":"issue","owner":"a","repo":"b","number":1}),
        json!({"operation":"commit","owner":"a","repo":"b","ref":"0123456789abcdef0123456789abcdef01234567"}),
    ] {
        let server = server(None).await;
        let failure = fail(&server, row.clone()).await;
        assert_eq!(
            failure.error.reason,
            Some(ProviderErrorReason::RepositoryNotFound),
            "{row}: {}",
            failure.message
        );
        assert_eq!(
            failure.message,
            crate::tools::gh_shared::repository_not_found_message("a", "b"),
            "{row}"
        );
        let calls = failure.calls.as_deref().expect("leads");
        assert_eq!(calls["findRepository"]["tool"], "ghSearchRepo", "{calls}");
        assert_eq!(
            calls["findRepository"]["input"]["queries"][0]["keywords"],
            json!(["b"]),
            "{calls}"
        );
        assert_eq!(calls["findRepository2"]["tool"], "ghSearchRepo", "{calls}");
        assert!(
            calls.get("findPullRequest").is_none() && calls.get("findCommit").is_none(),
            "no lead into the missing repository: {calls}"
        );
        assert!(
            !failure.hints.is_empty(),
            "the ghSearchRepo leads are labelled"
        );
        // The item read, its probe as the other kind of numbered item (a
        // commit has none), and the repository read.
        let limit = if row["operation"] == "commit" { 2 } else { 3 };
        assert!(
            sent(&server).await == limit,
            "{row}: {} requests",
            sent(&server).await
        );
    }
}

#[tokio::test]
async fn an_existing_repository_keeps_the_item_failure() {
    let server = server(Some(
        json!({"default_branch":"main","full_name":"a/b","private":false,"archived":false}),
    ))
    .await;
    let failure = fail(
        &server,
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":1}),
    )
    .await;
    assert_eq!(failure.error.kind, ProviderErrorKind::NotFound);
    assert_ne!(
        failure.error.reason,
        Some(ProviderErrorReason::RepositoryNotFound)
    );
    assert!(
        failure.message.contains("Pull request #1 not found"),
        "{}",
        failure.message
    );
    let calls = failure.calls.as_deref().expect("leads");
    assert_eq!(
        calls["findPullRequest"]["tool"], "ghSearchHistory",
        "{calls}"
    );
    assert_eq!(sent(&server).await, 3);
    assert!(
        !failure.message.contains(OR_REPOSITORY),
        "the repository read proved it exists: {}",
        failure.message
    );
}

/// A repository read that fails without an answer proves nothing: the
/// message keeps naming the repository as a possible cause.
#[tokio::test]
async fn an_unchecked_repository_keeps_the_honest_clause() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(500).set_body_json(json!({"message":"boom"})))
        .mount(&server)
        .await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    let failure = fail(
        &server,
        json!({"operation":"issue","owner":"a","repo":"b","number":1}),
    )
    .await;
    assert!(
        failure.message.contains(OR_REPOSITORY),
        "{}",
        failure.message
    );
}

/// A repository read refused (rate limit, bad credentials, forbidden) says
/// nothing about the repository: the item's not-found message keeps the
/// honest clause and the repository is never marked resolved.
#[tokio::test]
async fn a_refused_repository_read_keeps_the_honest_clause() {
    for status in [429_u16, 401, 403] {
        let server = MockServer::builder().start().await;
        Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b"))
            .respond_with(ResponseTemplate::new(status).set_body_json(json!({"message":"refused"})))
            .mount(&server)
            .await;
        Mock::given(wiremock::matchers::any())
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
            .mount(&server)
            .await;
        let provider = one_shot_provider(&server);
        let context = default_context();
        let row = json!({"operation":"issue","owner":"a","repo":"b","number":1});
        let request = super::super::HistoryItemRequest::from_row(&row).expect("row");
        let failure = super::super::run(&provider, &request, &row, &context, &Passthrough)
            .await
            .expect_err("not found");
        assert!(
            failure.message.contains(OR_REPOSITORY),
            "{status}: {}",
            failure.message
        );
        assert!(
            !context.repository_resolves("a", "b"),
            "{status}: a refused read must not mark the repository resolved"
        );
    }
}

/// The clause follows the typed `repository_proved` flag, for every operation.
#[test]
fn the_clause_follows_the_repository_proved_flag() {
    for row in [
        json!({"operation":"pullRequest","owner":"a","repo":"b","number":1}),
        json!({"operation":"issue","owner":"a","repo":"b","number":1}),
        json!({"operation":"commit","owner":"a","repo":"b","ref":"0123456789abcdef0123456789abcdef01234567"}),
        json!({"operation":"compare","owner":"a","repo":"b","baseRef":"x","headRef":"y"}),
    ] {
        let request = super::super::HistoryItemRequest::from_row(&row).expect("row");
        let unproven = super::super::not_found_message(&request, None, false);
        let proved = super::super::not_found_message(&request, None, true);
        assert!(unproven.ends_with(OR_REPOSITORY), "{row}: {unproven}");
        assert!(!proved.contains(OR_REPOSITORY), "{row}: {proved}");
        assert!(unproven.starts_with(proved.trim_end_matches(',')), "{row}");
    }
}
