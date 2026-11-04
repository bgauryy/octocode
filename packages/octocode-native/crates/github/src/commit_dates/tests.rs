//! A path's last commit: the GraphQL history, the failures it names, and the
//! REST fallback.
use super::*;
use crate::{CredentialSource, GitHubEndpoint, ResolvedCredential, RetryPolicy};
use octocode_io::budget::RequestBudget;
use std::time::Duration;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn transport(server: &MockServer) -> GitHubTransport {
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).unwrap()).unwrap();
    GitHubTransport::new(
        endpoint,
        RetryPolicy {
            max_attempts: 1,
            base_delay: Duration::from_millis(1),
            max_retry_after: Duration::from_secs(1),
        },
        crate::GitHubBudget::relaxed(),
    )
    .unwrap()
}

/// A request budget; `token` attaches a credential.
fn context(token: bool) -> RequestContext {
    RequestContext::new(
        RequestBudget::with_timeout(Duration::from_secs(5), 1 << 20),
        token.then(|| ResolvedCredential::new("secret", CredentialSource::Environment)),
    )
}

fn graphql_bodies(requests: &[Request]) -> Vec<Value> {
    requests
        .iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect()
}

/// A server whose GraphQL answers with `history`.
async fn answering(history: Value) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data":{"repository":{"object":{"history": history}}}})),
        )
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn quotes_backslashes_and_unicode_travel_as_variables() {
    let server = answering(json!({"nodes":[{"committedDate":"2025-02-04T00:00:00Z"}]})).await;
    let tricky = [
        r#"a "quoted" name.md"#,
        r"back\slash\}.txt",
        "docs/日本語 ✓.md",
        "x){repository(owner:\"evil\"",
    ];
    let transport = transport(&server);
    for raw in tricky {
        let last = transport
            .path_last_commit("a", "b", SHA, raw, &context(true))
            .await
            .expect("last commit");
        assert_eq!(last.0.as_deref(), Some("2025-02-04T00:00:00Z"));
    }
    let bodies = graphql_bodies(&server.received_requests().await.unwrap());
    assert_eq!(bodies.len(), tricky.len());
    for (body, raw) in bodies.iter().zip(tricky) {
        assert_eq!(body["query"], LAST_COMMIT_DOCUMENT);
        assert_eq!(
            body["variables"],
            json!({"owner":"a","name":"b","oid":SHA,"path":raw})
        );
    }
}

#[tokio::test]
async fn a_graphql_error_names_the_failure() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": null,
            "errors": [{"message": "Timeout on validation of query"}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let error = transport(&server)
        .path_last_commit("a", "b", SHA, "one", &context(true))
        .await
        .expect_err("error");
    assert!(error.message.contains("Timeout on validation"), "{error:?}");
}

/// A commit answered with no history, and an error beside it, is no date: the
/// error is the answer.
#[tokio::test]
async fn an_error_that_left_the_history_unanswered_is_the_answer() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": {"repository":{"object":{"history":null}}},
            "errors": [{"message": "history unavailable"}]
        })))
        .mount(&server)
        .await;
    let error = transport(&server)
        .path_last_commit("a", "b", SHA, "one", &context(true))
        .await
        .expect_err("error");
    assert_eq!(error.kind, ProviderErrorKind::Decode);
    assert!(error.message.contains("history unavailable"), "{error:?}");
}

/// One file's last commit comes from the GraphQL history, with its author:
/// the linked login, else the git name. No REST commit list is read.
#[tokio::test]
async fn a_files_last_commit_is_the_graphql_history_with_its_author() {
    for (author, expected) in [
        (json!({"name": "Ada", "user": {"login": "ada-l"}}), "ada-l"),
        (json!({"name": "Ada", "user": null}), "Ada"),
    ] {
        let server = answering(json!({"nodes": [{
            "committedDate": "2026-01-02T00:00:00Z", "author": author
        }]}))
        .await;
        let last = transport(&server)
            .path_last_commit("a", "b", SHA, "a.txt", &context(true))
            .await
            .expect("last commit");
        assert_eq!(
            last,
            (Some("2026-01-02T00:00:00Z".into()), Some(expected.into()))
        );
        assert!(
            server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|request| request.url.path() == "/api/graphql")
        );
    }
}

/// When GraphQL cannot be sent (no token), the one path is answered by one REST
/// commit-list request instead, with the same fields.
#[tokio::test]
async fn a_files_last_commit_reads_the_commit_list_when_graphql_cannot_be_sent() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .and(wiremock::matchers::query_param("path", "a.txt"))
        .and(wiremock::matchers::query_param("sha", SHA))
        .and(wiremock::matchers::query_param("per_page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "author": null,
            "commit": {"committer": {"date": "2026-01-02T00:00:00Z"}, "author": {"name": "Ada"}}
        }])))
        .expect(1)
        .mount(&server)
        .await;
    let expected = (Some("2026-01-02T00:00:00Z".into()), Some("Ada".into()));
    let anonymous = transport(&server)
        .path_last_commit("a", "b", SHA, "a.txt", &context(false))
        .await
        .expect("last commit");
    assert_eq!(anonymous, expected);
    assert!(graphql_bodies(&server.received_requests().await.unwrap()).is_empty());
}

#[tokio::test]
async fn a_path_without_history_has_no_date() {
    let server = answering(json!({"nodes": []})).await;
    let last = transport(&server)
        .path_last_commit("a", "b", SHA, "gone", &context(true))
        .await
        .expect("answered");
    assert_eq!(last, (None, None));
}
