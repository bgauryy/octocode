//! Path commit dates: one GraphQL request per chunk, the failures it names, and the anonymous
//! fallback.
use super::*;
use crate::{CredentialSource, GitHubEndpoint, ResolvedCredential, RetryPolicy};
use octocode_io::budget::RequestBudget;
use std::time::Duration;
use wiremock::{
    Mock, MockServer, Request, Respond, ResponseTemplate,
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
            max_rate_limit_wait: Duration::from_secs(1),
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

/// Answers every alias in the request with a date derived from its
/// path variable's length, so a test can tell which path got which date.
struct Echo;
impl Respond for Echo {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let mut object = Map::new();
        object.insert("committedDate".into(), json!("2026-01-31T10:00:00Z"));
        for (name, value) in body["variables"].as_object().unwrap() {
            if let Some(index) = name.strip_prefix('p') {
                let day = 1 + value.as_str().unwrap().chars().count() % 28;
                object.insert(
                    format!("p{index}"),
                    json!({"nodes":[{"committedDate": format!("2025-02-{day:02}T00:00:00Z")}]}),
                );
            }
        }
        ResponseTemplate::new(200).set_body_json(json!({"data":{"repository":{"object": object}}}))
    }
}

fn graphql_bodies(requests: &[Request]) -> Vec<Value> {
    requests
        .iter()
        .filter(|request| request.url.path() == "/api/graphql")
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect()
}

#[test]
fn the_document_names_each_path_by_variable_only() {
    let document = path_dates_document(2);
    assert_eq!(
        document,
        "query($owner:String!,$name:String!,$oid:GitObjectID!,$p0:String!,$p1:String!){repository(owner:$owner,name:$name){object(oid:$oid){...on Commit{committedDate p0:history(first:1,path:$p0){nodes{committedDate author{name user{login}}}} p1:history(first:1,path:$p1){nodes{committedDate author{name user{login}}}}}}}}"
    );
}

#[tokio::test]
async fn one_request_dates_every_path_and_the_commit() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(Echo)
        .expect(1)
        .mount(&server)
        .await;
    let dates = transport(&server)
        .path_commit_dates("a", "b", SHA, &["src", "README.md"], &context(true))
        .await;
    assert_eq!(dates.error, None);
    assert_eq!(dates.commit_date.as_deref(), Some("2026-01-31T10:00:00Z"));
    assert_eq!(
        dates.dates,
        vec![
            Some("2025-02-04T00:00:00Z".into()),
            Some("2025-02-10T00:00:00Z".into())
        ]
    );
    let bodies = graphql_bodies(&server.received_requests().await.unwrap());
    assert_eq!(
        bodies[0]["variables"],
        json!({"owner":"a","name":"b","oid":SHA,"p0":"src","p1":"README.md"})
    );
}

#[tokio::test]
async fn paths_past_one_hundred_are_chunked_in_order() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(Echo)
        .expect(3)
        .mount(&server)
        .await;
    let names = (0..201).map(|n| "x".repeat(n % 7 + 1)).collect::<Vec<_>>();
    let paths = names.iter().map(String::as_str).collect::<Vec<_>>();
    let dates = transport(&server)
        .path_commit_dates("a", "b", SHA, &paths, &context(true))
        .await;
    assert_eq!(dates.error, None);
    assert_eq!(dates.dates.len(), 201);
    for (name, date) in names.iter().zip(&dates.dates) {
        let day = 1 + name.len() % 28;
        assert_eq!(
            date.as_deref(),
            Some(format!("2025-02-{day:02}T00:00:00Z").as_str())
        );
    }
    let bodies = graphql_bodies(&server.received_requests().await.unwrap());
    let sizes = bodies
        .iter()
        .map(|body| body["variables"].as_object().unwrap().len() - 3)
        .collect::<Vec<_>>();
    assert_eq!(sizes, [100, 100, 1]);
}

#[tokio::test]
async fn quotes_backslashes_and_unicode_travel_as_variables() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(Echo)
        .mount(&server)
        .await;
    let tricky = [
        r#"a "quoted" name.md"#,
        r"back\slash\}.txt",
        "docs/日本語 ✓.md",
        "x){repository(owner:\"evil\"",
    ];
    let dates = transport(&server)
        .path_commit_dates("a", "b", SHA, &tricky, &context(true))
        .await;
    assert_eq!(dates.error, None);
    assert!(dates.dates.iter().all(Option::is_some), "{dates:?}");
    let body = &graphql_bodies(&server.received_requests().await.unwrap())[0];
    let document = body["query"].as_str().unwrap();
    assert_eq!(document, path_dates_document(4));
    for (index, raw) in tricky.iter().enumerate() {
        assert_eq!(body["variables"][format!("p{index}")], *raw);
        assert!(!document.contains(raw), "{document}");
    }
}

#[tokio::test]
async fn a_graphql_error_dates_nothing_and_names_the_failure() {
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
    let dates = transport(&server)
        .path_commit_dates("a", "b", SHA, &["one", "two"], &context(true))
        .await;
    assert_eq!(dates.dates, vec![None, None]);
    assert_eq!(dates.commit_date, None);
    let error = dates.error.expect("error");
    assert!(error.message.contains("Timeout on validation"), "{error:?}");
}

#[tokio::test]
async fn a_failed_chunk_stops_later_requests_and_keeps_earlier_dates() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(Echo)
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(502))
        .mount(&server)
        .await;
    let names = (0..250).map(|n| format!("f{n}")).collect::<Vec<_>>();
    let paths = names.iter().map(String::as_str).collect::<Vec<_>>();
    let dates = transport(&server)
        .path_commit_dates("a", "b", SHA, &paths, &context(true))
        .await;
    assert!(dates.error.is_some());
    assert!(dates.dates[..100].iter().all(Option::is_some));
    assert!(dates.dates[100..].iter().all(Option::is_none));
    assert_eq!(
        graphql_bodies(&server.received_requests().await.unwrap()).len(),
        2
    );
}

#[tokio::test]
async fn no_token_or_disabled_graphql_sends_nothing() {
    let server = MockServer::builder().start().await;
    let anonymous = transport(&server)
        .path_commit_dates("a", "b", SHA, &["one"], &context(false))
        .await;
    assert_eq!(
        anonymous.error.map(|error| error.kind),
        Some(ProviderErrorKind::Authentication)
    );
    let mut disabled = transport(&server);
    disabled.graphql_enabled = false;
    let disabled = disabled
        .path_commit_dates("a", "b", SHA, &["one"], &context(true))
        .await;
    assert_eq!(
        disabled.error.map(|error| error.kind),
        Some(ProviderErrorKind::Configuration)
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// One file's last commit comes from the same GraphQL history as a
/// listing's dates, with its author: the linked login, else the git
/// name. No REST commit list is read.
#[tokio::test]
async fn a_files_last_commit_is_the_graphql_history_with_its_author() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data":{"repository":{"object":{
                "committedDate": "2026-01-31T10:00:00Z",
                "p0": {"nodes": [{"committedDate": "2026-01-02T00:00:00Z",
                    "author": {"name": "Ada", "user": {"login": "ada-l"}}}]}
            }}}}),
        ))
        .expect(2)
        .mount(&server)
        .await;
    let transport = transport(&server);
    let last = transport
        .path_last_commit("a", "b", SHA, "a.txt", &context(true))
        .await
        .expect("last commit");
    assert_eq!(
        last,
        (Some("2026-01-02T00:00:00Z".into()), Some("ada-l".into()))
    );
    let listed = transport
        .path_commit_dates("a", "b", SHA, &["a.txt"], &context(true))
        .await;
    assert_eq!(listed.authors, vec![Some("ada-l".into())]);
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| request.url.path() == "/api/graphql")
    );
}

/// Without a token GraphQL cannot be sent: the one path is answered by
/// one REST commit-list request instead, with the same fields.
#[tokio::test]
async fn an_anonymous_files_last_commit_reads_the_commit_list() {
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
    let last = transport(&server)
        .path_last_commit("a", "b", SHA, "a.txt", &context(false))
        .await
        .expect("last commit");
    assert_eq!(
        last,
        (Some("2026-01-02T00:00:00Z".into()), Some("Ada".into()))
    );
}

#[tokio::test]
async fn a_path_without_history_has_no_date() {
    let server = MockServer::builder().start().await;
    Mock::given(method("POST"))
        .and(path("/api/graphql"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data":{"repository":{"object":{
                "committedDate": "2026-01-31T10:00:00Z",
                "p0": {"nodes": []},
                "p1": {"nodes": [{"committedDate": "2024-05-06T07:08:09Z"}]}
            }}}}),
        ))
        .mount(&server)
        .await;
    let dates = transport(&server)
        .path_commit_dates("a", "b", SHA, &["gone", "kept"], &context(true))
        .await;
    assert_eq!(dates.error, None);
    assert_eq!(dates.dates, vec![None, Some("2024-05-06T07:08:09Z".into())]);
}
