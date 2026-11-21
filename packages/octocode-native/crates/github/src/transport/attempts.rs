//! The attempts of one request: admission, send, redirects and retries.
use reqwest::{
    StatusCode,
    header::{ACCEPT, HeaderMap, LOCATION, USER_AGENT},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use octocode_contract_base::ClientWaitKind;
use octocode_io::budget::pause;

use super::response::{authorize, parse_next, read_bounded, read_error_body};
use super::{
    GitHubTransport, HTTP_USER_AGENT, HttpMethod, RequestContext, RequestSpec, ResponsePage,
};
use crate::budget::{Admission, CircuitCause, ExecutorConfig, GitHubResource, Group, KeyState};
use crate::{ProviderError, ProviderErrorKind};

mod failure;

pub(super) use failure::{secondary_cooldown, secondary_wait, spent_bucket_reset};

/// How a failed attempt may be retried: `wait` is held to the retry limits,
/// `sleep` is spent before the next attempt.
struct RetryWait {
    wait: Duration,
    sleep: Duration,
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
    /// The rename routes this request's redirects followed, as (old
    /// repository URL, its `/repositories/{id}` URL): the call's own, never
    /// another call's on the same context.
    followed: &'a mut Vec<(url::Url, url::Url)>,
    /// The operation's physical attempts so far (0-based index of the
    /// next send's predecessor), shared with any outer semantic retry.
    attempt: &'a mut u8,
}

impl<'a> Attempts<'a> {
    pub(super) fn new(
        transport: &'a GitHubTransport,
        spec: RequestSpec,
        context: &'a RequestContext,
        failed: &'a mut Option<CircuitCause>,
        attempt: &'a mut u8,
        followed: &'a mut Vec<(url::Url, url::Url)>,
    ) -> Self {
        let state = transport.key_state(context.resolved_credential());
        let config = transport.budget.config();
        let resource = transport.endpoint.resource(&spec.url);
        let group = match (resource, spec.method) {
            (GitHubResource::Graphql, _) => Some(Group::Graphql),
            (GitHubResource::Search, _) => Some(Group::Search),
            (GitHubResource::CodeSearch, _) => Some(Group::CodeSearch),
            (GitHubResource::Core, HttpMethod::Post) => Some(Group::Write),
            (GitHubResource::Core, HttpMethod::Get | HttpMethod::Head) => None,
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
            followed,
            attempt,
        }
    }

    /// Admit, send and classify attempts until one answers, fails for good,
    /// or the retry policy gives up.
    pub(super) async fn run(mut self) -> Result<ResponsePage, ProviderError> {
        loop {
            let admission = self.admit().await?;
            let sent = Instant::now();
            let Some(response) = self.send().await? else {
                // Release permits before backing off.
                drop(admission);
                self.back_off_transport().await?;
                continue;
            };
            let status = response.status();
            let mut headers = response.headers().clone();
            *self.failed = None;
            self.state.observe(&headers, self.resource.bucket());
            if status == StatusCode::NOT_MODIFIED && self.spec.is_conditional() {
                octocode_io::http::account_response_delay(&mut headers, sent.elapsed());
                drop(admission);
                self.state.record_success();
                return Ok(self.page(status, headers, bytes::Bytes::new(), None));
            }
            if status.is_redirection() {
                drop(admission);
                self.follow_redirect(status, &headers)?;
                continue;
            }
            if status.is_success() {
                let next = parse_next(&headers)?;
                let body = read_bounded(response, self.context).await?;
                octocode_io::http::account_response_delay(&mut headers, sent.elapsed());
                drop(admission);
                self.state.record_success();
                return Ok(self.page(status, headers, body, next));
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

    fn page(
        &self,
        status: StatusCode,
        headers: HeaderMap,
        body: bytes::Bytes,
        next: Option<url::Url>,
    ) -> ResponsePage {
        ResponsePage {
            status: status.as_u16(),
            headers,
            body,
            next,
            redirected: self.redirects > 0,
        }
    }

    /// Wait out a block or the quota-paced start, then take this attempt's
    /// permits and slots.
    async fn admit(&mut self) -> Result<Admission, ProviderError> {
        self.context.check()?;
        // Admission waits (a rate-limit block, search pacing, a permit) run
        // within the request's wait limit (its deadline, and its sooner
        // wait bound for a new sleep); only a wait past it is refused.
        let wait_deadline = self.context.wait_limit();
        self.state
            .wait_unblocked(
                self.resource.bucket(),
                self.config,
                wait_deadline,
                &self.context.budget.cancellation,
                &self.context.waits,
            )
            .await?;
        let admission = self
            .state
            .admit(
                self.group,
                self.config,
                wait_deadline,
                &self.context.budget.cancellation,
                &self.context.waits,
            )
            .await?;
        Ok(admission)
    }

    /// Send the attempt within the request deadline; `None` when it failed
    /// in transport.
    async fn send(&self) -> Result<Option<reqwest::Response>, ProviderError> {
        let spec = &self.spec;
        let client = self.transport.client()?;
        let mut request = match spec.method {
            HttpMethod::Get => client.get(spec.url.clone()),
            HttpMethod::Head => client.head(spec.url.clone()),
            HttpMethod::Post => client.post(spec.url.clone()),
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
        Ok(self.context.budget.wait_stalled(request.send()).await?.ok())
    }

    /// Back off after a transport failure, or fail once attempts run out.
    async fn back_off_transport(&mut self) -> Result<(), ProviderError> {
        *self.failed = Some(CircuitCause::Transport);
        let retry = &self.transport.retry;
        if *self.attempt + 1 < retry.max_attempts {
            let delay = retry.backoff().next_delay(u32::from(*self.attempt));
            self.context
                .waits
                .timed(
                    ClientWaitKind::RetryAfter,
                    pause(
                        delay,
                        self.context.budget.deadline,
                        &self.context.budget.cancellation,
                    ),
                )
                .await?;
            *self.attempt += 1;
            return Ok(());
        }
        let mut error = ProviderError::new(ProviderErrorKind::Transport, "GitHub transport failed");
        // The route the failed request took: a proxy that refuses or
        // cannot reach GitHub is the usual cause, and the only one the
        // caller can see.
        error.proxy = octocode_io::http::proxy_in_effect(
            &super::client_policy(&self.transport.endpoint),
            self.spec.url.as_str(),
        )
        .map(Into::into);
        Err(error)
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
            && matches!(self.spec.method, HttpMethod::Get | HttpMethod::Head)
            && self.redirects < 3
            && self.transport.endpoint.permits(&location)
        {
            self.redirects += 1;
            self.context.record_rename(&self.spec.url, &location);
            // Kept routes cannot revalidate the redirect's policy. Only a
            // redirect with no explicit cache policy may use their heuristic
            // lifetime; every permitted redirect is still followed now.
            if (status == StatusCode::MOVED_PERMANENTLY || status == StatusCode::PERMANENT_REDIRECT)
                && !headers.contains_key(reqwest::header::CACHE_CONTROL)
                && let Some((old, new)) = super::route::rename_route(&self.spec.url, &location)
            {
                self.context.record_route(old.clone(), new.clone());
                self.followed.push((old, new));
            }
            self.spec.url = location;
            return Ok(());
        }
        Err(ProviderError {
            kind: ProviderErrorKind::RedirectDenied,
            message: "GitHub API redirect was not followed".into(),
            status: Some(status.as_u16()),
            rate_limit: None,
            retryable: false,
            reason: None,
            sso_url: None,
            proxy: None,
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
        // Every wait is bounded by the request's wait limit alone (a rate
        // limit's by the next turn's admission too).
        if *self.attempt + 1 >= retry.max_attempts || !self.context.wait_limit().takes(backoff.wait)
        {
            return Err(error);
        }
        // The delay before a retry: the `retry-after` GitHub sent, else the
        // backoff. A rate limit sleeps none here; its admission waits.
        self.context
            .waits
            .timed(
                ClientWaitKind::RetryAfter,
                pause(backoff.sleep, budget.deadline, &budget.cancellation),
            )
            .await?;
        *self.attempt += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
