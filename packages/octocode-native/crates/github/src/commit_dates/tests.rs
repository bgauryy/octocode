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

fn wants(count: usize) -> Vec<LastCommitWant> {
    (0..count)
        .map(|index| LastCommitWant {
            owner: "a".into(),
            repo: "b".into(),
            sha: SHA.into(),
            path: format!("f{index}"),
        })
        .collect()
}

/// Paths of one commit travel in requests of at most [`WARM_CHUNK`], each
/// answered path is read from the memo with no request of its own, and an
/// alias the answer lacks stays unanswered.
#[tokio::test]
async fn warm_chunks_by_a_hundred_and_a_missing_alias_is_left_unanswered() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(|request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let mut object = serde_json::Map::new();
            for name in body["variables"].as_object().unwrap().keys() {
                // `h0` is never answered.
                if let Some(index) = name.strip_prefix('p').filter(|index| *index != "0") {
                    object.insert(
                        format!("p{index}"),
                        json!({"nodes":[{"committedDate":"2026-03-03T00:00:00Z"}]}),
                    );
                }
            }
            ResponseTemplate::new(200)
                .set_body_json(json!({"data":{"repository":{"object":object}}}))
        })
        .mount(&server)
        .await;
    let transport = transport(&server);
    let memo = std::sync::Arc::new(LastCommitMemo::new());
    let asked = wants(WARM_CHUNK + 1);
    transport
        .warm_last_commits(&asked, &memo, &context(true))
        .await;
    memo.refs_known();
    memo.settle();
    let posts = graphql_bodies(&server.received_requests().await.unwrap());
    assert_eq!(posts.len(), 2);
    let mut sizes = posts
        .iter()
        .map(|body| body["variables"].as_object().unwrap().len() - 3)
        .collect::<Vec<_>>();
    sizes.sort_unstable();
    assert_eq!(sizes, [1, WARM_CHUNK]);
    let with_memo = context(true).with_last_commits(memo.clone());
    let last = transport
        .path_last_commit("a", "b", SHA, "f1", &with_memo)
        .await
        .expect("memo");
    assert_eq!(last.0.as_deref(), Some("2026-03-03T00:00:00Z"));
    // Answered paths sent nothing more.
    assert_eq!(
        graphql_bodies(&server.received_requests().await.unwrap()).len(),
        2
    );
}

/// Without a credential nothing is asked, and the row asks by itself.
#[tokio::test]
async fn warm_asks_nothing_without_a_credential() {
    let server = MockServer::builder().start().await;
    let memo = LastCommitMemo::new();
    transport(&server)
        .warm_last_commits(&wants(2), &memo, &context(false))
        .await;
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// One path asks the single-path document: the request a lone read always
/// sent, so its kept and recorded answers still serve it.
#[tokio::test]
async fn warm_of_one_path_sends_the_single_path_document() {
    let server = answering(json!({"nodes":[{"committedDate":"2026-04-04T00:00:00Z"}]})).await;
    let transport = transport(&server);
    let memo = std::sync::Arc::new(LastCommitMemo::new());
    transport
        .warm_last_commits(&wants(1), &memo, &context(true))
        .await;
    memo.settle();
    let posts = graphql_bodies(&server.received_requests().await.unwrap());
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0]["query"], LAST_COMMIT_DOCUMENT);
    assert_eq!(
        posts[0]["variables"],
        json!({"owner":"a","name":"b","oid":SHA,"path":"f0"})
    );
    let with_memo = context(true).with_last_commits(memo);
    let last = transport
        .path_last_commit("a", "b", SHA, "f0", &with_memo)
        .await
        .expect("memo");
    assert_eq!(last.0.as_deref(), Some("2026-04-04T00:00:00Z"));
}

/// A refused credential fails each warmed path as its own GraphQL request
/// would have; no REST request that cannot succeed follows.
#[tokio::test]
async fn warm_refused_for_the_credential_fails_the_paths_without_rest() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message":"Bad credentials"})),
        )
        .mount(&server)
        .await;
    let transport = transport(&server);
    let memo = std::sync::Arc::new(LastCommitMemo::new());
    transport
        .warm_last_commits(&wants(2), &memo, &context(true))
        .await;
    memo.settle();
    let with_memo = context(true).with_last_commits(memo);
    let error = transport
        .path_last_commit("a", "b", SHA, "f1", &with_memo)
        .await
        .expect_err("refused");
    assert_eq!(error.kind, ProviderErrorKind::Authentication, "{error:?}");
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| request.url.path() == "/api/graphql")
    );
}

/// An error a warm-up recorded for a ref names the repository as the row that
/// reads it spells it: another spelling resolves for itself.
#[tokio::test]
async fn a_recorded_ref_error_serves_only_the_spelling_it_named() {
    let memo = LastCommitMemo::new();
    let error = ProviderError::new(ProviderErrorKind::NotFound, "Repository A/B not found");
    memo.record_ref("A", "B", None, Err(error));
    memo.refs_known();
    assert!(memo.resolved_ref("A", "B", None).await.is_some());
    assert!(memo.resolved_ref("a", "b", None).await.is_none());
}

/// One rule for a refused GraphQL request: a refused credential fails the
/// path (REST would be refused too); any other failure leaves it to REST.
#[test]
fn a_failed_graphql_request_fails_the_path_only_for_the_credential() {
    let refused = ProviderError::new(ProviderErrorKind::Authentication, "Bad credentials");
    assert!(matches!(Warmed::of_failure(refused), Warmed::Failed(_)));
    let other = ProviderError::new(ProviderErrorKind::NotFound, "gone");
    assert!(matches!(Warmed::of_failure(other), Warmed::Unanswered));
}

/// A saved-answer store that outlives the requests (and so the processes)
/// reading through it.
#[derive(Default)]
struct Kept(Mutex<HashMap<(String, String), crate::SavedAnswer>>);

impl crate::SavedAnswers for Kept {
    fn load(&self, scope: &str, request: &str) -> Option<crate::SavedAnswer> {
        let map = self.0.lock().unwrap();
        map.get(&(scope.to_owned(), request.to_owned())).cloned()
    }
    fn save(&self, scope: &str, request: &str, saved: crate::SavedAnswer) {
        let mut map = self.0.lock().unwrap();
        map.insert((scope.to_owned(), request.to_owned()), saved);
    }
    fn forget(&self, scope: &str, request: &str) {
        let mut map = self.0.lock().unwrap();
        map.remove(&(scope.to_owned(), request.to_owned()));
    }
}

/// A commit's date never changes: a later process, with a fresh per-batch
/// memo, reads the saved answer and asks the provider nothing, over GraphQL
/// and over the REST commit list alike.
#[tokio::test]
async fn a_last_commit_read_again_by_a_later_process_asks_nothing() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"repository":{
            "object":{"history":{"nodes":[{"committedDate":"2026-01-02T00:00:00Z"}]}}}}})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{
            "author": null,
            "commit": {"committer": {"date": "2026-01-02T00:00:00Z"}, "author": {"name": "Ada"}}
        }])))
        .mount(&server)
        .await;
    let store = std::sync::Arc::new(Kept::default());
    for token in [true, false] {
        let process = || {
            let mut process = transport(&server);
            process.set_saved_answers(Some(store.clone()));
            process
        };
        let first = process()
            .path_last_commit("a", "b", SHA, "a.txt", &context(token))
            .await
            .expect("first process");
        let asked = server.received_requests().await.unwrap().len();
        let second = process()
            .path_last_commit("a", "b", SHA, "a.txt", &context(token))
            .await
            .expect("second process");
        assert_eq!(second, first);
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            asked,
            "graphql={token}: the second process asked the provider"
        );
    }
}
