use bytes::Bytes;
use reqwest::{
    Client, StatusCode,
    header::{ACCEPT, HeaderMap, LOCATION, USER_AGENT},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use url::Url;

use octocode_io::{
    budget::{BudgetStop, RequestBudget},
    retry::{Backoff, BodyError, classify_status, header_u64, retry_after_delay},
};

use super::{
    GitHubEndpoint, ProviderError, ProviderErrorKind, ProviderErrorReason, RateLimit,
    ResolvedCredential,
    budget::{
        Admission, CircuitCause, ExecutorConfig, GitHubBudget, GitHubResource, Group, KeyState,
        LimiterKey, is_primary_rate_limit, is_secondary_rate_limit, now_ms, pause,
        rate_limited_error,
    },
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

    pub async fn execute_graphql(
        &self,
        query: &str,
        variables: Value,
        context: &RequestContext,
    ) -> Result<GraphQlPage, ProviderError> {
        // One physical-attempt budget for the whole operation: HTTP retries
        // inside `execute_counted` and the semantic retries below share it.
        let mut attempt: u8 = 0;
        loop {
            let page = self
                .execute_counted(
                    RequestSpec::graphql(
                        self.endpoint.graphql(),
                        serde_json::json!({ "query": query, "variables": variables }),
                    ),
                    context,
                    &mut attempt,
                )
                .await?;
            let parsed: GraphQlPage = serde_json::from_slice(&page.body).map_err(|_| {
                ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub GraphQL response")
            })?;
            if parsed.errors.iter().any(GraphQlError::is_rate_limited) {
                // GraphQL primary limit arrives as HTTP 200 + errors[].type.
                let state = self.key_state(context.resolved_credential());
                let reset = header_u64(&page.headers, "x-ratelimit-reset")
                    .unwrap_or_else(|| now_ms() / 1000 + 60);
                state.exhaust(GitHubResource::Graphql.bucket(), reset);
                let wait = self.budget.config().reset_wait(reset);
                if attempt + 1 < self.retry.max_attempts
                    && wait <= self.retry.max_rate_limit_wait
                    && Instant::now() + wait < context.budget.wait_until
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
                if attempt + 1 < self.retry.max_attempts {
                    let delay = self.retry.backoff().next_delay(u32::from(attempt));
                    if pause(
                        delay,
                        context.budget.effective_deadline(),
                        &context.budget.cancellation,
                    )
                    .await
                    .is_ok()
                    {
                        attempt += 1;
                        continue;
                    }
                    if context.budget.cancellation.is_cancelled() {
                        return Err(BudgetStop::Cancelled.into());
                    }
                }
            }
            return Ok(parsed);
        }
    }

    /// One logical request: its attempts count one failure toward the
    /// circuit breaker, however many retries it took.
    pub async fn execute(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
    ) -> Result<ResponsePage, ProviderError> {
        self.execute_counted(spec, context, &mut 0).await
    }

    /// [`Self::execute`] drawing on `attempt`, the physical-attempt count of
    /// an enclosing logical operation: its sends and an outer semantic retry
    /// share one `max_attempts` budget.
    async fn execute_counted(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
        attempt: &mut u8,
    ) -> Result<ResponsePage, ProviderError> {
        let mut failed = None;
        let result = self.run_attempts(spec, context, &mut failed, attempt).await;
        if let (Err(error), Some(cause)) = (&result, failed)
            && error.kind != ProviderErrorKind::Cancelled
        {
            let state = self.key_state(context.resolved_credential());
            state.record_circuit_failure(self.budget.config(), cause);
        }
        result
    }

    /// The attempts of one request; `failed` is the circuit-breaker cause of
    /// the last attempt, cleared whenever GitHub answers without one.
    async fn run_attempts(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
        failed: &mut Option<CircuitCause>,
        attempt: &mut u8,
    ) -> Result<ResponsePage, ProviderError> {
        if !self.endpoint.permits(&spec.url) {
            return Err(ProviderError::new(
                ProviderErrorKind::RedirectDenied,
                "request URL is outside the configured GitHub API origin",
            ));
        }
        Attempts::new(self, spec, context, failed, attempt)
            .run()
            .await
    }
}

/// How a failed attempt may be retried: `wait` is held to the retry limits,
/// `sleep` is spent before the next attempt.
struct RetryWait {
    wait: Duration,
    sleep: Duration,
    /// A rate limit, waited out by the next attempt's admission.
    rate_limited: bool,
}

/// The state one request's attempts share.
struct Attempts<'a> {
    transport: &'a GitHubTransport,
    context: &'a RequestContext,
    failed: &'a mut Option<CircuitCause>,
    state: Arc<KeyState>,
    config: &'a ExecutorConfig,
    resource: GitHubResource,
    group: Option<Group>,
    spec: RequestSpec,
    redirects: u8,
    /// The operation's physical attempts so far (0-based index of the
    /// next send's predecessor), shared with any outer semantic retry.
    attempt: &'a mut u8,
    counted_window: bool,
}

impl<'a> Attempts<'a> {
    fn new(
        transport: &'a GitHubTransport,
        spec: RequestSpec,
        context: &'a RequestContext,
        failed: &'a mut Option<CircuitCause>,
        attempt: &'a mut u8,
    ) -> Self {
        let state = transport.key_state(context.resolved_credential());
        let config = transport.budget.config();
        let resource = transport.endpoint.resource(&spec.url);
        let group = match (resource, spec.method) {
            (GitHubResource::Graphql, _) => Some(Group::Graphql),
            (GitHubResource::Search, _) => Some(Group::Search),
            (GitHubResource::CodeSearch, _) => Some(Group::CodeSearch),
            (GitHubResource::Core, HttpMethod::Post) => Some(Group::Write),
            (GitHubResource::Core, HttpMethod::Get) => None,
        };
        Self {
            transport,
            context,
            failed,
            state,
            config,
            resource,
            group,
            spec,
            redirects: 0,
            attempt,
            counted_window: false,
        }
    }

    /// Admit, send and classify attempts until one answers, fails for good,
    /// or the retry policy gives up.
    async fn run(mut self) -> Result<ResponsePage, ProviderError> {
        loop {
            let admission = self.admit().await?;
            let Some(response) = self.send().await? else {
                // Release permits before backing off.
                drop(admission);
                self.back_off_transport().await?;
                continue;
            };
            let status = response.status();
            let headers = response.headers().clone();
            *self.failed = None;
            self.state.observe(&headers, self.resource.bucket());
            if status.is_redirection() && status != StatusCode::NOT_MODIFIED {
                drop(admission);
                self.follow_redirect(status, &headers)?;
                continue;
            }
            if status.is_success() || status == StatusCode::NOT_MODIFIED {
                let next = parse_next(&headers, &self.transport.endpoint)?;
                let body = read_bounded(response, self.context).await?;
                drop(admission);
                self.state.record_success();
                return Ok(ResponsePage {
                    status: status.as_u16(),
                    headers,
                    body,
                    next,
                });
            }
            let (body, body_note) = read_error_body(response, self.context).await?;
            let (error, backoff) = self.classify(status, &headers, body, body_note);
            drop(admission);
            let Some(backoff) = backoff else {
                return Err(error);
            };
            self.retry(error, backoff).await?;
        }
    }

    /// Wait out a short block, then take this attempt's permits and slots.
    async fn admit(&mut self) -> Result<Admission, ProviderError> {
        self.context.check()?;
        let cap = self.transport.retry.max_rate_limit_wait;
        // Admission waits (a rate-limit block, search spacing, the
        // code-search window, a permit) may use the request time left or
        // `cap`, whichever ends later, within the call's own deadline;
        // the time they take is credited back to the request budget.
        let queued = Instant::now();
        let wait_deadline = self.context.budget.admission_deadline(cap);
        self.state
            .wait_unblocked(
                self.resource.bucket(),
                self.config,
                cap,
                wait_deadline,
                &self.context.budget.cancellation,
            )
            .await?;
        // The code-search window is charged once per logical request.
        let charge_window = self.resource == GitHubResource::CodeSearch && !self.counted_window;
        let admission = self
            .state
            .admit(
                self.group,
                self.config,
                charge_window,
                cap,
                wait_deadline,
                &self.context.budget.cancellation,
            )
            .await?;
        self.context.budget.credit_wait(queued.elapsed());
        self.counted_window |= charge_window;
        Ok(admission)
    }

    /// Send the attempt within the request deadline; `None` when it failed
    /// in transport.
    async fn send(&self) -> Result<Option<reqwest::Response>, ProviderError> {
        let spec = &self.spec;
        let mut request = match spec.method {
            HttpMethod::Get => self.transport.client.get(spec.url.clone()),
            HttpMethod::Post => self.transport.client.post(spec.url.clone()),
        }
        .header(USER_AGENT, HTTP_USER_AGENT)
        .header(ACCEPT, "application/vnd.github+json")
        .header("x-github-api-version", "2022-11-28")
        .headers(spec.headers.clone());
        if let Some(token) = self.context.resolved_credential() {
            request = authorize(request, token.expose_secret());
        }
        if let Some(body) = &spec.body {
            request = request.json(body);
        }
        Ok(self.context.budget.wait(request.send()).await?.ok())
    }

    /// Back off after a transport failure, or fail once attempts run out.
    async fn back_off_transport(&mut self) -> Result<(), ProviderError> {
        *self.failed = Some(CircuitCause::Transport);
        let retry = &self.transport.retry;
        if *self.attempt + 1 < retry.max_attempts {
            let delay = retry.backoff().next_delay(u32::from(*self.attempt));
            pause(
                delay,
                self.context.budget.effective_deadline(),
                &self.context.budget.cancellation,
            )
            .await?;
            *self.attempt += 1;
            return Ok(());
        }
        Err(ProviderError::new(
            ProviderErrorKind::Transport,
            "GitHub transport failed",
        ))
    }

    /// Point the next attempt at a redirect's location, or refuse it.
    fn follow_redirect(
        &mut self,
        status: StatusCode,
        headers: &HeaderMap,
    ) -> Result<(), ProviderError> {
        // GitHub answers 301 for renamed repositories; follow
        // bounded same-origin GET redirects so renamed repos
        // stay reachable. `permits` gates the new location, so
        // the Authorization header never leaves the configured
        // API origin.
        let location = headers
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| self.spec.url.join(value).ok());
        if let Some(location) = location
            && matches!(self.spec.method, HttpMethod::Get)
            && self.redirects < 3
            && self.transport.endpoint.permits(&location)
        {
            self.redirects += 1;
            self.transport.record_rename(&self.spec.url, &location);
            self.spec.url = location;
            return Ok(());
        }
        Err(ProviderError {
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
            sso_url: None,
        })
    }

    /// The error for a failed response, after recording any rate limit it
    /// carries, and how it may be retried (`None`: not at all).
    fn classify(
        &mut self,
        status: StatusCode,
        headers: &HeaderMap,
        body: Bytes,
        body_note: Option<&str>,
    ) -> (ProviderError, Option<RetryWait>) {
        let failure = classify_failure(status, headers, &body, self.config);
        let mut error = response_error(status, headers, body);
        if let Some(note) = body_note {
            error.message = format!("{} ({note})", error.message).into_boxed_str();
        }
        let resource = self.resource;
        let backoff = match failure {
            Failure::Primary { reset, wait } => {
                let bucket = headers
                    .get("x-ratelimit-resource")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or(resource.bucket())
                    .to_owned();
                self.state.exhaust(&bucket, reset);
                error.kind = ProviderErrorKind::RateLimited;
                error.retryable = true;
                error.rate_limit =
                    Some(Box::new(RateLimit {
                        remaining: Some(0),
                        reset_epoch_seconds: Some(reset),
                        retry_after_seconds: Some(provider_retry_after(headers).map_or_else(
                            || crate::budget::ceil_secs(wait),
                            crate::budget::ceil_secs,
                        )),
                        resource: Some(bucket.into()),
                    }));
                // The next loop turn waits in `wait_unblocked`.
                Some(RetryWait {
                    wait,
                    sleep: Duration::ZERO,
                    rate_limited: true,
                })
            }
            Failure::Secondary { wait } => {
                *self.failed = Some(CircuitCause::Secondary);
                let cooldown_end = now_ms().saturating_add(wait.as_millis() as u64);
                self.state.cool_down(cooldown_end);
                error.kind = ProviderErrorKind::RateLimited;
                error.retryable = true;
                error.message = "GitHub secondary rate limit exceeded".into();
                error.rate_limit = Some(Box::new(RateLimit {
                    remaining: header_u64(headers, "x-ratelimit-remaining"),
                    // The cooldown end: the primary window reset GitHub
                    // also sends is unrelated to when a retry can run.
                    reset_epoch_seconds: Some(cooldown_end.div_ceil(1000)),
                    retry_after_seconds: Some(crate::budget::ceil_secs(wait)),
                    resource: Some(
                        headers
                            .get("x-ratelimit-resource")
                            .and_then(|value| value.to_str().ok())
                            .unwrap_or(resource.bucket())
                            .into(),
                    ),
                }));
                Some(RetryWait {
                    wait,
                    sleep: Duration::ZERO,
                    rate_limited: true,
                })
            }
            Failure::Server => {
                *self.failed = Some(CircuitCause::Server);
                let requested = provider_retry_after(headers);
                if let Some(requested) = requested {
                    // A long provider delay fails fast in `retry`; the
                    // caller still learns when to retry.
                    error.rate_limit = Some(Box::new(RateLimit {
                        remaining: None,
                        reset_epoch_seconds: None,
                        retry_after_seconds: Some(crate::budget::ceil_secs(requested)),
                        resource: None,
                    }));
                }
                let retry = &self.transport.retry;
                let delay = retry.backoff().after(requested, u32::from(*self.attempt));
                Some(RetryWait {
                    wait: delay,
                    sleep: delay,
                    rate_limited: false,
                })
            }
            Failure::Final => None,
        };
        (error, backoff)
    }

    /// Sleep before the next attempt, or fail with `error` when attempts run
    /// out or the wait is past its limit or the deadline.
    async fn retry(
        &mut self,
        error: ProviderError,
        backoff: RetryWait,
    ) -> Result<(), ProviderError> {
        let retry = &self.transport.retry;
        let budget = &self.context.budget;
        // A rate limit waits up to `cap` within the call's deadline (the
        // next turn's admission waits and credits it); a 5xx backoff
        // spends request time.
        let (limit, deadline) = if backoff.rate_limited {
            (retry.max_rate_limit_wait, budget.wait_until)
        } else {
            (retry.max_retry_after, budget.effective_deadline())
        };
        if *self.attempt + 1 >= retry.max_attempts
            || backoff.wait > limit
            || Instant::now() + backoff.wait >= deadline
        {
            return Err(error);
        }
        pause(
            backoff.sleep,
            budget.effective_deadline(),
            &budget.cancellation,
        )
        .await?;
        *self.attempt += 1;
        Ok(())
    }
}

/// An error response's body, and why it is empty when it could not be read.
/// A cancel while reading it is a cancel, not an empty body classified by
/// status; an oversized body keeps the status classification.
async fn read_error_body(
    response: reqwest::Response,
    context: &RequestContext,
) -> Result<(Bytes, Option<&'static str>), ProviderError> {
    match read_bounded(response, context).await {
        Ok(body) => Ok((body, None)),
        Err(error) if error.kind == ProviderErrorKind::Cancelled => Err(error),
        Err(error) if error.kind == ProviderErrorKind::ResponseTooLarge => {
            Ok((Bytes::new(), Some("error body exceeded limit")))
        }
        Err(_) => Ok((Bytes::new(), Some("error body could not be read"))),
    }
}

/// The User-Agent every GitHub request (API and OAuth) sends.
pub const HTTP_USER_AGENT: &str = "octocode-native";

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

/// The longest provider-requested retry delay honored: a day.
const MAX_PROVIDER_RETRY_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// `Retry-After` (delta-seconds or HTTP-date) or `retry-after-ms`.
fn provider_retry_after(headers: &HeaderMap) -> Option<Duration> {
    retry_after_delay(
        headers,
        MAX_PROVIDER_RETRY_AFTER,
        std::time::SystemTime::now(),
    )
}

fn classify_failure(
    status: StatusCode,
    headers: &HeaderMap,
    body: &[u8],
    config: &super::budget::ExecutorConfig,
) -> Failure {
    let code = status.as_u16();
    let remaining = header_u64(headers, "x-ratelimit-remaining");
    let requested = provider_retry_after(headers);
    let retry_after = requested.map(crate::budget::ceil_secs);
    let reset = header_u64(headers, "x-ratelimit-reset");
    let text = String::from_utf8_lossy(body);
    if is_secondary_rate_limit(code, remaining, retry_after, &text) {
        let wait = match (requested, remaining, reset) {
            (Some(requested), _, _) => requested,
            (None, Some(0), Some(reset)) => config.reset_wait(reset),
            _ => config.secondary_default,
        };
        return Failure::Secondary { wait };
    }
    if is_primary_rate_limit(code, remaining, &text) {
        let reset = reset.unwrap_or_else(|| {
            now_ms() / 1000 + retry_after.unwrap_or(config.secondary_default.as_secs())
        });
        return Failure::Primary {
            reset,
            wait: config.reset_wait(reset),
        };
    }
    if classify_status(code).is_server() {
        Failure::Server
    } else {
        Failure::Final
    }
}

/// A response body within the request budget and its body limit.
async fn read_bounded(
    response: reqwest::Response,
    context: &RequestContext,
) -> Result<Bytes, ProviderError> {
    octocode_io::retry::read_bounded(response, &context.budget, context.budget.max_body_bytes)
        .await
        .map_err(|error| match error {
            // A body that stalls after the headers is bounded by the
            // request deadline too, not only by the outer runtime timeout.
            BodyError::Stopped(BudgetStop::Deadline) => ProviderError::new(
                ProviderErrorKind::Timeout,
                "GitHub response body deadline exceeded",
            ),
            BodyError::Stopped(stop) => stop.into(),
            BodyError::TooLarge => ProviderError::new(
                ProviderErrorKind::ResponseTooLarge,
                "GitHub response exceeded configured byte limit",
            ),
            BodyError::Transport => ProviderError::new(
                ProviderErrorKind::Transport,
                "failed reading GitHub response",
            ),
        })
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
/// Attach the GitHub token as a bearer `Authorization` header. `bearer_auth`
/// marks the value sensitive, so `Debug` of the request redacts it.
fn authorize(request: reqwest::RequestBuilder, token: &str) -> reqwest::RequestBuilder {
    request.bearer_auth(token)
}

fn response_error(status: StatusCode, headers: &HeaderMap, body: Bytes) -> ProviderError {
    let parsed: ErrorBody = serde_json::from_slice(&body).unwrap_or_default();
    // GitHub names a branch, tag, or SHA it cannot resolve "No commit found
    // for SHA: …" (422) or "No commit found for the ref …" (404): a missing
    // ref, typed once here so no tool matches the message text.
    let missing_ref = matches!(status.as_u16(), 404 | 422)
        && parsed
            .message
            .as_deref()
            .is_some_and(|message| message.starts_with("No commit found"));
    let kind = match status.as_u16() {
        _ if missing_ref => ProviderErrorKind::NotFound,
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
        reason: missing_ref.then_some(ProviderErrorReason::RefNotFound),
        sso_url: sso_directive(headers, "required")
            .and_then(|parameters| sso_parameter(parameters, "url"))
            .filter(|url| Url::parse(url).is_ok_and(|url| matches!(url.scheme(), "http" | "https")))
            .map(Into::into),
    }
}

/// The parameters of an `x-github-sso: <directive>; k=v; …` header, when it
/// names `directive` (`required` or `partial-results`).
fn sso_directive<'a>(headers: &'a HeaderMap, directive: &str) -> Option<&'a str> {
    let value = headers.get("x-github-sso")?.to_str().ok()?;
    let (head, parameters) = value.split_once(';').unwrap_or((value, ""));
    head.trim()
        .eq_ignore_ascii_case(directive)
        .then_some(parameters)
}

fn sso_parameter<'a>(parameters: &'a str, name: &str) -> Option<&'a str> {
    parameters.split(';').find_map(|part| {
        let (key, value) = part.split_once('=')?;
        (key.trim() == name).then(|| value.trim())
    })
}

#[cfg(test)]
mod response_error_tests {
    use super::*;
    use reqwest::header::AUTHORIZATION;

    /// A loopback API (a test server, a local proxy to GHES) is reached
    /// directly: no system proxy lookup, which is the first client's
    /// startup cost, and no proxy that could see the token.
    #[test]
    fn a_loopback_api_connects_directly() {
        use octocode_io::http::Proxy;
        for (base, proxy) in [
            ("http://127.0.0.1:8080/api/v3", Proxy::Direct),
            ("http://localhost:1/api/v3", Proxy::Direct),
            ("https://api.github.com", Proxy::System),
            ("https://ghe.example.com/api/v3", Proxy::System),
        ] {
            let endpoint =
                GitHubEndpoint::new(url::Url::parse(base).expect("url")).expect("endpoint");
            assert_eq!(client_policy(&endpoint).proxy, proxy, "{base}");
        }
    }

    /// Review open item: the token header is marked sensitive so `Debug`
    /// output of a request or header map never prints it.
    #[test]
    fn authorization_header_is_sensitive() {
        let request = authorize(
            reqwest::Client::new().get("https://api.github.com/"),
            "ghp_secret",
        )
        .build()
        .expect("request");
        let value = &request.headers()[AUTHORIZATION];
        assert_eq!(value, "Bearer ghp_secret");
        assert!(value.is_sensitive());
        assert!(!format!("{:?}", request.headers()).contains("ghp_secret"));
    }

    /// Every validation cause GitHub returns is kept (never-trim).
    #[test]
    fn validation_errors_keep_every_detail() {
        let body = Bytes::from_static(
            br#"{"message":"Validation Failed","errors":[{"message":"a"},{"message":"b"},{"message":"c"},{"message":"d"},{"message":"e"}]}"#,
        );
        let error = response_error(StatusCode::UNPROCESSABLE_ENTITY, &HeaderMap::new(), body);
        assert_eq!(&*error.message, "Validation Failed: a; b; c; d; e");
    }

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

    /// GitHub's "No commit found" (422 on commits/compare, 404 on contents)
    /// is a missing ref for every tool, typed once here.
    #[test]
    fn no_commit_found_is_a_missing_ref() {
        for (status, body) in [
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                &br#"{"message":"No commit found for SHA: nope"}"#[..],
            ),
            (
                StatusCode::NOT_FOUND,
                &br#"{"message":"No commit found for the ref nope"}"#[..],
            ),
        ] {
            let error = response_error(status, &HeaderMap::new(), Bytes::from_static(body));
            assert_eq!(error.kind, ProviderErrorKind::NotFound);
            assert_eq!(error.reason, Some(ProviderErrorReason::RefNotFound));
            assert_eq!(error.status, Some(status.as_u16()));
        }
        let missing = response_error(
            StatusCode::NOT_FOUND,
            &HeaderMap::new(),
            Bytes::from_static(br#"{"message":"Not Found"}"#),
        );
        assert_eq!(missing.reason, None);
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
