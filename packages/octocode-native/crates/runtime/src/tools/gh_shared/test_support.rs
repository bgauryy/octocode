//! Test doubles shared by the GitHub tool tests: a provider aimed at a mock
//! server and the request context the runtime attaches. The wiremock mounts
//! and responders are `octocode_github::mock_http`.
use octocode_github::{
    CredentialSource, GitHubBudget, GitHubEndpoint, GitHubProvider, GitHubTransport,
    RequestContext, ResolvedCredential, RetryPolicy,
};
pub(crate) mod backed;

use octocode_io::budget::RequestBudget;
use std::time::Duration;
use wiremock::MockServer;

/// A provider whose GitHub API is `server`, with its own unthrottled budget: tests in one process never wait on each
/// other's search spacing (the process-wide budget is the github crate's
/// subject).
pub(crate) fn mock_provider(server: &MockServer, retry: RetryPolicy) -> GitHubProvider {
    provider_at(mock_endpoint(server), retry)
}

/// The GitHub Enterprise-style API endpoint `server` serves.
pub(crate) fn mock_endpoint(server: &MockServer) -> GitHubEndpoint {
    GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
        .expect("endpoint")
}

fn provider_at(endpoint: GitHubEndpoint, retry: RetryPolicy) -> GitHubProvider {
    GitHubProvider::new(
        GitHubTransport::new(endpoint, retry, GitHubBudget::relaxed()).expect("transport"),
    )
}

/// [`mock_provider`] that sends each request once: a failure surfaces as it
/// happened instead of after retries.
pub(crate) fn one_shot_provider(server: &MockServer) -> GitHubProvider {
    one_shot_provider_at(mock_endpoint(server))
}

/// [`one_shot_provider`] for any `endpoint`, such as one nothing listens on.
pub(crate) fn one_shot_provider_at(endpoint: GitHubEndpoint) -> GitHubProvider {
    provider_at(
        endpoint,
        RetryPolicy {
            max_attempts: 1,
            ..Default::default()
        },
    )
}

/// A request carrying the fixed `fixture` credential, as the runtime
/// attaches the credential it resolved for each request.
pub(crate) fn fixture_context(timeout: Duration, max_body_bytes: usize) -> RequestContext {
    RequestContext::new(
        RequestBudget::with_timeout(timeout, max_body_bytes),
        Some(ResolvedCredential::new(
            "fixture",
            CredentialSource::Override,
        )),
    )
}

/// [`fixture_context`] with the budget most tests need: 5 s and a 1 MiB body.
pub(crate) fn default_context() -> RequestContext {
    fixture_context(Duration::from_secs(5), 1 << 20)
}

/// A renamed repository as GitHub serves it: its metadata names `c/d`, and
/// every other GET under `/repos/a/b/` answers 301 to the same route and
/// query under `/repositories/1`, which the test mounts.
pub(crate) async fn mount_renamed(server: &MockServer) {
    use wiremock::matchers::{method, path, path_regex};
    use wiremock::{Mock, ResponseTemplate};
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "default_branch":"main","full_name":"c/d","private":false,"archived":false
        })))
        .mount(server)
        .await;
    let origin = server.uri();
    Mock::given(method("GET"))
        .and(path_regex("^/api/v3/repos/a/b/"))
        .respond_with(move |request: &wiremock::Request| {
            let route = request
                .url
                .path()
                .replacen("/repos/a/b/", "/repositories/1/", 1);
            let query = request
                .url
                .query()
                .map_or_else(String::new, |query| format!("?{query}"));
            ResponseTemplate::new(301).insert_header("location", format!("{origin}{route}{query}"))
        })
        .mount(server)
        .await;
}

/// The repository (`owner`, `repo`) each call in `data` names: every query
/// row of every `input.queries`, wherever the row or its leads hold it.
pub(crate) fn lead_repositories(data: &serde_json::Value) -> Vec<(String, String)> {
    use serde_json::Value;
    let mut named = Vec::new();
    let mut pending = vec![data];
    while let Some(value) = pending.pop() {
        match value {
            Value::Array(items) => pending.extend(items),
            Value::Object(fields) => {
                let rows = fields
                    .get("input")
                    .and_then(|input| input.get("queries"))
                    .and_then(Value::as_array);
                for row in rows.into_iter().flatten() {
                    if let (Some(owner), Some(repo)) = (row["owner"].as_str(), row["repo"].as_str())
                    {
                        named.push((owner.to_owned(), repo.to_owned()));
                    }
                }
                pending.extend(fields.values());
            }
            _ => {}
        }
    }
    named
}

/// Asserts that `data` leads somewhere and that every lead names `c/d`, the
/// canonical name [`mount_renamed`] serves.
#[track_caller]
pub(crate) fn assert_leads_canonical(data: &serde_json::Value) {
    let named = lead_repositories(data);
    assert!(!named.is_empty(), "the row leads nowhere: {data}");
    for (owner, repo) in &named {
        assert_eq!(
            (owner.as_str(), repo.as_str()),
            ("c", "d"),
            "a lead names the old repository: {data}"
        );
    }
}
