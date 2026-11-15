//! The bounded GitHub HTTP transport: request and response types, the
//! transport handle with its limiter state, and the client it sends through.
use bytes::Bytes;
use reqwest::{Client, header::HeaderMap};
use serde::Deserialize;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use url::Url;

use octocode_io::retry::Backoff;

pub(crate) use response::last_page;
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
/// Octokit retry parity: `max_attempts` = retries + 1, with full-jitter 5xx/network backoff.
/// Every wait, a 5xx `retry-after` or a rate limit, keeps the origin's full
/// delay and is refused only when it would outlast the request deadline.
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub base_delay: Duration,
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
        }
    }
}

#[derive(Clone, Debug)]
pub struct RequestSpec {
    pub method: HttpMethod,
    pub url: Url,
    pub body: Option<Value>,
    pub headers: HeaderMap,
    /// The repository file this request reads when its URL names none (a blob).
    /// Included in [`evidence::request_key`] and exposed by [`requested_file`].
    file: Option<String>,
    /// A read of one pull request by a route with no validator (GraphQL): the
    /// only GraphQL answer kept, since something later says how long it
    /// stands, the lifetime GitHub states for that pull request's REST read
    /// ([`GitHubTransport::reuse_window`]).
    reads_pull_request: bool,
    /// The caller stores the answer's body itself: no saved answer keeps it
    /// a second time ([`GitHubTransport::keep`]), and no continuation's
    /// evidence does either (`Journal::record_page`).
    kept_by_caller: bool,
}
impl RequestSpec {
    pub fn get(url: Url) -> Self {
        Self {
            method: HttpMethod::Get,
            url,
            body: None,
            headers: HeaderMap::new(),
            file: None,
            reads_pull_request: false,
            kept_by_caller: false,
        }
    }
    pub(crate) fn graphql(url: Url, body: Value) -> Self {
        Self {
            method: HttpMethod::Post,
            url,
            body: Some(body),
            headers: HeaderMap::new(),
            file: None,
            reads_pull_request: false,
            kept_by_caller: false,
        }
    }

    /// The caller stores this answer's body itself: it is not saved.
    pub(crate) fn kept_by_caller(mut self) -> Self {
        self.kept_by_caller = true;
        self
    }

    /// This query reads one pull request.
    pub(crate) fn reading_pull_request(mut self) -> Self {
        self.reads_pull_request = true;
        self
    }

    /// This request reads the repository file at `path`.
    pub(crate) fn reading_file(mut self, path: &str) -> Self {
        self.file = Some(path.to_owned());
        self
    }

    /// Whether this request asks the origin to confirm a saved answer.
    fn is_conditional(&self) -> bool {
        self.headers.contains_key(reqwest::header::IF_NONE_MATCH)
            || self
                .headers
                .contains_key(reqwest::header::IF_MODIFIED_SINCE)
    }

    /// Whether the answer to this request can never change.
    pub(crate) fn is_pinned(&self) -> bool {
        matches!(self.method, HttpMethod::Get) && names_commit(&self.url)
    }
}

/// Whether a REST `url` names one immutable git object or a comparison of two:
/// file contents at a commit (`contents/…?ref=<sha>`), a tree (`git/trees/<sha>`
/// or `<sha>:<path>`), a blob, a commit (`commits/<sha>`, not the mutable lists
/// under it such as `commits/<sha>/pulls`) or `compare/<sha>...<sha>`, under
/// `repos/{owner}/{repo}` or `repositories/{id}` (where a rename leads).
fn names_commit(url: &Url) -> bool {
    let Some(segments) = url.path_segments() else {
        return false;
    };
    let segments = segments.collect::<Vec<_>>();
    let Some(rest) = segments
        .iter()
        .enumerate()
        .find_map(|(at, segment)| match *segment {
            "repos" => segments.get(at + 3..),
            "repositories" => segments.get(at + 2..),
            _ => None,
        })
    else {
        return false;
    };
    let is_sha = |value: &str| crate::is_full_sha(value);
    match rest {
        ["contents", ..] => url
            .query_pairs()
            .any(|(key, value)| key == "ref" && is_sha(&value)),
        // `<sha>:<path>`, its colon written or percent-encoded.
        ["git", "trees", tree] => tree
            .split(':')
            .next()
            .and_then(|tree| tree.split("%3A").next())
            .is_some_and(is_sha),
        ["git", "blobs", object] | ["commits", object] => is_sha(object),
        ["compare", range] => range
            .split_once("...")
            .is_some_and(|(base, head)| is_sha(base) && is_sha(head)),
        _ => false,
    }
}

#[derive(Clone, Debug)]
pub struct ResponsePage {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub next: Option<Url>,
    /// GitHub reached this answer through a redirect (a renamed repository).
    pub redirected: bool,
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

    /// A secondary limit named on HTTP 200, as GitHub documents it may be
    /// (Octokit tests the same words on a GraphQL error's message).
    pub(crate) fn is_secondary_limit(&self) -> bool {
        super::budget::names_secondary_limit(&self.message)
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
pub(super) fn client_policy(endpoint: &GitHubEndpoint) -> octocode_io::http::ClientPolicy {
    octocode_io::http::ClientPolicy {
        proxy: endpoint.proxy(),
        ..Default::default()
    }
}

#[derive(Clone)]
pub struct GitHubTransport {
    /// Built on the first request ([`GitHubTransport::client`]): a
    /// transport that never sends never pays for the client (TLS roots, the
    /// system proxy lookup).
    client: Arc<OnceLock<Client>>,
    endpoint: GitHubEndpoint,
    retry: RetryPolicy,
    budget: Arc<GitHubBudget>,
    state_dir: Option<PathBuf>,
    answers: Option<Arc<dyn SavedAnswers>>,
    fills: Arc<octocode_io::flight::KeyedFlight<execute::FillKey, bool>>,
}
impl GitHubTransport {
    /// A transport admitting its requests through `budget`: production
    /// passes [`GitHubBudget::global`] (or [`GitHubBudget::paced`]).
    pub fn new(
        endpoint: GitHubEndpoint,
        retry: RetryPolicy,
        budget: Arc<GitHubBudget>,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            client: Arc::new(OnceLock::new()),
            endpoint,
            retry,
            budget,
            state_dir: None,
            answers: None,
            fills: Arc::new(octocode_io::flight::KeyedFlight::transient()),
        })
    }

    /// Keep answers in `answers`, so a repeat costs one validated request at
    /// most and an immutable or still-fresh answer costs none.
    pub fn set_saved_answers(&mut self, answers: Option<Arc<dyn SavedAnswers>>) {
        self.answers = answers;
    }

    /// Mirror blocking rate-limit facts under `dir` so separate processes
    /// (one CLI process per call) honor each other's limits. `None` keeps
    /// the state in memory only (non-persistent storage mode).
    /// Clone admission on this transport's budget and rate-limit state.
    #[must_use]
    pub fn git_limiter(&self) -> crate::GitLimiter {
        crate::GitLimiter::new(Arc::clone(&self.budget), self.state_dir.clone())
    }

    /// The HTTP client, built on first use; transports cloned from one share
    /// it. When two first requests race, the first client stored is kept.
    pub(crate) fn client(&self) -> Result<&Client, ProviderError> {
        if let Some(client) = self.client.get() {
            return Ok(client);
        }
        let built = octocode_io::http::client(&client_policy(&self.endpoint)).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::Configuration,
                "failed to initialize GitHub HTTP client",
            )
        })?;
        Ok(self.client.get_or_init(|| built))
    }

    pub fn set_rate_limit_state_dir(&mut self, dir: Option<PathBuf>) {
        self.state_dir = dir;
    }
    pub fn endpoint(&self) -> &GitHubEndpoint {
        &self.endpoint
    }

    /// Refuse a row's `searches` code searches before the first is sent when
    /// they cannot all start before the request's deadline (the wait bound
    /// `execute` applies to each).
    pub fn code_search_batch_refusal(
        &self,
        searches: usize,
        context: &RequestContext,
    ) -> Option<ProviderError> {
        let deadline = context.budget.deadline;
        self.key_state(context.resolved_credential())
            .code_search_batch_refusal(searches, self.budget.config(), deadline)
    }

    /// Code searches the credential's reported (or assumed) quota still covers.
    pub fn code_search_room(&self, context: &RequestContext) -> usize {
        self.key_state(context.resolved_credential())
            .code_search_room(self.budget.config())
    }

    /// Limiter state for this transport's host and the request credential.
    pub(crate) fn key_state(
        &self,
        credential: Option<&ResolvedCredential>,
    ) -> Arc<super::budget::KeyState> {
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
    /// its bucket or secondary cooldown blocks past the request deadline.
    pub fn graphql_available(&self, context: &RequestContext) -> bool {
        let state = self.key_state(context.resolved_credential());
        state.refresh_from_disk();
        state
            .blocked(GitHubResource::Graphql.bucket(), self.budget.config())
            .is_none_or(|block| {
                let wait = block.wait(now_ms());
                block.circuit.is_none()
                    && wait
                        < context
                            .budget
                            .deadline
                            .saturating_duration_since(Instant::now())
            })
    }
}

/// The User-Agent every GitHub request (API and OAuth) sends.
pub const HTTP_USER_AGENT: &str = "octocode-native";

/// Upper bound for one 5xx/network backoff sleep.
const MAX_BACKOFF: Duration = Duration::from_secs(8);

mod attempts;
mod context;
mod evidence;
mod execute;
mod graphql;
mod headers;
pub use context::RequestContext;
pub use evidence::{AnswerBody, ProviderAnswer, ProviderEvidence, requested_file};
pub use execute::SEARCH_REUSE_WINDOW;
#[cfg(test)]
pub(crate) use route::RENAME_ROUTE_WINDOW;
mod response;
#[cfg(test)]
mod response_error_tests;
mod route;
mod saved;
pub use saved::{Reuse, SavedAnswer, SavedAnswers, worth_keeping};
