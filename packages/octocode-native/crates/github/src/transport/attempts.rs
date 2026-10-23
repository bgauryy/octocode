//! The attempts of one request: admission, send, redirects and retries.
use reqwest::{
    StatusCode,
    header::{ACCEPT, HeaderMap, LOCATION, USER_AGENT},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use octocode_io::budget::pause;

use super::response::{authorize, parse_next, read_bounded, read_error_body};
use super::{
    GitHubTransport, HTTP_USER_AGENT, HttpMethod, RequestContext, RequestSpec, ResponsePage,
};
use crate::budget::{Admission, CircuitCause, ExecutorConfig, GitHubResource, Group, KeyState};
use crate::{ProviderError, ProviderErrorKind};

mod failure;

/// How a failed attempt may be retried: `wait` is held to the retry limits,
/// `sleep` is spent before the next attempt.
struct RetryWait {
    wait: Duration,
    sleep: Duration,
    /// A rate limit, waited out by the next attempt's admission.
    rate_limited: bool,
}

/// The state one request's attempts share.
pub(super) struct Attempts<'a> {
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
    pub(super) fn new(
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
    pub(super) async fn run(mut self) -> Result<ResponsePage, ProviderError> {
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
