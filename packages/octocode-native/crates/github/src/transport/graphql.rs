//! GraphQL execution: every document is a read, and a limit GitHub reports on
//! HTTP 200 counts as one.
use octocode_io::{
    budget::{BudgetStop, pause},
    retry::header_u64,
};
use serde_json::Value;

use super::attempts::{secondary_cooldown, secondary_wait, spent_bucket_reset};
use super::execute::Obtained;
use super::{
    GitHubTransport, GraphQlError, GraphQlPage, RequestContext, RequestSpec, ResponsePage,
};
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

    /// [`Self::execute_graphql`] for a read of one pull request: a kept answer
    /// stands for as long as GitHub says that pull request's REST read stands,
    /// disclosed by when it was observed.
    ///
    /// A batch that read its rows' pull requests in one aliased request
    /// ([`crate::RefMemo::record_pull_request`]) answers this read with the
    /// part that is this read's own, recorded and kept as its answer.
    pub async fn execute_pull_request_graphql(
        &self,
        query: &str,
        variables: Value,
        context: &RequestContext,
    ) -> Result<GraphQlPage, ProviderError> {
        let spec = self.graphql_spec(query, variables)?.reading_pull_request();
        if let Some(memo) = context.ref_memo()
            && let Some(body) = spec.body.as_ref().map(ToString::to_string)
            && let Some(page) = memo.pull_request(&body).await
            && let Ok(parsed) = serde_json::from_slice::<GraphQlPage>(&page.body)
        {
            memo.adopt_waits(context);
            let page = super::execute::joined(&spec, page, context);
            if parsed.errors.is_empty() {
                self.keep(&spec, &page, context);
            } else {
                context.journal.forget(&spec);
            }
            return Ok(parsed);
        }
        self.graphql(spec, context).await
    }

    /// [`Self::execute_graphql`] for a read of git objects named by their
    /// ids (tag objects, blobs, trees), which cannot change: an answer kept
    /// for the same read answers it with nothing sent, and an answer with no
    /// `errors[]` that `complete` finds names every object asked is kept
    /// pinned. An object the answer did not name may be pushed later, so an
    /// answer missing one is not kept.
    pub(crate) async fn execute_object_graphql(
        &self,
        query: &str,
        variables: Value,
        context: &RequestContext,
        complete: impl Fn(&GraphQlPage) -> bool,
    ) -> Result<GraphQlPage, ProviderError> {
        let spec = self.graphql_spec(query, variables)?.reading_objects();
        let (page, fresh) = self.graphql_uncoordinated(spec.clone(), context).await?;
        if let Some(raw) = fresh
            && complete(&page)
        {
            self.keep(&spec, &raw, context);
        }
        Ok(page)
    }

    /// A GraphQL read; identical reads of one pull request asked together
    /// share one fill ([`GitHubTransport::fill_key`]): the first sends it,
    /// and the rest take the answer GitHub gave it, so a call that reads one
    /// pull request in two rows asks GitHub once.
    async fn graphql(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
    ) -> Result<GraphQlPage, ProviderError> {
        if let Some(key) = self.fill_key(&spec, context) {
            let ticket = self.fills.join(key);
            let mut fill = context.budget.wait(ticket.turn()).await?;
            if let Some(page) = fill.answer.clone() {
                drop(fill);
                let page = super::execute::joined(&spec, page, context);
                return serde_json::from_slice(&page.body).map_err(|_| {
                    ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub GraphQL response")
                });
            }
            if !fill.started {
                fill.started = true;
                let result = self.graphql_uncoordinated(spec.clone(), context).await;
                if let Ok((_, Some(raw))) = &result
                    && self.shares_in_flight(&spec, raw)
                {
                    fill.answer = Some(raw.clone());
                }
                drop(fill);
                return result.map(|(page, _)| page);
            }
        }
        self.graphql_uncoordinated(spec, context)
            .await
            .map(|(page, _)| page)
    }

    /// [`Self::graphql`] outside any fill, with GitHub's page when it sent
    /// it just now and it holds no `errors[]` (what a waiting identical read
    /// may take).
    async fn graphql_uncoordinated(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
    ) -> Result<(GraphQlPage, Option<ResponsePage>), ProviderError> {
        // One physical-attempt budget for the whole operation: HTTP retries
        // inside `execute_counted` and the semantic retries below share it.
        let mut attempt: u8 = 0;
        loop {
            // GraphQL is never redirected along a repository rename.
            let Obtained {
                page, from_origin, ..
            } = self
                .execute_counted(spec.clone(), context, &mut attempt, &mut Vec::new())
                .await?;
            let parsed: GraphQlPage = serde_json::from_slice(&page.body).map_err(|_| {
                context.journal.forget(&spec);
                ProviderError::new(ProviderErrorKind::Decode, "invalid GitHub GraphQL response")
            })?;
            // An answer with errors is a failure a later ask may not repeat:
            // it is never evidence a continuation replays, or kept.
            if !parsed.errors.is_empty() {
                context.journal.forget(&spec);
            } else if from_origin && spec.reads_pull_request {
                self.keep(&spec, &page, context);
            }
            // Before the primary check: a secondary limit's `type` can say
            // RATE_LIMITED too, and it blocks the whole key, not one bucket.
            // A spent bucket in the headers is that bucket's limit, as on REST.
            let spent = spent_bucket_reset(&page.headers);
            if spent.is_none() && parsed.errors.iter().any(GraphQlError::is_secondary_limit) {
                let state = self.key_state(context.resolved_credential());
                let config = self.budget.config();
                let wait = secondary_wait(&page.headers, config, &self.retry, attempt);
                let limit =
                    secondary_cooldown(&state, &page.headers, wait, GitHubResource::Graphql);
                if attempt + 1 < self.retry.max_attempts && context.wait_limit().takes(wait) {
                    attempt += 1;
                    continue;
                }
                state.record_circuit_failure(config, CircuitCause::Secondary);
                return Err(rate_limited_error(
                    "GitHub secondary rate limit exceeded",
                    limit,
                ));
            }
            let secondary_on_spent =
                spent.is_some() && parsed.errors.iter().any(GraphQlError::is_secondary_limit);
            if secondary_on_spent || parsed.errors.iter().any(GraphQlError::is_rate_limited) {
                // GraphQL primary limit arrives as HTTP 200 + errors[].type.
                let state = self.key_state(context.resolved_credential());
                if secondary_on_spent {
                    // As on REST: the named secondary limit cools the key
                    // beside the spent bucket.
                    let config = self.budget.config();
                    secondary_cooldown(
                        &state,
                        &page.headers,
                        secondary_wait(&page.headers, config, &self.retry, attempt),
                        GitHubResource::Graphql,
                    );
                    state.record_circuit_failure(config, CircuitCause::Secondary);
                }
                let reset = spent
                    .or_else(|| header_u64(&page.headers, "x-ratelimit-reset"))
                    .unwrap_or_else(|| now_ms() / 1000 + 60);
                state.exhaust(GitHubResource::Graphql.bucket(), reset);
                let wait = self.budget.config().reset_wait(reset);
                if crate::budget::waits_out_spent(GitHubResource::Graphql.bucket())
                    && attempt + 1 < self.retry.max_attempts
                    && context.wait_limit().takes(wait)
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
                    if context
                        .waits
                        .timed(
                            octocode_contract_base::ClientWaitKind::RetryAfter,
                            pause(delay, context.budget.deadline, &context.budget.cancellation),
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
            let fresh = (from_origin && parsed.errors.is_empty()).then_some(page);
            return Ok((parsed, fresh));
        }
    }
}
