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
