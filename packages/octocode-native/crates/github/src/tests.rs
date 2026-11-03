//! octocode-github's tests, grouped by concern; the shared fixtures live here.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{sync::Arc, time::Duration};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

use super::*;
use crate::budget::ExecutorConfig;
use octocode_io::{budget::RequestBudget, civil::civil_from_days};

use crate::mock_http::{Arrivals, Delayed, Sequence, mount_json};

fn api_github_com() -> GitHubEndpoint {
    GitHubEndpoint::new(url::Url::parse("https://api.github.com/").expect("URL")).expect("endpoint")
}

/// A request with a `timeout` budget carrying `token`.
fn with_token(token: &str, timeout: Duration, max_body_bytes: usize) -> RequestContext {
    RequestContext::new(
        RequestBudget::with_timeout(timeout, max_body_bytes),
        Some(ResolvedCredential::new(
            token,
            CredentialSource::Environment,
        )),
    )
}

/// A 403 secondary rate limit, with `retry-after` when given.
fn secondary_limit(retry_after: Option<&str>) -> ResponseTemplate {
    let response = ResponseTemplate::new(403)
        .insert_header("x-ratelimit-remaining", "21")
        .set_body_json(serde_json::json!({
            "message": "You have exceeded a secondary rate limit. Please wait a few minutes before you try again."
        }));
    match retry_after {
        Some(retry_after) => response.insert_header("retry-after", retry_after),
        None => response,
    }
}

fn ok_json() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true}))
}

/// The GitHub endpoint `server` serves.
fn mock_endpoint(server: &MockServer) -> GitHubEndpoint {
    GitHubEndpoint::new(url::Url::parse(&format!("{}/api/v3", server.uri())).expect("URL"))
        .expect("endpoint")
}

/// An anonymous transport to `endpoint` with the default retry policy.
fn anonymous_transport(endpoint: &GitHubEndpoint) -> GitHubTransport {
    GitHubTransport::new(
        endpoint.clone(),
        RetryPolicy::default(),
        GitHubBudget::relaxed(),
    )
    .expect("transport")
}

/// A read of `path` in `a/b` at `main`.
fn main_file(path: &str) -> ContentRequest {
    ContentRequest {
        owner: "a".into(),
        repo: "b".into(),
        path: path.into(),
        reference: Some("main".into()),
    }
}

/// `GET x` through `transport`, with a 2 s deadline and a `max_bytes` body cap.
async fn get_x(
    transport: &GitHubTransport,
    endpoint: &GitHubEndpoint,
    max_bytes: usize,
) -> Result<ResponsePage, ProviderError> {
    transport
        .execute(
            RequestSpec::get(endpoint.rest(&["x"]).expect("route")),
            &RequestContext::with_timeout(Duration::from_secs(2), max_bytes),
        )
        .await
}

/// Requests to it carry the `"secret"` token through [`with_token`].
async fn provider(server: &MockServer) -> GitHubProvider {
    let endpoint = mock_endpoint(server);
    let transport = GitHubTransport::new(
        endpoint,
        RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(1),
            max_retry_after: Duration::from_secs(1),
        },
        GitHubBudget::relaxed(),
    )
    .expect("transport");
    GitHubProvider::new(transport)
}

fn breaker_budget(circuit_failures: u32) -> Arc<GitHubBudget> {
    Arc::new(GitHubBudget::with_config(ExecutorConfig {
        circuit_failures,
        circuit_open: Duration::from_secs(30),
        ..ExecutorConfig::relaxed()
    }))
}

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs()
}

/// `template` reporting 1000 requests left, resetting 1000 × `spacing` + 10 s
/// from now (under zero reset grace). Covered admissions burst; `spacing` is
/// the reference interval for tests contrasting that with even spreading.
fn reporting_quota_spacing(template: ResponseTemplate, spacing: Duration) -> ResponseTemplate {
    let reset = epoch_secs() + 10 + u64::try_from(spacing.as_millis()).expect("spacing");
    template
        .insert_header("x-ratelimit-remaining", "1000")
        .insert_header("x-ratelimit-reset", reset.to_string().as_str())
}

// ── Keyed executor: Octokit throttling/retry parity ─────────────────────────

fn executor_transport(
    server: &MockServer,
    budget: Arc<GitHubBudget>,
    retry: RetryPolicy,
) -> (GitHubTransport, GitHubEndpoint) {
    let endpoint = mock_endpoint(server);
    let transport = GitHubTransport::new(endpoint.clone(), retry, budget).expect("transport");
    (transport, endpoint)
}

async fn get_status(
    transport: &GitHubTransport,
    endpoint: &GitHubEndpoint,
    segments: &[&str],
) -> Result<ResponsePage, ProviderError> {
    transport
        .execute(
            RequestSpec::get(endpoint.rest(segments).expect("route")),
            &with_token("t", Duration::from_secs(5), 1024),
        )
        .await
}

fn short_retry() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_millis(1),
        max_retry_after: Duration::from_secs(5),
    }
}

mod content;
mod graphql;
mod limits;
mod refs;
mod search;
mod transport;
