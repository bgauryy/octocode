use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use reqwest::{
    Client, StatusCode,
    header::{ACCEPT, AUTHORIZATION, HeaderMap, RETRY_AFTER, USER_AGENT},
};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;
use url::Url;

use super::{
    CredentialRequest, CredentialResolver, GitHubEndpoint, ProviderError, ProviderErrorKind,
    RateLimit,
    budget::{
        GitHubBudget, GitHubResource, graphql_is_skipped, is_primary_rate_limit,
        is_secondary_rate_limit, retry_after_or_backoff, skip_graphql_host,
    },
};

#[derive(Clone, Copy, Debug)]
pub enum HttpMethod {
    Get,
    Post,
}
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
            base_delay: Duration::from_millis(100),
            max_retry_after: Duration::from_secs(60),
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
    #[serde(default)]
    pub path: Vec<Value>,
    #[serde(default)]
    pub extensions: Value,
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
    pub graphql_enabled: bool,
}
impl<R> Clone for GitHubTransport<R> {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            endpoint: self.endpoint.clone(),
            credentials: self.credentials.clone(),
            retry: self.retry.clone(),
            budget: self.budget.clone(),
            graphql_enabled: self.graphql_enabled,
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
            graphql_enabled: true,
        })
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
            digest.update(credential.expose().as_bytes());
        }
        digest.update([0]);
        if let Some(session) = session {
            digest.update(session.as_bytes());
        }
        Ok(super::CachePartition(hex::encode(digest.finalize())))
    }

    pub async fn execute_graphql(
        &self,
        query: &str,
        variables: Value,
        context: &RequestContext,
    ) -> Result<GraphQlPage, ProviderError> {
        if graphql_is_skipped(self.endpoint.credential_host()) {
            return Err(ProviderError::new(
                ProviderErrorKind::RateLimited,
                "GitHub GraphQL skipped after primary rate limit",
            ));
        }
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
        if parsed.errors.iter().any(|error| {
            error.message.contains("RATE_LIMITED")
                || error.extensions.get("type").and_then(Value::as_str) == Some("RATE_LIMITED")
        }) {
            skip_graphql_host(self.endpoint.credential_host());
        }
        Ok(parsed)
    }
    pub async fn execute(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
    ) -> Result<ResponsePage, ProviderError> {
        if !self.endpoint.permits(&spec.url) {
            return Err(ProviderError::new(
                ProviderErrorKind::RedirectDenied,
                "request URL is outside the configured GitHub API origin",
            ));
        }
        let credential = self.credential(context).await?;
        let resource = GitHubResource::classify(&spec.url);
        for attempt in 0..self.retry.max_attempts {
            let _permit = self
                .budget
                .acquire(resource, context.deadline, &context.cancellation)
                .await?;
            if context.cancellation.is_cancelled() {
                return Err(ProviderError::new(
                    ProviderErrorKind::Cancelled,
                    "GitHub request cancelled",
                ));
            }
            let now = Instant::now();
            if now >= context.deadline {
                return Err(ProviderError::new(
                    ProviderErrorKind::Timeout,
                    "GitHub request deadline exceeded",
                ));
            }
            let mut request = match spec.method {
                HttpMethod::Get => self.client.get(spec.url.clone()),
                HttpMethod::Post => self.client.post(spec.url.clone()),
            }
            .header(USER_AGENT, "octocode-native")
            .header(ACCEPT, "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28")
            .headers(spec.headers.clone());
            if let Some(token) = &credential {
                request = request.header(AUTHORIZATION, format!("Bearer {}", token.expose()));
            }
            if let Some(body) = &spec.body {
                request = request.json(body);
            }
            let response = tokio::select! { _ = context.cancellation.cancelled() => return Err(ProviderError::new(ProviderErrorKind::Cancelled, "GitHub request cancelled")), value = tokio::time::timeout(context.deadline.saturating_duration_since(Instant::now()), request.send()) => value.map_err(|_| ProviderError::new(ProviderErrorKind::Timeout, "GitHub request deadline exceeded"))? };
            match response {
                Ok(response) => {
                    let status = response.status();
                    let headers = response.headers().clone();
                    if status.is_redirection() && status != StatusCode::NOT_MODIFIED {
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
                        });
                    }
                    if status.is_success() || status == StatusCode::NOT_MODIFIED {
                        let next = parse_next(&headers, &self.endpoint)?;
                        let body = read_bounded(response, context).await?;
                        self.budget.record_success();
                        return Ok(ResponsePage {
                            status: status.as_u16(),
                            headers,
                            body,
                            next,
                        });
                    }
                    let body = read_bounded(response, context).await.unwrap_or_default();
                    let remaining = header_u64(&headers, "x-ratelimit-remaining");
                    let retry_after = header_u64(&headers, RETRY_AFTER.as_str());
                    let text = String::from_utf8_lossy(&body);
                    let primary = is_primary_rate_limit(status.as_u16(), remaining);
                    let secondary =
                        is_secondary_rate_limit(status.as_u16(), remaining, retry_after, &text);
                    let mut error = response_error(status, &headers, body);
                    if primary || secondary {
                        error.kind = ProviderErrorKind::RateLimited;
                        error.retryable = true;
                        error.message = if secondary {
                            "GitHub secondary rate limit exceeded"
                        } else {
                            error.message.as_ref()
                        }
                        .into();
                    }
                    if primary && resource == GitHubResource::Graphql {
                        skip_graphql_host(self.endpoint.credential_host());
                    }
                    self.budget
                        .record_failure(secondary || status.is_server_error());
                    let retry_delay = if primary || secondary {
                        retry_after_or_backoff(retry_after, attempt, self.retry.max_retry_after)
                    } else {
                        retry_delay(status, &headers, attempt, &self.retry)
                    };
                    if let Some(delay) = retry_delay
                        && Instant::now() + delay < context.deadline
                        && attempt + 1 < self.retry.max_attempts
                    {
                        tokio::select! { _ = context.cancellation.cancelled() => return Err(ProviderError::new(ProviderErrorKind::Cancelled, "GitHub request cancelled")), _ = tokio::time::sleep(delay) => {} }
                        continue;
                    }
                    return Err(error);
                }
                Err(_) if attempt + 1 < self.retry.max_attempts => {
                    let delay = self.retry.base_delay.saturating_mul(1_u32 << attempt);
                    if Instant::now() + delay >= context.deadline {
                        return Err(ProviderError::new(
                            ProviderErrorKind::Timeout,
                            "GitHub request deadline exceeded",
                        ));
                    }
                    tokio::select! {
                        _ = context.cancellation.cancelled() => return Err(ProviderError::new(
                            ProviderErrorKind::Cancelled,
                            "GitHub request cancelled",
                        )),
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
                Err(_) => {
                    return Err(ProviderError::new(
                        ProviderErrorKind::Transport,
                        "GitHub transport failed",
                    ));
                }
            }
        }
        Err(ProviderError::new(
            ProviderErrorKind::Transport,
            "GitHub retry budget exhausted",
        ))
    }
}

async fn read_bounded(
    response: reqwest::Response,
    context: &RequestContext,
) -> Result<Bytes, ProviderError> {
    let mut stream = response.bytes_stream();
    let mut result = BytesMut::new();
    while let Some(chunk) = tokio::select! { _ = context.cancellation.cancelled() => return Err(ProviderError::new(ProviderErrorKind::Cancelled, "GitHub request cancelled")), chunk = stream.next() => chunk }
    {
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
    for part in value.split(',') {
        if part.contains("rel=\"next\"") {
            let raw = part
                .trim()
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .trim_start_matches('<')
                .trim_end_matches('>');
            let url = Url::parse(raw).map_err(|_| {
                ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub pagination link")
            })?;
            if !endpoint.permits(&url) {
                return Err(ProviderError::new(
                    ProviderErrorKind::RedirectDenied,
                    "GitHub pagination link changed origin",
                ));
            }
            return Ok(Some(url));
        }
    }
    Ok(None)
}
fn retry_delay(
    status: StatusCode,
    headers: &HeaderMap,
    attempt: u8,
    policy: &RetryPolicy,
) -> Option<Duration> {
    if attempt + 1 >= policy.max_attempts {
        return None;
    }
    if status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN
            && headers
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                == Some("0"))
    {
        let seconds = headers.get(RETRY_AFTER)?.to_str().ok()?.parse().ok()?;
        let delay = Duration::from_secs(seconds);
        return (delay < policy.max_retry_after).then_some(delay);
    }
    status
        .is_server_error()
        .then_some(policy.base_delay.saturating_mul(1_u32 << attempt))
}
#[derive(Deserialize, Default)]
struct ErrorBody {
    message: Option<String>,
    documentation_url: Option<String>,
}
fn response_error(status: StatusCode, headers: &HeaderMap, body: Bytes) -> ProviderError {
    let parsed: ErrorBody = serde_json::from_slice(&body).unwrap_or_default();
    let remaining = header_u64(headers, "x-ratelimit-remaining");
    let rate_limited = status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN && remaining == Some(0));
    let kind = if rate_limited {
        ProviderErrorKind::RateLimited
    } else {
        match status.as_u16() {
            401 => ProviderErrorKind::Authentication,
            403 => ProviderErrorKind::Permission,
            404 | 410 => ProviderErrorKind::NotFound,
            400 | 422 => ProviderErrorKind::Validation,
            500..=599 => ProviderErrorKind::Server,
            _ => ProviderErrorKind::Transport,
        }
    };
    ProviderError {
        kind,
        message: parsed
            .message
            .unwrap_or_else(|| format!("GitHub API returned HTTP {}", status.as_u16()))
            .into_boxed_str(),
        status: Some(status.as_u16()),
        request_id: headers
            .get("x-github-request-id")
            .and_then(|v| v.to_str().ok())
            .map(Into::into),
        documentation_url: parsed.documentation_url.map(String::into_boxed_str),
        rate_limit: rate_limited.then(|| RateLimit {
            remaining,
            reset_epoch_seconds: header_u64(headers, "x-ratelimit-reset"),
            retry_after_seconds: header_u64(headers, RETRY_AFTER.as_str()),
        }),
        retryable: rate_limited || status.is_server_error(),
    }
}
fn header_u64(headers: &HeaderMap, name: &str) -> Option<u64> {
    headers.get(name)?.to_str().ok()?.parse().ok()
}
