use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use reqwest::{
    Client, StatusCode,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, LOCATION, RETRY_AFTER, USER_AGENT},
};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use url::Url;

use super::{
    CredentialRequest, CredentialResolver, GitHubEndpoint, ProviderError, ProviderErrorKind,
    RateLimit,
    budget::{
        GitHubBudget, GitHubResource, Group, LimiterKey, count_call, count_failure,
        count_rate_limit, full_jitter, is_primary_rate_limit, is_secondary_rate_limit, now_ms,
        pause, rate_limited_error,
    },
};

#[derive(Clone, Copy, Debug)]
pub enum HttpMethod {
    Get,
    Post,
}
/// Octokit plugin-retry parity: `max_attempts` = retries + 1; 5xx/network
/// backoff is full-jitter from `base_delay`; any rate-limit or `retry-after`
/// wait longer than `max_retry_after` fails fast with metadata instead.
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub base_delay: Duration,
    pub max_retry_after: Duration,
}
impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            base_delay: Duration::from_millis(250),
            max_retry_after: Duration::from_secs(10),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RequestSpec {
    pub method: HttpMethod,
    pub url: Url,
    pub body: Option<Value>,
    pub headers: HeaderMap,
}
impl RequestSpec {
    pub fn get(url: Url) -> Self {
        Self {
            method: HttpMethod::Get,
            url,
            body: None,
            headers: HeaderMap::new(),
        }
    }
    pub fn graphql(url: Url, body: Value) -> Self {
        Self {
            method: HttpMethod::Post,
            url,
            body: Some(body),
            headers: HeaderMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct RequestContext {
    pub deadline: Instant,
    pub cancellation: CancellationToken,
    pub max_body_bytes: usize,
    pub override_token: Option<SecretString>,
    credential: Arc<tokio::sync::OnceCell<Option<super::ResolvedCredential>>>,
}
impl RequestContext {
    pub fn with_timeout(timeout: Duration, max_body_bytes: usize) -> Self {
        Self {
            deadline: Instant::now() + timeout,
            cancellation: CancellationToken::new(),
            max_body_bytes,
            override_token: None,
            credential: Arc::new(tokio::sync::OnceCell::new()),
        }
    }
    pub fn resolved_credential(&self) -> Option<&super::ResolvedCredential> {
        self.credential.get().and_then(Option::as_ref)
    }

    pub fn with_resolved_credential(
        timeout: Duration,
        max_body_bytes: usize,
        resolved: Option<super::ResolvedCredential>,
    ) -> Self {
        let credential = tokio::sync::OnceCell::new();
        let _ = credential.set(resolved);
        Self {
            deadline: Instant::now() + timeout,
            cancellation: CancellationToken::new(),
            max_body_bytes,
            override_token: None,
            credential: Arc::new(credential),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResponsePage {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub next: Option<Url>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct GraphQlError {
    pub message: String,
    /// GitHub puts the error class at the top level (`"type": "RATE_LIMITED"`).
    #[serde(default, rename = "type")]
    pub error_type: Option<String>,
    #[serde(default)]
    pub path: Vec<Value>,
    #[serde(default)]
    pub extensions: Value,
}

impl GraphQlError {
    pub fn is_rate_limited(&self) -> bool {
        self.error_type.as_deref() == Some("RATE_LIMITED")
            || self.extensions.get("type").and_then(Value::as_str) == Some("RATE_LIMITED")
            || self.message.contains("RATE_LIMITED")
    }

    /// Octokit plugin-retry retries this GraphQL failure like a 500.
    pub fn is_transient(&self) -> bool {
        self.message
            .contains("Something went wrong while executing your query")
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct GraphQlPage {
    pub data: Option<Value>,
    #[serde(default)]
    pub errors: Vec<GraphQlError>,
}

pub struct GitHubTransport<R> {
    client: Client,
    endpoint: GitHubEndpoint,
    credentials: Arc<R>,
    retry: RetryPolicy,
    budget: Arc<GitHubBudget>,
    state_dir: Option<PathBuf>,
    pub graphql_enabled: bool,
    /// Code-search pages by request URL and accept mode for 60 s: a repeated
    /// search (re-page, retry, follow-up) spends none of the 10/min budget.
    /// One transport serves one credential resolver, so results never cross
    /// identities; clones share it.
    pub(crate) search_results: moka::sync::Cache<String, Arc<super::search::CodeSearchPage>>,
}
impl<R> Clone for GitHubTransport<R> {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            endpoint: self.endpoint.clone(),
            credentials: self.credentials.clone(),
            retry: self.retry.clone(),
            budget: self.budget.clone(),
            state_dir: self.state_dir.clone(),
            graphql_enabled: self.graphql_enabled,
            search_results: self.search_results.clone(),
        }
    }
}
impl<R: CredentialResolver> GitHubTransport<R> {
    async fn credential(
        &self,
        context: &RequestContext,
    ) -> Result<Option<super::ResolvedCredential>, ProviderError> {
        context
            .credential
            .get_or_try_init(|| {
                self.credentials.resolve(CredentialRequest {
                    host: self.endpoint.credential_host(),
                    override_token: context.override_token.as_ref().map(|v| v.expose_secret()),
                })
            })
            .await
            .cloned()
    }
    pub fn new(
        endpoint: GitHubEndpoint,
        credentials: Arc<R>,
        retry: RetryPolicy,
    ) -> Result<Self, ProviderError> {
        let budget = if cfg!(test) {
            GitHubBudget::relaxed()
        } else {
            GitHubBudget::global()
        };
        Self::with_budget(endpoint, credentials, retry, budget)
    }

    pub fn with_budget(
        endpoint: GitHubEndpoint,
        credentials: Arc<R>,
        retry: RetryPolicy,
        budget: Arc<GitHubBudget>,
    ) -> Result<Self, ProviderError> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| {
                ProviderError::new(
                    ProviderErrorKind::Configuration,
                    "failed to initialize GitHub HTTP client",
                )
            })?;
        Ok(Self {
            client,
            endpoint,
            credentials,
            retry,
            budget,
            state_dir: None,
            graphql_enabled: true,
            search_results: moka::sync::Cache::builder()
                .max_capacity(256)
                .time_to_live(std::time::Duration::from_secs(60))
                .build(),
        })
    }

    /// Mirror blocking rate-limit facts under `dir` so separate processes
    /// (one CLI process per call) honor each other's limits. `None` keeps
    /// the state in memory only (non-persistent storage mode).
    pub fn set_rate_limit_state_dir(&mut self, dir: Option<PathBuf>) {
        self.state_dir = dir;
    }
    pub fn endpoint(&self) -> &GitHubEndpoint {
        &self.endpoint
    }

    pub async fn cache_partition(
        &self,
        context: &RequestContext,
        session: Option<&str>,
    ) -> Result<super::CachePartition, ProviderError> {
        let credential = self.credential(context).await?;
        let mut digest = Sha256::new();
        digest.update(self.endpoint.rest(&[])?.as_str().as_bytes());
        digest.update([0]);
        if let Some(credential) = credential {
            digest.update(credential.expose_secret().as_bytes());
        }
        digest.update([0]);
        if let Some(session) = session {
            digest.update(session.as_bytes());
        }
        Ok(super::CachePartition(hex::encode(digest.finalize())))
    }

    /// Limiter state for this transport's host and the request credential.
    fn key_state(
        &self,
        credential: Option<&super::ResolvedCredential>,
    ) -> Arc<super::budget::KeyState> {
        let key = LimiterKey::for_url(
            &self
                .endpoint
                .rest(&[])
                .unwrap_or_else(|_| self.endpoint.graphql()),
            credential.map(super::ResolvedCredential::expose_secret),
        );
        self.budget.key_state(&key, self.state_dir.as_deref())
    }

    /// Whether GraphQL can be attempted now for this credential: false while
    /// the key's graphql bucket (or a secondary cooldown) blocks longer than
    /// the retry cap. Replaces the old permanent per-host skip.
    pub async fn graphql_available(&self, context: &RequestContext) -> bool {
        let Ok(credential) = self.credential(context).await else {
            return false;
        };
        let state = self.key_state(credential.as_ref());
        state.refresh_from_disk();
        state
            .blocked(GitHubResource::Graphql.bucket(), self.budget.config())
            .is_none_or(|block| {
                let wait = block.wait(now_ms());
                !block.circuit
                    && wait <= self.retry.max_retry_after
                    && Instant::now() + wait < context.deadline
            })
    }

    pub async fn execute_graphql(
        &self,
        query: &str,
        variables: Value,
        context: &RequestContext,
    ) -> Result<GraphQlPage, ProviderError> {
        let mut attempt: u8 = 0;
        loop {
            let page = self
                .execute(
                    RequestSpec::graphql(
                        self.endpoint.graphql(),
                        serde_json::json!({ "query": query, "variables": variables }),
                    ),
                    context,
                )
                .await?;
            let parsed: GraphQlPage = serde_json::from_slice(&page.body).map_err(|_| {
                ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub GraphQL response")
            })?;
            if parsed.errors.iter().any(GraphQlError::is_rate_limited) {
                // GraphQL primary limit arrives as HTTP 200 + errors[].type.
                count_rate_limit();
                let credential = self.credential(context).await?;
                let state = self.key_state(credential.as_ref());
                let reset = header_u64(&page.headers, "x-ratelimit-reset")
                    .unwrap_or_else(|| now_ms() / 1000 + 60);
                state.exhaust(GitHubResource::Graphql.bucket(), reset);
                let wait = Duration::from_millis(
                    reset
                        .saturating_mul(1000)
                        .saturating_add(self.budget.config().reset_grace.as_millis() as u64)
                        .saturating_sub(now_ms()),
                );
                if attempt + 1 < self.retry.max_attempts
                    && wait <= self.retry.max_retry_after
                    && Instant::now() + wait < context.deadline
                {
                    attempt += 1;
                    continue;
                }
                return Err(rate_limited_error(
                    "GitHub GraphQL rate limit exceeded",
                    RateLimit {
                        remaining: Some(0),
                        reset_epoch_seconds: Some(reset),
                        retry_after_seconds: Some(wait.as_millis().div_ceil(1000) as u64),
                        resource: Some(GitHubResource::Graphql.bucket().into()),
                    },
                ));
            }
            if parsed.errors.iter().any(GraphQlError::is_transient) {
                // Octokit plugin-retry treats this GraphQL failure as a 500.
                count_failure();
                if attempt + 1 < self.retry.max_attempts {
                    let delay = full_jitter(self.retry.base_delay, attempt, MAX_BACKOFF);
                    if pause(delay, context.deadline, &context.cancellation)
                        .await
                        .is_ok()
                    {
                        attempt += 1;
                        continue;
                    }
                    if context.cancellation.is_cancelled() {
                        return Err(ProviderError::new(
                            ProviderErrorKind::Cancelled,
                            "GitHub request cancelled",
                        ));
                    }
                }
            }
            return Ok(parsed);
        }
    }

    pub async fn execute(
        &self,
        mut spec: RequestSpec,
        context: &RequestContext,
    ) -> Result<ResponsePage, ProviderError> {
        if !self.endpoint.permits(&spec.url) {
            return Err(ProviderError::new(
                ProviderErrorKind::RedirectDenied,
                "request URL is outside the configured GitHub API origin",
            ));
        }
        let credential = self.credential(context).await?;
        let state = self.key_state(credential.as_ref());
        let config = self.budget.config();
        let cap = self.retry.max_retry_after;
        let resource = GitHubResource::classify(&spec.url);
        let group = match (resource, spec.method) {
            (GitHubResource::Graphql, _) => Some(Group::Graphql),
            (GitHubResource::Search | GitHubResource::CodeSearch, _) => Some(Group::Search),
            (GitHubResource::Core, HttpMethod::Post) => Some(Group::Write),
            (GitHubResource::Core, HttpMethod::Get) => None,
        };
        let mut redirects: u8 = 0;
        let mut attempt: u8 = 0;
        let mut counted_window = false;
        loop {
            if context.cancellation.is_cancelled() {
                return Err(ProviderError::new(
                    ProviderErrorKind::Cancelled,
                    "GitHub request cancelled",
                ));
            }
            if Instant::now() >= context.deadline {
                return Err(ProviderError::new(
                    ProviderErrorKind::Timeout,
                    "GitHub request deadline exceeded",
                ));
            }
            state
                .wait_unblocked(
                    resource.bucket(),
                    config,
                    cap,
                    context.deadline,
                    &context.cancellation,
                )
                .await?;
            // The code-search window is charged once per logical request.
            let charge_window = resource == GitHubResource::CodeSearch && !counted_window;
            let admission = state
                .admit(
                    group,
                    config,
                    charge_window,
                    cap,
                    context.deadline,
                    &context.cancellation,
                )
                .await?;
            counted_window |= charge_window;
            let mut request = match spec.method {
                HttpMethod::Get => self.client.get(spec.url.clone()),
                HttpMethod::Post => self.client.post(spec.url.clone()),
            }
            .header(USER_AGENT, "octocode-native")
            .header(ACCEPT, "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28")
            .headers(spec.headers.clone());
            if let Some(token) = &credential {
                request =
                    request.header(AUTHORIZATION, format!("Bearer {}", token.expose_secret()));
            }
            if let Some(body) = &spec.body {
                request = request.json(body);
            }
            count_call();
            let response = tokio::select! { _ = context.cancellation.cancelled() => return Err(ProviderError::new(ProviderErrorKind::Cancelled, "GitHub request cancelled")), value = tokio::time::timeout(context.deadline.saturating_duration_since(Instant::now()), request.send()) => value.map_err(|_| ProviderError::new(ProviderErrorKind::Timeout, "GitHub request deadline exceeded"))? };
            let response = match response {
                Ok(response) => response,
                Err(_) => {
                    // Release permits before backing off.
                    drop(admission);
                    count_failure();
                    state.record_circuit_failure(config);
                    if attempt + 1 < self.retry.max_attempts {
                        let delay = full_jitter(self.retry.base_delay, attempt, MAX_BACKOFF);
                        pause(delay, context.deadline, &context.cancellation).await?;
                        attempt += 1;
                        continue;
                    }
                    return Err(ProviderError::new(
                        ProviderErrorKind::Transport,
                        "GitHub transport failed",
                    ));
                }
            };
            let status = response.status();
            let headers = response.headers().clone();
            state.observe(&headers, resource.bucket());
            if status.is_redirection() && status != StatusCode::NOT_MODIFIED {
                drop(admission);
                // GitHub answers 301 for renamed repositories; follow
                // bounded same-origin GET redirects so renamed repos
                // stay reachable. `permits` gates the new location, so
                // the Authorization header never leaves the configured
                // API origin.
                let location = headers
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| spec.url.join(value).ok());
                if let Some(location) = location
                    && matches!(spec.method, HttpMethod::Get)
                    && redirects < 3
                    && self.endpoint.permits(&location)
                {
                    redirects += 1;
                    spec.url = location;
                    continue;
                }
                return Err(ProviderError {
                    kind: ProviderErrorKind::RedirectDenied,
                    message: "GitHub API redirect was not followed".into(),
                    status: Some(status.as_u16()),
                    request_id: headers
                        .get("x-github-request-id")
                        .and_then(|value| value.to_str().ok())
                        .map(Into::into),
                    documentation_url: None,
                    rate_limit: None,
                    retryable: false,
                    reason: None,
                });
            }
            if status.is_success() || status == StatusCode::NOT_MODIFIED {
                let next = parse_next(&headers, &self.endpoint)?;
                let body = read_bounded(response, context).await?;
                drop(admission);
                state.record_success();
                return Ok(ResponsePage {
                    status: status.as_u16(),
                    headers,
                    body,
                    next,
                });
            }
            // A cancel while reading the error body is a cancel, not an empty
            // body classified by status. An oversized body keeps the status
            // classification but says the body was not read.
            let (body, body_note) = match read_bounded(response, context).await {
                Ok(body) => (body, None),
                Err(error) if error.kind == ProviderErrorKind::Cancelled => {
                    drop(admission);
                    return Err(error);
                }
                Err(error) if error.kind == ProviderErrorKind::ResponseTooLarge => {
                    (Bytes::new(), Some("error body exceeded limit"))
                }
                Err(_) => (Bytes::new(), Some("error body could not be read")),
            };
            drop(admission);
            let failure = classify_failure(status, &headers, &body, config);
            let mut error = response_error(status, &headers, body);
            if let Some(note) = body_note {
                error.message = format!("{} ({note})", error.message).into_boxed_str();
            }
            let retry_wait = match failure {
                Failure::Primary { reset, wait } => {
                    count_rate_limit();
                    let bucket = headers
                        .get("x-ratelimit-resource")
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or(resource.bucket())
                        .to_owned();
                    state.exhaust(&bucket, reset);
                    error.kind = ProviderErrorKind::RateLimited;
                    error.retryable = true;
                    error.rate_limit = Some(RateLimit {
                        remaining: Some(0),
                        reset_epoch_seconds: Some(reset),
                        retry_after_seconds: Some(
                            header_u64(&headers, RETRY_AFTER.as_str())
                                .unwrap_or_else(|| ceil_secs(wait)),
                        ),
                        resource: Some(bucket.into()),
                    });
                    // The next loop turn waits in `wait_unblocked`.
                    Some((wait, Duration::ZERO))
                }
                Failure::Secondary { wait } => {
                    count_rate_limit();
                    state.record_circuit_failure(config);
                    state.cool_down(now_ms().saturating_add(wait.as_millis() as u64));
                    error.kind = ProviderErrorKind::RateLimited;
                    error.retryable = true;
                    error.message = "GitHub secondary rate limit exceeded".into();
                    error.rate_limit = Some(RateLimit {
                        remaining: header_u64(&headers, "x-ratelimit-remaining"),
                        reset_epoch_seconds: header_u64(&headers, "x-ratelimit-reset"),
                        retry_after_seconds: Some(ceil_secs(wait)),
                        resource: Some(
                            headers
                                .get("x-ratelimit-resource")
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or(resource.bucket())
                                .into(),
                        ),
                    });
                    Some((wait, Duration::ZERO))
                }
                Failure::Server => {
                    count_failure();
                    state.record_circuit_failure(config);
                    let delay = header_u64(&headers, RETRY_AFTER.as_str())
                        .map(Duration::from_secs)
                        .unwrap_or_else(|| {
                            full_jitter(self.retry.base_delay, attempt, MAX_BACKOFF)
                        });
                    Some((delay, delay))
                }
                Failure::Final => {
                    count_failure();
                    None
                }
            };
            let Some((wait, sleep)) = retry_wait else {
                return Err(error);
            };
            if attempt + 1 >= self.retry.max_attempts
                || wait > cap
                || Instant::now() + wait >= context.deadline
            {
                return Err(error);
            }
            pause(sleep, context.deadline, &context.cancellation).await?;
            attempt += 1;
        }
    }
}

/// Upper bound for one 5xx/network backoff sleep.
const MAX_BACKOFF: Duration = Duration::from_secs(8);

enum Failure {
    /// Primary limit: bucket exhausted until `reset` (epoch seconds).
    Primary { reset: u64, wait: Duration },
    /// Secondary limit: key-wide cooldown for `wait`.
    Secondary { wait: Duration },
    /// 5xx: retried with jittered backoff or `retry-after`.
    Server,
    /// Octokit doNotRetry (400/401/403/404/410/422/451) and everything else.
    Final,
}

fn classify_failure(
    status: StatusCode,
    headers: &HeaderMap,
    body: &[u8],
    config: &super::budget::ExecutorConfig,
) -> Failure {
    let code = status.as_u16();
    let remaining = header_u64(headers, "x-ratelimit-remaining");
    let retry_after = header_u64(headers, RETRY_AFTER.as_str());
    let reset = header_u64(headers, "x-ratelimit-reset");
    let text = String::from_utf8_lossy(body);
    let reset_wait = |reset: u64| {
        Duration::from_millis(
            reset
                .saturating_mul(1000)
                .saturating_add(config.reset_grace.as_millis() as u64)
                .saturating_sub(now_ms()),
        )
    };
    if is_secondary_rate_limit(code, remaining, retry_after, &text) {
        let wait = match (retry_after, remaining, reset) {
            (Some(seconds), _, _) => Duration::from_secs(seconds),
            (None, Some(0), Some(reset)) => reset_wait(reset),
            _ => config.secondary_default,
        };
        return Failure::Secondary { wait };
    }
    if is_primary_rate_limit(code, remaining) {
        let reset = reset.unwrap_or_else(|| {
            now_ms() / 1000 + retry_after.unwrap_or(config.secondary_default.as_secs())
        });
        return Failure::Primary {
            reset,
            wait: reset_wait(reset),
        };
    }
    if status.is_server_error() {
        Failure::Server
    } else {
        Failure::Final
    }
}

fn ceil_secs(duration: Duration) -> u64 {
    duration.as_millis().div_ceil(1000) as u64
}

async fn read_bounded(
    response: reqwest::Response,
    context: &RequestContext,
) -> Result<Bytes, ProviderError> {
    let mut stream = response.bytes_stream();
    let mut result = BytesMut::new();
    // The send phase is bounded by the request deadline; a body that stalls
    // after the headers must be too, not only by the outer runtime timeout.
    let deadline = tokio::time::Instant::from_std(context.deadline);
    while let Some(chunk) = tokio::select! {
        _ = context.cancellation.cancelled() => return Err(ProviderError::new(ProviderErrorKind::Cancelled, "GitHub request cancelled")),
        _ = tokio::time::sleep_until(deadline) => return Err(ProviderError::new(ProviderErrorKind::Timeout, "GitHub response body deadline exceeded")),
        chunk = stream.next() => chunk,
    } {
        let chunk = chunk.map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Transport,
                "failed reading GitHub response",
            )
        })?;
        if result.len().saturating_add(chunk.len()) > context.max_body_bytes {
            return Err(ProviderError::new(
                ProviderErrorKind::ResponseTooLarge,
                "GitHub response exceeded configured byte limit",
            ));
        }
        result.extend_from_slice(&chunk);
    }
    Ok(result.freeze())
}

fn parse_next(
    headers: &HeaderMap,
    endpoint: &GitHubEndpoint,
) -> Result<Option<Url>, ProviderError> {
    let Some(value) = headers.get("link").and_then(|v| v.to_str().ok()) else {
        return Ok(None);
    };
    let link = |relation: &str| {
        value
            .split(',')
            .find(|part| part.contains(relation))
            .map(|part| {
                part.trim()
                    .split(';')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
            })
    };
    let Some(raw) = link("rel=\"next\"") else {
        return Ok(None);
    };
    let next = Url::parse(raw).map_err(|_| {
        ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub pagination link")
    })?;
    if !endpoint.permits(&next) {
        return Err(ProviderError::new(
            ProviderErrorKind::RedirectDenied,
            "GitHub pagination link changed origin",
        ));
    }
    let page = |url: &Url| {
        url.query_pairs()
            .find(|(key, _)| key == "page")
            .and_then(|(_, value)| value.parse::<u64>().ok())
    };
    // GitHub may advertise next=N+1 alongside last=N on a full final page.
    // Only comparable numbered links establish that next is past the end.
    if let Some(last) = link("rel=\"last\"").and_then(|raw| Url::parse(raw).ok())
        && last.origin() == next.origin()
        && last.path() == next.path()
        && last
            .query_pairs()
            .filter(|(key, _)| key != "page")
            .eq(next.query_pairs().filter(|(key, _)| key != "page"))
        && page(&next)
            .zip(page(&last))
            .is_some_and(|(next, last)| next > last)
    {
        return Ok(None);
    }
    Ok(Some(next))
}
#[derive(Deserialize, Default)]
struct ErrorBody {
    message: Option<String>,
    documentation_url: Option<String>,
    /// GitHub validation failures carry the specific cause here
    /// (`"abc" is not a numeric value`) under a generic `Validation Failed`.
    #[serde(default)]
    errors: Vec<ErrorDetail>,
}
#[derive(Deserialize, Default)]
struct ErrorDetail {
    message: Option<String>,
}
fn response_error(status: StatusCode, headers: &HeaderMap, body: Bytes) -> ProviderError {
    let parsed: ErrorBody = serde_json::from_slice(&body).unwrap_or_default();
    let kind = match status.as_u16() {
        401 => ProviderErrorKind::Authentication,
        403 => ProviderErrorKind::Permission,
        404 | 410 => ProviderErrorKind::NotFound,
        400 | 422 => ProviderErrorKind::Validation,
        429 => ProviderErrorKind::RateLimited,
        451 => ProviderErrorKind::Unavailable,
        500..=599 => ProviderErrorKind::Server,
        _ => ProviderErrorKind::HttpStatus,
    };
    let code = status.as_u16();
    let validation_details: Vec<String> = parsed
        .errors
        .into_iter()
        .filter_map(|error| error.message)
        .map(|message| message.trim().to_owned())
        .filter(|message| !message.is_empty())
        .take(3)
        .collect();
    let message = match (kind, parsed.message) {
        (ProviderErrorKind::Unavailable, Some(detail)) => {
            format!("GitHub resource blocked for legal reasons (HTTP 451): {detail}")
        }
        (ProviderErrorKind::Unavailable, None) => {
            "GitHub resource blocked for legal reasons (HTTP 451)".to_owned()
        }
        (ProviderErrorKind::HttpStatus, Some(detail)) => {
            format!("GitHub API returned HTTP {code}: {detail}")
        }
        (ProviderErrorKind::Validation, detail) if !validation_details.is_empty() => match detail {
            Some(detail) => format!("{detail}: {}", validation_details.join("; ")),
            None => validation_details.join("; "),
        },
        (_, Some(detail)) => detail,
        (_, None) => format!("GitHub API returned HTTP {code}"),
    };
    ProviderError {
        kind,
        message: message.into_boxed_str(),
        status: Some(status.as_u16()),
        request_id: headers
            .get("x-github-request-id")
            .and_then(|v| v.to_str().ok())
            .map(Into::into),
        documentation_url: parsed.documentation_url.map(String::into_boxed_str),
        rate_limit: None,
        retryable: status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS,
        reason: None,
    }
}
fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.parse().ok()
}

#[cfg(test)]
mod response_error_tests {
    use super::*;

    #[test]
    fn validation_errors_keep_github_detail() {
        let body = Bytes::from_static(
            br#"{"message":"Validation Failed","errors":[{"message":"\"abc\" is not a numeric value"}]}"#,
        );
        let error = response_error(StatusCode::UNPROCESSABLE_ENTITY, &HeaderMap::new(), body);
        assert_eq!(error.kind, ProviderErrorKind::Validation);
        assert_eq!(
            &*error.message,
            "Validation Failed: \"abc\" is not a numeric value"
        );
        let bare = response_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            &HeaderMap::new(),
            Bytes::from_static(br#"{"message":"Validation Failed"}"#),
        );
        assert_eq!(&*bare.message, "Validation Failed");
    }

    #[tokio::test]
    async fn a_body_that_stalls_after_the_headers_hits_the_request_deadline() {
        use tokio::io::AsyncWriteExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                // Promise 1000 bytes, send 5, then go silent.
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 1000\r\n\r\nhello")
                    .await;
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });
        let response = reqwest::get(format!("http://{addr}/"))
            .await
            .expect("headers arrive");
        let context = RequestContext::with_timeout(Duration::from_millis(300), 1 << 20);
        let started = Instant::now();
        let error = read_bounded(response, &context)
            .await
            .expect_err("stalled body times out");
        assert_eq!(error.kind, ProviderErrorKind::Timeout);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "bounded by the deadline"
        );
    }
}
