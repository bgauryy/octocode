//! An item read whose repository does not resolve answers the
//! shared repository-not-found failure with its ghSearchRepo leads, after
//! one repository read; an existing repository keeps the item's failure.
use super::*;
use crate::gh_get_history_item::OR_REPOSITORY;
use octocode_runtime_base::security::scan::Passthrough;
use octocode_test_support::json_fixture;
use octocode_tools_shared::gh_shared::test_support::{default_context, one_shot_provider};
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
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
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

/// The REST reads `server` received, and its GraphQL reads apart: a pull
/// request summary asks its whole comment count over GraphQL beside the
/// item read (TB6), and a failed item read may end that read before it is
/// sent, so only the REST reads have one count.
async fn sent(server: &MockServer) -> (usize, usize) {
    let requests = server.received_requests().await.expect("recorded");
    let graphql = requests
        .iter()
        .filter(|request| request.url.path().ends_with("/graphql"))
        .count();
    (requests.len() - graphql, graphql)
}

#[tokio::test]
async fn a_missing_repository_leads_to_a_repository_search_beside_a_summary_count_read() {
    for row in [
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 1
            }"#,
        ),
        json_fixture(
            r#"{
                "operation": "issue",
                "owner": "a",
                "repo": "b",
                "number": 1
            }"#,
        ),
        json_fixture(
            r#"{
                "operation": "commit",
                "owner": "a",
                "repo": "b",
                "ref": "0123456789abcdef0123456789abcdef01234567"
            }"#,
        ),
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
            octocode_tools_shared::gh_shared::repository_not_found_message("a", "b"),
            "{row}"
        );
        let calls = failure.calls.as_deref().expect("leads");
        assert_eq!(calls["findRepository"]["tool"], "ghSearchRepo", "{calls}");
        assert_eq!(
            calls["findRepository"]["input"]["queries"][0]["keywords"],
            json_fixture(r#"["b"]"#),
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
        // The item read and the repository read; a pull request's also
        // probes the number as an issue, and may ask its whole comment
        // count beside the item read (TB6), which the failed read can end
        // unsent; an issue's always asks its closing pull requests beside it.
        let (rest, graphql) = sent(&server).await;
        let (reads, counts) = match row["operation"].as_str() {
            Some("commit") => (2, 0..=0),
            Some("pullRequest") => (3, 0..=1),
            _ => (2, 1..=1),
        };
        assert!(
            rest == reads && counts.contains(&graphql),
            "{row}: {rest} REST and {graphql} GraphQL requests"
        );
    }
}

#[tokio::test]
async fn an_existing_repository_keeps_the_item_failure_after_a_summary_count_read() {
    let server = server(Some(json_fixture(
        r#"{
            "default_branch": "main",
            "full_name": "a/b",
            "private": false,
            "archived": false
        }"#,
    )))
    .await;
    let failure = fail(
        &server,
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 1
            }"#,
        ),
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
    // The item read, the issue probe, the repository read, and at most the
    // summary's whole comment count read beside the item read (TB6).
    let (rest, graphql) = sent(&server).await;
    assert!(
        rest == 3 && graphql <= 1,
        "{rest} REST and {graphql} GraphQL"
    );
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
        .respond_with(
            ResponseTemplate::new(500).set_body_json(json_fixture(r#"{"message": "boom"}"#)),
        )
        .mount(&server)
        .await;
    Mock::given(wiremock::matchers::any())
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
        )
        .mount(&server)
        .await;
    let failure = fail(
        &server,
        json_fixture(
            r#"{
                "operation": "issue",
                "owner": "a",
                "repo": "b",
                "number": 1
            }"#,
        ),
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
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_json(json_fixture(r#"{"message": "refused"}"#)),
            )
            .mount(&server)
            .await;
        Mock::given(wiremock::matchers::any())
            .respond_with(
                ResponseTemplate::new(404)
                    .set_body_json(json_fixture(r#"{"message": "Not Found"}"#)),
            )
            .mount(&server)
            .await;
        let provider = one_shot_provider(&server);
        let context = default_context();
        let row = json_fixture(
            r#"{
                "operation": "issue",
                "owner": "a",
                "repo": "b",
                "number": 1
            }"#,
        );
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
        json_fixture(
            r#"{
                "operation": "pullRequest",
                "owner": "a",
                "repo": "b",
                "number": 1
            }"#,
        ),
        json_fixture(
            r#"{
                "operation": "issue",
                "owner": "a",
                "repo": "b",
                "number": 1
            }"#,
        ),
        json_fixture(
            r#"{
                "operation": "commit",
                "owner": "a",
                "repo": "b",
                "ref": "0123456789abcdef0123456789abcdef01234567"
            }"#,
        ),
        json_fixture(
            r#"{
                "operation": "compare",
                "owner": "a",
                "repo": "b",
                "baseRef": "x",
                "headRef": "y"
            }"#,
        ),
    ] {
        let request = super::super::HistoryItemRequest::from_row(&row).expect("row");
        let unproven = super::super::not_found_message(&request, None, false);
        let proved = super::super::not_found_message(&request, None, true);
        assert!(unproven.ends_with(OR_REPOSITORY), "{row}: {unproven}");
        assert!(!proved.contains(OR_REPOSITORY), "{row}: {proved}");
        assert!(unproven.starts_with(proved.trim_end_matches(',')), "{row}");
    }
}
