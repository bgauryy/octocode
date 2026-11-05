//! GraphQL execution: every document is a read, and a limit GitHub reports on
//! HTTP 200 counts as one.
use octocode_io::{
    budget::{BudgetStop, pause},
    retry::header_u64,
};
use serde_json::Value;
use std::time::Instant;

use super::attempts::{secondary_cooldown, secondary_wait};
use super::execute::Obtained;
use super::{GitHubTransport, GraphQlError, GraphQlPage, RequestContext, RequestSpec};
use crate::budget::{CircuitCause, GitHubResource, now_ms, rate_limited_error};
use crate::{ProviderError, ProviderErrorKind, RateLimit};

/// Whether a GraphQL document holds a mutation operation: an operation keyword
/// outside any selection, arguments, comment or string.
fn holds_mutation(document: &str) -> bool {
    let mut depth = 0usize;
    let mut chars = document.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        match c {
            '#' => {
                for (_, skipped) in chars.by_ref() {
                    if skipped == '\n' {
                        break;
                    }
                }
            }
            '"' => {
                while let Some((_, skipped)) = chars.next() {
                    match skipped {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth = depth.saturating_sub(1),
            c if depth == 0 && (c.is_alphabetic() || c == '_') => {
                let mut end = at + c.len_utf8();
                while let Some(&(next, word)) = chars.peek() {
                    if !(word.is_alphanumeric() || word == '_') {
                        break;
                    }
                    end = next + word.len_utf8();
                    chars.next();
                }
                if &document[at..end] == "mutation" {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

impl GitHubTransport {
    fn graphql_spec(&self, query: &str, variables: Value) -> Result<RequestSpec, ProviderError> {
        // GraphQL reads are unspaced because they are reads: GitHub asks for
        // the pause between mutative requests ([`crate::budget::ExecutorConfig`]).
        if holds_mutation(query) {
            return Err(ProviderError::new(
                ProviderErrorKind::Validation,
                "GraphQL mutations are not sent: this transport only reads.",
            ));
        }
        Ok(RequestSpec::graphql(
            self.endpoint.graphql(),
            serde_json::json!({ "query": query, "variables": variables }),
        ))
    }

    pub async fn execute_graphql(
        &self,
        query: &str,
        variables: Value,
        context: &RequestContext,
    ) -> Result<GraphQlPage, ProviderError> {
        self.graphql(self.graphql_spec(query, variables)?, context)
            .await
    }

    /// [`Self::execute_graphql`] for a query bound to a commit: its answer
    /// cannot change, so a kept one answers it.
    pub(crate) async fn execute_pinned_graphql(
        &self,
        query: &str,
        variables: Value,
        context: &RequestContext,
    ) -> Result<GraphQlPage, ProviderError> {
        let spec = self.graphql_spec(query, variables)?.pinned_to_commit();
        self.graphql(spec, context).await
    }

    /// [`Self::execute_graphql`] for a read of one pull request: a kept answer
    /// stands for as long as GitHub says that pull request's REST read stands,
    /// disclosed by when it was observed.
    pub async fn execute_pull_request_graphql(
        &self,
        query: &str,
        variables: Value,
        context: &RequestContext,
    ) -> Result<GraphQlPage, ProviderError> {
        let spec = self.graphql_spec(query, variables)?.reading_pull_request();
        self.graphql(spec, context).await
    }

    async fn graphql(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
    ) -> Result<GraphQlPage, ProviderError> {
        // One physical-attempt budget for the whole operation: HTTP retries
        // inside `execute_counted` and the semantic retries below share it.
        let mut attempt: u8 = 0;
        loop {
            let Obtained { page, from_origin } = self
                .execute_counted(spec.clone(), context, &mut attempt)
                .await?;
            let parsed: GraphQlPage = serde_json::from_slice(&page.body).map_err(|_| {
                context.journal.forget(&spec);
                ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub GraphQL response")
            })?;
            // An answer with errors is a failure a later ask may not repeat:
            // it is never evidence a continuation replays, or kept.
            if !parsed.errors.is_empty() {
                context.journal.forget(&spec);
            } else if from_origin && spec.keeps_graphql_answer() {
                self.keep(&spec, &page, context);
            }
            // Before the primary check: a secondary limit's `type` can say
            // RATE_LIMITED too, and it blocks the whole key, not one bucket.
            if parsed.errors.iter().any(GraphQlError::is_secondary_limit) {
                let state = self.key_state(context.resolved_credential());
                let config = self.budget.config();
                let wait = secondary_wait(&page.headers, config);
                let limit =
                    secondary_cooldown(&state, &page.headers, wait, GitHubResource::Graphql);
                if attempt + 1 < self.retry.max_attempts
                    && Instant::now() + wait < context.budget.admission_deadline()
                {
                    attempt += 1;
                    continue;
                }
                state.record_circuit_failure(config, CircuitCause::Secondary);
                return Err(rate_limited_error(
                    "GitHub secondary rate limit exceeded",
                    limit,
                ));
            }
            if parsed.errors.iter().any(GraphQlError::is_rate_limited) {
                // GraphQL primary limit arrives as HTTP 200 + errors[].type.
                let state = self.key_state(context.resolved_credential());
                let reset = header_u64(&page.headers, "x-ratelimit-reset")
                    .unwrap_or_else(|| now_ms() / 1000 + 60);
                state.exhaust(GitHubResource::Graphql.bucket(), reset);
                let wait = self.budget.config().reset_wait(reset);
                if attempt + 1 < self.retry.max_attempts
                    && Instant::now() + wait < context.budget.admission_deadline()
                {
                    attempt += 1;
                    continue;
                }
                return Err(rate_limited_error(
                    "GitHub GraphQL rate limit exceeded",
                    RateLimit {
                        remaining: Some(0),
                        reset_epoch_seconds: Some(reset),
                        retry_after_seconds: Some(crate::budget::ceil_secs(wait)),
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
}
