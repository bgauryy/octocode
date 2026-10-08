//! Test doubles shared by the GitHub tool tests: a provider aimed at a mock
//! server and one-line JSON routes.
use crate::providers::RequestBudget;
use crate::providers::github::{
    CredentialSource, GitHubBudget, GitHubEndpoint, GitHubProvider, GitHubTransport, NoCache,
    RequestContext, ResolvedCredential, RetryPolicy,
};
use std::time::Duration;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A provider whose GitHub API is `server`, with its own unthrottled budget: tests in one process never wait on each
/// other's search spacing (the process-wide budget is the github crate's
/// subject).
pub(crate) fn mock_provider(server: &MockServer, retry: RetryPolicy) -> GitHubProvider<NoCache> {
    let endpoint =
        GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
            .expect("endpoint");
    GitHubProvider {
        transport: GitHubTransport::with_budget(endpoint, retry, GitHubBudget::relaxed())
            .expect("transport"),
        cache: NoCache,
    }
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

/// Answer `GET route` on `server` with `status` and a JSON `body`.
pub(crate) async fn mount_json(
    server: &MockServer,
    route: impl Into<String>,
    status: u16,
    body: impl serde::Serialize,
) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(server)
        .await;
}
