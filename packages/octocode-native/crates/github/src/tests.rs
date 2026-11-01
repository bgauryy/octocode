//! octocode-github's tests, grouped by concern; the shared fixtures live here.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
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

#[derive(Default)]
struct MemoryCache(Mutex<Option<CachedContent>>);

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

impl ConditionalCache for MemoryCache {
    // Single-slot body cache: ref memo entries (`github-ref:`) are not stored
    // so each test controls ref resolution through its mocks.
    fn get<'a>(
        &'a self,
        _: &'a Partition,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<CachedContent>> + Send + 'a>>
    {
        Box::pin(async move {
            if key.starts_with("github-ref:") {
                return None;
            }
            self.0.lock().expect("cache lock").clone()
        })
    }
    fn put<'a>(
        &'a self,
        _: &'a Partition,
        key: String,
        value: CachedContent,
        _: Retention,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if !key.starts_with("github-ref:") {
                *self.0.lock().expect("cache lock") = Some(value);
            }
        })
    }
}

/// A keyed in-memory cache that honors partitions, for transport-level reads.
#[derive(Default)]
struct PartitionedCache(Mutex<std::collections::HashMap<(String, String), CachedContent>>);

impl ConditionalCache for PartitionedCache {
    fn get<'a>(
        &'a self,
        partition: &'a Partition,
        key: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<CachedContent>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut value = self
                .0
                .lock()
                .expect("cache lock")
                .get(&(partition.identity().to_owned(), key.to_owned()))
                .cloned()?;
            // Pinned commits are fresh; everything else revalidates.
            if key.starts_with("github-commit:") {
                value.etag = None;
            }
            Some(value)
        })
    }
    fn put<'a>(
        &'a self,
        partition: &'a Partition,
        key: String,
        value: CachedContent,
        _: Retention,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.0
                .lock()
                .expect("cache lock")
                .insert((partition.identity().to_owned(), key), value);
        })
    }
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
fn main_file(path: &str, force_refresh: bool) -> ContentRequest {
    ContentRequest {
        owner: "a".into(),
        repo: "b".into(),
        path: path.into(),
        reference: Some("main".into()),
        force_refresh,
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
            max_rate_limit_wait: Duration::from_secs(1),
        },
        GitHubBudget::relaxed(),
    )
    .expect("transport");
    GitHubProvider::new(transport, MemoryCache::default())
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
        max_rate_limit_wait: Duration::from_secs(5),
    }
}

mod content;
mod graphql;
mod limits;
mod refs;
mod search;
mod transport;
