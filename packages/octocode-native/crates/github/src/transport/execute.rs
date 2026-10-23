//! One logical request on the transport: REST and GraphQL execution with
//! the circuit breaker's per-operation failure accounting.
use octocode_io::{
    budget::{BudgetStop, pause},
    retry::header_u64,
};
use serde_json::Value;
use std::time::Instant;

use super::attempts::Attempts;
use super::{
    GitHubTransport, GraphQlError, GraphQlPage, RequestContext, RequestSpec, ResponsePage,
};
use crate::budget::{CircuitCause, GitHubResource, now_ms, rate_limited_error};
use crate::{ProviderError, ProviderErrorKind, RateLimit};

impl GitHubTransport {
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
