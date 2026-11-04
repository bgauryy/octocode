//! One logical request on the transport: REST and GraphQL execution with
//! the circuit breaker's per-operation failure accounting.
use octocode_io::{
    budget::{BudgetStop, pause},
    retry::header_u64,
};
use reqwest::header::HeaderValue;
use serde::Deserialize;
use serde_json::Value;
use std::time::{Duration, Instant};

use super::attempts::Attempts;
use super::evidence::{answer_of, page_of, request_key};
use super::{
    GitHubTransport, GraphQlError, GraphQlPage, HttpMethod, RequestContext, RequestSpec,
    ResponsePage, Reuse, SavedAnswer, worth_keeping,
};
use crate::budget::{CircuitCause, GitHubResource, now_ms, rate_limited_error};
use crate::{Partition, ProviderError, ProviderErrorKind, RateLimit};

/// How long a GitHub search answer stands in for asking again. GitHub's search
/// endpoints send no validator (`ETag`, `Last-Modified`) and no lifetime, so no
/// answer can be confirmed: this is a chosen bound, the one freshness number
/// the origin does not state. Each reuse is disclosed
/// ([`RequestContext::reused_since`]), and a search hit is a lead that a read
/// (which is validated) settles.
pub(crate) const SEARCH_REUSE_WINDOW: Duration = Duration::from_secs(300);

/// A page, and whether the origin sent it just now.
struct Obtained {
    page: ResponsePage,
    from_origin: bool,
}

/// A saved answer found for a request, and how to put it back.
struct Saved {
    scope: String,
    request: String,
    saved: SavedAnswer,
    reuse: Reuse,
}

/// Whether GitHub said this answer lacks results: `incomplete_results` on a
/// search, or organizations SAML enforcement withheld.
fn is_partial(page: &ResponsePage) -> bool {
    #[derive(Deserialize)]
    struct Search {
        #[serde(default)]
        incomplete_results: bool,
    }
    !page.sso_partial_organizations().is_empty()
        || serde_json::from_slice::<Search>(&page.body).is_ok_and(|page| page.incomplete_results)
}

impl GitHubTransport {
    pub async fn execute_graphql(
        &self,
        query: &str,
        variables: Value,
        context: &RequestContext,
    ) -> Result<GraphQlPage, ProviderError> {
        let spec = RequestSpec::graphql(
            self.endpoint.graphql(),
            serde_json::json!({ "query": query, "variables": variables }),
        );
        self.graphql(spec, context).await
    }

    /// [`Self::execute_graphql`] for a query bound to a commit: its answer
    /// cannot change, so a kept one answers it.
    pub(crate) async fn execute_pinned_graphql(
        &self,
        query: &str,
        variables: Value,
        context: &RequestContext,
    ) -> Result<GraphQlPage, ProviderError> {
        let spec = RequestSpec::graphql(
            self.endpoint.graphql(),
            serde_json::json!({ "query": query, "variables": variables }),
        )
        .pinned_to_commit();
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
            } else if from_origin && spec.is_pinned() {
                self.keep(&spec, &page, context);
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

    /// One logical request: its attempts count one failure toward the
    /// circuit breaker, however many retries it took.
    pub async fn execute(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
    ) -> Result<ResponsePage, ProviderError> {
        let Obtained { page, from_origin } =
            self.execute_counted(spec.clone(), context, &mut 0).await?;
        if from_origin && matches!(spec.method, HttpMethod::Get) {
            self.keep(&spec, &page, context);
        }
        Ok(page)
    }

    /// [`Self::execute`] drawing on `attempt`, the physical-attempt count of
    /// an enclosing logical operation: its sends and an outer semantic retry
    /// share one `max_attempts` budget.
    async fn execute_counted(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
        attempt: &mut u8,
    ) -> Result<Obtained, ProviderError> {
        // A resumed continuation's evidence answers what it recorded.
        if let Some(answer) = context.journal.replay(&spec) {
            return answer.map(|page| Obtained {
                page,
                from_origin: false,
            });
        }
        let saved = self.saved(&spec, context);
        let mut sent = spec.clone();
        if let Some(saved) = &saved {
            match &saved.reuse {
                Reuse::Pinned | Reuse::Fresh => {
                    context.check()?;
                    let page = page_of(&saved.saved.answer);
                    context.journal.record_page(&spec, &page);
                    if saved.reuse == Reuse::Fresh {
                        context.record_reused(saved.saved.observed_ms);
                    }
                    return Ok(Obtained {
                        page,
                        from_origin: false,
                    });
                }
                Reuse::Ask(name, value) => {
                    if let Ok(value) = HeaderValue::from_str(value) {
                        sent.headers.insert(name.clone(), value);
                    }
                }
                Reuse::Spent => {}
            }
        }
        let mut failed = None;
        let result = self.run_attempts(sent, context, &mut failed, attempt).await;
        let result = self.settle(result, saved, context);
        let page = result
            .as_ref()
            .map(|obtained| obtained.page.clone())
            .map_err(Clone::clone);
        context.journal.record(&spec, &page);
        if let (Err(error), Some(cause)) = (&result, failed)
            && error.kind != ProviderErrorKind::Cancelled
        {
            let state = self.key_state(context.resolved_credential());
            state.record_circuit_failure(self.budget.config(), cause);
        }
        result
    }

    /// What GitHub's answer means for a saved one: a 304 confirms it and
    /// restarts it; losing the resource (or access to it) drops it.
    fn settle(
        &self,
        result: Result<ResponsePage, ProviderError>,
        saved: Option<Saved>,
        context: &RequestContext,
    ) -> Result<Obtained, ProviderError> {
        match (result, saved) {
            (Ok(page), Some(mut saved)) if page.status == 304 => {
                let headers = page
                    .headers
                    .iter()
                    .filter_map(|(name, value)| {
                        Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned()))
                    })
                    .collect::<Vec<_>>();
                saved.saved.confirm(&headers, now_ms());
                if let Some(store) = &self.answers {
                    store.save(&saved.scope, &saved.request, saved.saved.clone());
                }
                let mut answer = page_of(&saved.saved.answer);
                answer.redirected = page.redirected;
                Ok(Obtained {
                    page: answer,
                    from_origin: false,
                })
            }
            (Ok(page), _) => Ok(Obtained {
                page,
                from_origin: true,
            }),
            // GitHub refused to answer now: the last answer it gave, disclosed
            // by the time it was observed, beats no answer.
            (Err(error), Some(saved)) if error.kind == ProviderErrorKind::RateLimited => {
                context.record_reused(saved.saved.observed_ms);
                Ok(Obtained {
                    page: page_of(&saved.saved.answer),
                    from_origin: false,
                })
            }
            (Err(error), Some(saved)) => {
                if matches!(
                    error.kind,
                    ProviderErrorKind::NotFound
                        | ProviderErrorKind::Permission
                        | ProviderErrorKind::Authentication
                        | ProviderErrorKind::Unavailable
                ) && let Some(store) = &self.answers
                {
                    store.forget(&saved.scope, &saved.request);
                }
                Err(error)
            }
            (Err(error), None) => Err(error),
        }
    }

    /// The saved answer to `spec` under this request's credential, and what
    /// it can do. A body past this call's limit answers nothing: the request
    /// reaches GitHub and meets that limit there.
    fn saved(&self, spec: &RequestSpec, context: &RequestContext) -> Option<Saved> {
        let store = self.answers.as_ref()?;
        if !matches!(spec.method, HttpMethod::Get) && !spec.is_pinned() {
            return None;
        }
        let scope = self.answer_scope(context)?;
        let request = request_key(spec);
        let saved = store.load(&scope, &request)?;
        if saved.answer.body.len() > context.budget.max_body_bytes {
            return None;
        }
        let reuse = saved.reuse(spec.is_pinned(), self.reuse_window(spec), now_ms());
        Some(Saved {
            scope,
            request,
            saved,
            reuse,
        })
    }

    /// Whether `spec` would be answered with nothing sent: a resumed
    /// continuation's journal or a saved answer that needs no asking.
    pub(crate) fn answers_unasked(&self, spec: &RequestSpec, context: &RequestContext) -> bool {
        context.journal_answers(spec)
            || self
                .saved(spec, context)
                .is_some_and(|saved| matches!(saved.reuse, Reuse::Pinned | Reuse::Fresh))
    }

    /// Keep `page`, the origin's answer to `spec`, when something can later
    /// vouch for it. A partial answer (GitHub says some results are missing,
    /// or SAML withheld some) is worth asking again and is never kept.
    fn keep(&self, spec: &RequestSpec, page: &ResponsePage, context: &RequestContext) {
        let Some(store) = &self.answers else {
            return;
        };
        if page.status != 200 || is_partial(page) {
            return;
        }
        let Some(scope) = self.answer_scope(context) else {
            return;
        };
        let request = request_key(spec);
        let Some(answer) = answer_of(&request, page) else {
            return;
        };
        if worth_keeping(&answer.headers, spec.is_pinned(), self.reuse_window(spec)) {
            store.save(
                &scope,
                &request,
                SavedAnswer {
                    answer,
                    observed_ms: now_ms(),
                    redirected: page.redirected,
                },
            );
        }
    }

    /// Answers of one API origin and exact credential: never another's.
    fn answer_scope(&self, context: &RequestContext) -> Option<String> {
        let origin = self.endpoint.rest(&[]).ok()?;
        let secret = context
            .resolved_credential()
            .map(crate::ResolvedCredential::secret);
        Some(
            Partition::new(origin.as_str(), secret)
                .identity()
                .to_owned(),
        )
    }

    /// How long an answer `spec` gets with no validator or lifetime may stand.
    fn reuse_window(&self, spec: &RequestSpec) -> Option<Duration> {
        matches!(
            self.endpoint.resource(&spec.url),
            GitHubResource::Search | GitHubResource::CodeSearch
        )
        .then_some(SEARCH_REUSE_WINDOW)
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
