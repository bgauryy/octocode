//! The bounded GitHub HTTP transport: request and response types, the
//! transport handle with its limiter state, and the client it sends through.
use bytes::Bytes;
use reqwest::{Client, header::HeaderMap};
use serde::Deserialize;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use url::Url;

use octocode_io::{budget::RequestBudget, retry::Backoff};

use response::{sso_directive, sso_parameter};

use super::{
    GitHubEndpoint, ProviderError, ProviderErrorKind, ResolvedCredential,
    budget::{GitHubBudget, GitHubResource, LimiterKey, now_ms},
};

#[derive(Clone, Copy, Debug)]
pub enum HttpMethod {
    Get,
    Post,
}
/// Octokit plugin-retry parity: `max_attempts` = retries + 1; 5xx/network
/// backoff is full-jitter from `base_delay`; a 5xx `retry-after` longer than
/// `max_retry_after` fails fast. A rate-limit wait (primary or secondary
/// limit, search spacing, the code-search window) longer than
/// `max_rate_limit_wait` fails fast with `retryAfter` instead; a shorter one
/// is credited to the request budget, so it never spends the request time.
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub base_delay: Duration,
    pub max_retry_after: Duration,
    /// The runtime sets its constant rate-limit wait; the transport's own
    /// default is the retry-after cap.
    pub max_rate_limit_wait: Duration,
}
impl RetryPolicy {
    /// The 5xx/network backoff: full jitter from `base_delay`, no floor.
    fn backoff(&self) -> Backoff {
        Backoff {
            base: self.base_delay,
            cap: MAX_BACKOFF,
            floor: Duration::ZERO,
        }
    }
}
impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            base_delay: Duration::from_millis(250),
            max_retry_after: Duration::from_secs(10),
            max_rate_limit_wait: Duration::from_secs(10),
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
    pub(crate) fn graphql(url: Url, body: Value) -> Self {
        Self {
            method: HttpMethod::Post,
            url,
            body: Some(body),
            headers: HeaderMap::new(),
        }
    }
}

/// One request's budget and the credential the host resolved for it. Every
/// GitHub call made with this context carries that credential (or none).
#[derive(Clone)]
pub struct RequestContext {
    pub budget: RequestBudget,
    credential: Option<ResolvedCredential>,
}
impl RequestContext {
    pub fn new(budget: RequestBudget, credential: Option<ResolvedCredential>) -> Self {
        Self { budget, credential }
    }
    /// An anonymous request with a fresh `timeout` budget.
    pub fn with_timeout(timeout: Duration, max_body_bytes: usize) -> Self {
        Self::new(RequestBudget::with_timeout(timeout, max_body_bytes), None)
    }
    pub fn resolved_credential(&self) -> Option<&ResolvedCredential> {
        self.credential.as_ref()
    }
    /// The budget check as a provider error: cancellation first, then the deadline.
    pub fn check(&self) -> Result<(), ProviderError> {
        Ok(self.budget.check()?)
    }
}

#[derive(Clone, Debug)]
pub struct ResponsePage {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub next: Option<Url>,
}

impl ResponsePage {
    /// Organization ids whose results GitHub withheld (`x-github-sso:
    /// partial-results; organizations=1,2`): the token is not authorized
    /// for them, so the page is partial.
    pub(crate) fn sso_partial_organizations(&self) -> Vec<u64> {
        sso_directive(&self.headers, "partial-results")
            .and_then(|parameters| sso_parameter(parameters, "organizations"))
            .map(|ids| {
                ids.split(',')
                    .filter_map(|id| id.trim().parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// The warning for a search GitHub only partly answered because SAML
/// enforcement withheld some organizations (`None` when nothing was withheld).
pub fn sso_partial_warning(organizations: &[u64]) -> Option<String> {
    if organizations.is_empty() {
        return None;
    }
    let ids = organizations
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "Results from SAML-protected organizations (ids {ids}) were withheld: authorize the token for them (token settings, Configure SSO) and search again."
    ))
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
    pub(crate) fn is_rate_limited(&self) -> bool {
        self.error_type.as_deref() == Some("RATE_LIMITED")
            || self.extensions.get("type").and_then(Value::as_str) == Some("RATE_LIMITED")
            || self.message.contains("RATE_LIMITED")
    }

    /// Octokit plugin-retry retries this GraphQL failure like a 500.
    pub(crate) fn is_transient(&self) -> bool {
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

/// The HTTP client a transport for `endpoint` uses: a loopback API is
/// reached directly, any other through the user's system proxy.
fn client_policy(endpoint: &GitHubEndpoint) -> octocode_io::http::ClientPolicy {
    octocode_io::http::ClientPolicy {
        proxy: if endpoint.is_loopback() {
            octocode_io::http::Proxy::Direct
        } else {
            octocode_io::http::Proxy::System
        },
        ..Default::default()
    }
}

#[derive(Clone)]
pub struct GitHubTransport {
    client: Client,
    endpoint: GitHubEndpoint,
    retry: RetryPolicy,
    budget: Arc<GitHubBudget>,
    state_dir: Option<PathBuf>,
    pub graphql_enabled: bool,
    /// Response cache for reads made on the transport (search pages and
    /// history reads): a repeated search, even from a new CLI process,
    /// sends no request, so it spends no search budget and waits for no
    /// search spacing. Clones share it.
    pub cache: Arc<dyn super::ConditionalCache>,
    /// `owner/repo` (lowercase) whose requests GitHub redirected to
    /// `/repositories/<id>`: a renamed repository. Clones share it.
    renamed: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    /// Repository metadata reads, shared by the rows of a batch.
    pub(crate) repositories: Arc<super::search::RepositoryMemo>,
    /// Where each owner's repositories are listed from, per credential.
    pub(crate) owners: Arc<super::search::OwnerSources>,
}
impl GitHubTransport {
    /// A transport admitting its requests through `budget`: production
    /// passes [`GitHubBudget::global`] (or [`GitHubBudget::paced`]).
    pub fn new(
        endpoint: GitHubEndpoint,
        retry: RetryPolicy,
        budget: Arc<GitHubBudget>,
    ) -> Result<Self, ProviderError> {
        let client = octocode_io::http::client(&client_policy(&endpoint)).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Configuration,
                "failed to initialize GitHub HTTP client",
            )
        })?;
        Ok(Self {
            client,
            endpoint,
            retry,
            budget,
            state_dir: None,
            graphql_enabled: true,
            cache: Arc::new(super::NoCache),
            renamed: Arc::default(),
            repositories: Arc::new(super::KeyedFlight::retained(
                super::search::REPOSITORY_MEMO_KEYS,
            )),
            owners: Arc::default(),
        })
    }

    /// GitHub redirected a request under `/repos/{owner}/{repo}` to the
    /// repository's id: the name is stale (renamed or transferred).
    pub fn followed_rename(&self, owner: &str, repo: &str) -> bool {
        let key = format!("{owner}/{repo}").to_ascii_lowercase();
        self.renamed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&key)
    }

    /// Remember the repository a followed `/repos/{owner}/{repo}/…` →
    /// `/repositories/<id>/…` redirect names.
    fn record_rename(&self, from: &Url, to: &Url) {
        if !to.path().contains("/repositories/") {
            return;
        }
        let Some(segments) = from.path_segments() else {
            return;
        };
        let segments = segments.collect::<Vec<_>>();
        let Some(at) = segments.iter().position(|segment| *segment == "repos") else {
            return;
        };
        if let (Some(owner), Some(repo)) = (segments.get(at + 1), segments.get(at + 2)) {
            self.renamed
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(format!("{owner}/{repo}").to_ascii_lowercase());
        }
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

    pub fn cache_partition(
        &self,
        context: &RequestContext,
        session: Option<&str>,
    ) -> Result<super::Partition, ProviderError> {
        let partition = super::Partition::new(
            self.endpoint.rest(&[])?.as_str(),
            context
                .resolved_credential()
                .map(ResolvedCredential::secret),
        );
        Ok(match session {
            Some(session) => partition.within(session),
            None => partition,
        })
    }

    /// Refuse a row's `searches` code searches before the first is sent when
    /// they cannot all start within the rate-limit cap and the request's
    /// deadline (the wait bound `execute` applies to each).
    pub fn code_search_batch_refusal(
        &self,
        searches: usize,
        context: &RequestContext,
    ) -> Option<ProviderError> {
        let cap = self.retry.max_rate_limit_wait;
        let deadline = context.budget.admission_deadline(cap);
        self.key_state(context.resolved_credential())
            .code_search_batch_refusal(searches, self.budget.config(), cap, deadline)
    }

    /// Code searches the request credential's window still has room for.
    pub fn code_search_room(&self, context: &RequestContext) -> usize {
        self.key_state(context.resolved_credential())
            .code_search_room(self.budget.config())
    }

    /// Limiter state for this transport's host and the request credential.
    fn key_state(&self, credential: Option<&ResolvedCredential>) -> Arc<super::budget::KeyState> {
        let key = LimiterKey::for_url(
            &self
                .endpoint
                .rest(&[])
                .unwrap_or_else(|_| self.endpoint.graphql()),
            credential.map(ResolvedCredential::expose_secret),
        );
        self.budget.key_state(&key, self.state_dir.as_deref())
    }

    /// Whether GraphQL can be attempted now for this credential: false while
    /// the key's graphql bucket (or a secondary cooldown) blocks longer than
    /// the retry cap. Replaces the old permanent per-host skip.
    pub fn graphql_available(&self, context: &RequestContext) -> bool {
        let state = self.key_state(context.resolved_credential());
        state.refresh_from_disk();
        state
            .blocked(GitHubResource::Graphql.bucket(), self.budget.config())
            .is_none_or(|block| {
                let wait = block.wait(now_ms());
                block.circuit.is_none()
                    && wait <= self.retry.max_rate_limit_wait
                    && Instant::now() + wait < context.budget.wait_until
            })
    }
}

/// The User-Agent every GitHub request (API and OAuth) sends.
pub const HTTP_USER_AGENT: &str = "octocode-native";

/// Upper bound for one 5xx/network backoff sleep.
const MAX_BACKOFF: Duration = Duration::from_secs(8);

mod attempts;
mod execute;
mod response;
#[cfg(test)]
mod response_error_tests;
