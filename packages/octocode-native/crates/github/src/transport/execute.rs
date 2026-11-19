//! One logical request on the transport, with the circuit breaker's
//! per-operation failure accounting.
use reqwest::header::HeaderValue;
use serde::Deserialize;
use std::time::Duration;
use url::Url;

use super::attempts::Attempts;
use super::evidence::{answer_of, page_of, request_key};
use super::route::{
    Missing, RENAME_ROUTE_WINDOW, commit_read, repository_id_prefix, repository_prefix,
    route_answer, route_key, route_target, routed,
};
use super::{
    GitHubTransport, HttpMethod, RequestContext, RequestSpec, ResponsePage, Reuse, SavedAnswer,
    worth_keeping,
};
use crate::budget::{CircuitCause, GitHubResource, now_ms};
use crate::{Partition, ProviderError, ProviderErrorKind};

/// Disclosed reuse window for search answers without origin validators or cache policy.
/// Hits remain leads; source reads verify them. See [`RequestContext::reused_since`].
pub const SEARCH_REUSE_WINDOW: Duration = Duration::from_secs(300);

/// Disclosed GraphQL PR reuse window (owner decision GHI-3).
/// The origin's REST cache policy takes precedence.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "Disclosed cache freshness policy, not a request deadline or terminal evidence limit"
    )
)]
pub(crate) const PULL_REQUEST_REUSE_WINDOW: Duration = Duration::from_secs(60);

/// A page, and whether the origin sent it just now.
pub(super) struct Obtained {
    pub(super) page: ResponsePage,
    pub(super) from_origin: bool,
    /// GitHub answered this request now: its page, or a `304` confirming a
    /// saved one. A replay or a saved answer served in its place is not.
    pub(super) asked: bool,
}

/// A saved answer found for a request, and how to put it back.
struct Saved {
    scope: String,
    request: String,
    saved: SavedAnswer,
    reuse: Reuse,
}

/// The exact request and credential whose concurrent cache fill may be awaited.
/// The request identity is shared with saved answers and continuation journals.
#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct FillKey {
    scope: String,
    request: String,
}

/// One concurrent wave's fill of a request: whether its first asker has
/// started, and the answer GitHub gave it just now when that answer stands
/// for every identical request asked with it ([`GitHubTransport::shares_in_flight`]).
#[derive(Default)]
pub(super) struct Fill {
    pub(super) started: bool,
    pub(super) answer: Option<ResponsePage>,
}

/// Whether GitHub said this answer lacks results: `incomplete_results` on a
/// search, or organizations SAML enforcement withheld. A non-search body is
/// source data: its own JSON fields have no provider-envelope meaning.
fn is_partial(page: &ResponsePage, resource: GitHubResource) -> bool {
    #[derive(Deserialize)]
    struct Search {
        #[serde(default)]
        incomplete_results: bool,
    }
    !page.sso_partial_organizations().is_empty()
        || (matches!(
            resource,
            GitHubResource::Search | GitHubResource::CodeSearch
        ) && serde_json::from_slice::<Search>(&page.body)
            .is_ok_and(|page| page.incomplete_results))
}

/// Whether a REST `url` reads one pull request or something under it: what the
/// lifetime GitHub states there is about.
fn names_pull_request(url: &Url) -> bool {
    let Some(segments) = url.path_segments() else {
        return false;
    };
    let segments = segments.collect::<Vec<_>>();
    let Some(at) = segments.iter().position(|segment| *segment == "repos") else {
        return false;
    };
    matches!(
        segments.get(at + 3..).unwrap_or_default(),
        ["pulls", number, ..] if number.bytes().all(|byte| byte.is_ascii_digit())
    )
}

/// `page`, the answer GitHub just gave an identical request of this wave
/// (or a batch's read of this one), as this request's own: recorded in its
/// evidence, with what it proves of the repository and commit it reads.
pub(super) fn joined(
    spec: &RequestSpec,
    page: ResponsePage,
    context: &RequestContext,
) -> ResponsePage {
    context.journal.record_page(spec, &page);
    context.record_proven(&spec.url);
    context.record_answered(&spec.url);
    page
}

impl GitHubTransport {
    /// One logical request: its attempts count one failure toward the
    /// circuit breaker, however many retries it took.
    pub async fn execute(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
    ) -> Result<ResponsePage, ProviderError> {
        if let Some(key) = self.fill_key(&spec, context) {
            let ticket = self.fills.join(key);
            let mut fill = context.budget.wait(ticket.turn()).await?;
            if let Some(page) = fill.answer.clone() {
                drop(fill);
                return Ok(joined(&spec, page, context));
            }
            if !fill.started {
                fill.started = true;
                // Hold the first turn through the cache write. On cancellation,
                // dropping the future releases it just as completion does.
                let result = self.execute_uncoordinated(spec.clone(), context).await;
                if let Ok((page, true)) = &result
                    && self.shares_in_flight(&spec, page)
                {
                    fill.answer = Some(page.clone());
                }
                drop(fill);
                return result.map(|(page, _)| page);
            }
            // Wait only once for this wave's fill. When it failed or could not
            // be cached, followers send independently, never in a network convoy.
        }
        self.execute_uncoordinated(spec, context)
            .await
            .map(|(page, _)| page)
    }

    /// The fill a request may join: a GET, or a GraphQL read of one pull
    /// request, with nothing its own continuation already answers.
    pub(super) fn fill_key(&self, spec: &RequestSpec, context: &RequestContext) -> Option<FillKey> {
        if self.answers.is_none()
            || !(matches!(spec.method, HttpMethod::Get) || spec.reads_pull_request)
            || spec.kept_by_caller
            || context.journal_answers(spec)
        {
            return None;
        }
        Some(FillKey {
            scope: self.answer_scope(context)?,
            request: request_key(spec),
        })
    }

    /// Whether the answer GitHub just gave a fill's first asker answers the
    /// identical requests that waited on it, though it may not be kept for
    /// later ones: a search GitHub marks `no-cache`, or a pull request read
    /// by GraphQL. They were asked together, so it is their answer too, not
    /// a reuse in time. An answer the origin forbids anyone to keep
    /// (`no-store`, `Vary: *`) is asked again by each.
    pub(super) fn shares_in_flight(&self, spec: &RequestSpec, page: &ResponsePage) -> bool {
        let headers = page
            .headers
            .iter()
            .filter_map(|(name, value)| {
                Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned()))
            })
            .collect::<Vec<_>>();
        page.status == 200
            && !page.redirected
            && !super::saved::forbids_reuse(&headers)
            && (spec.reads_pull_request
                || matches!(
                    self.endpoint.resource(&spec.url),
                    GitHubResource::Search | GitHubResource::CodeSearch
                ))
    }

    /// One request outside any fill, and whether GitHub answered it just
    /// now without a redirect (an answer a waiting identical request shares).
    async fn execute_uncoordinated(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
    ) -> Result<(ResponsePage, bool), ProviderError> {
        let mut followed = Vec::new();
        let mut route = self.route(&spec, context);
        let mut sent = route
            .as_ref()
            .map_or_else(|| spec.clone(), |(routed, _, _)| routed.clone());
        let replayed = route.is_some() && self.answers_unasked(&sent, context);
        let mut result = self
            .execute_counted(sent.clone(), context, &mut 0, &mut followed)
            .await;
        // A kept route that leads nowhere is forgotten with its rename, and
        // the request asks under the name it was given, as it did before the
        // route was kept. A path missing under a route that still leads to
        // its repository is that repository's answer.
        if !replayed
            && result
                .as_ref()
                .is_err_and(|error| error.kind == ProviderErrorKind::NotFound)
            && let Some((_, key, _)) = &route
            && self
                .route_is_stale(&spec, &sent, context, &mut followed)
                .await
        {
            if let (Some(store), Some(scope)) = (&self.answers, self.answer_scope(context)) {
                store.forget(&scope, key);
            }
            context.forget_rename(&spec.url);
            route = None;
            sent = spec;
            result = self
                .execute_counted(sent.clone(), context, &mut 0, &mut followed)
                .await;
        }
        if result
            .as_ref()
            .is_err_and(|error| error.kind == ProviderErrorKind::NotFound)
            && context.awaits_repository(&request_key(&sent))
        {
            self.prove_repository(&sent.url, context, &mut followed)
                .await;
        }
        self.keep_routes(&followed, context);
        let Obtained {
            mut page,
            from_origin,
            asked,
        } = result?;
        // A redirect GitHub answered now: its repository answered under its id.
        if asked {
            for (_, new) in &followed {
                context.record_answered(new);
            }
        }
        self.keep_proved_missing(context);
        if let Some((_, _, Some(observed_ms))) = &route {
            context.record_reused(*observed_ms);
        }
        if matches!(sent.method, HttpMethod::Get) {
            if from_origin {
                self.keep(&sent, &page, context);
                // A rename just followed: the answer is its target's own,
                // which a routed repeat asks for directly.
                let target = followed.iter().find_map(|(old, new)| {
                    let (prefix, _) = repository_prefix(&sent.url)?;
                    if prefix == *old {
                        routed(&sent.url, new)
                    } else {
                        None
                    }
                });
                if page.redirected
                    && let Some(url) = target
                {
                    let mut direct = sent.clone();
                    direct.url = url;
                    page.redirected = false;
                    self.keep(&direct, &page, context);
                    page.redirected = true;
                }
            }
            self.learn_pull_request_lifetime(&sent, &page, context);
        }
        page.redirected |= route.is_some();
        let answered_now = asked && !page.redirected;
        Ok((page, answered_now))
    }

    /// Route `spec` through its saved rename, returning the store key and observed time.
    /// Record the rename and disclose heuristic reuse.
    fn route(
        &self,
        spec: &RequestSpec,
        context: &RequestContext,
    ) -> Option<(RequestSpec, String, Option<u64>)> {
        if !matches!(spec.method, HttpMethod::Get) {
            return None;
        }
        let store = self.answers.as_ref()?;
        let (prefix, _) = repository_prefix(&spec.url)?;
        let scope = self.answer_scope(context)?;
        let key = route_key(&prefix);
        let saved = store.load(&scope, &key)?;
        let reuse = saved.reuse(false, Some(RENAME_ROUTE_WINDOW), now_ms());
        if !matches!(reuse, Reuse::Fresh | Reuse::Window) {
            return None;
        }
        let target = route_target(&saved)?;
        let url = routed(&spec.url, &target)?;
        if !self.endpoint.permits(&url) {
            return None;
        }
        // A route GitHub stated earlier in this request is no reuse.
        let reused = reuse == Reuse::Window && !context.minted_route(&prefix);
        context.record_rename(&spec.url, &url);
        context.record_route_taken(prefix, target);
        let mut sent = spec.clone();
        sent.url = url;
        Some((sent, key, reused.then_some(saved.observed_ms)))
    }

    /// Whether the kept route `routed` took leads to no repository: the
    /// routed read was the repository itself, or the repository the route
    /// leads to is not found now.
    async fn route_is_stale(
        &self,
        spec: &RequestSpec,
        routed: &RequestSpec,
        context: &RequestContext,
        followed: &mut Vec<(Url, Url)>,
    ) -> bool {
        if repository_prefix(&spec.url).is_some_and(|(_, rest)| rest.is_empty()) {
            return true;
        }
        let Some((repository, _)) = repository_id_prefix(&routed.url) else {
            return false;
        };
        self.obtain_kept(RequestSpec::get(repository), context, followed, false)
            .await
            .is_err_and(|error| error.kind == ProviderErrorKind::NotFound)
    }

    /// Revalidate a cached repository before using it to prove a contents miss.
    /// Cached freshness alone proves no current access; no saved answer means no request.
    async fn prove_repository(
        &self,
        url: &Url,
        context: &RequestContext,
        followed: &mut Vec<(Url, Url)>,
    ) {
        let Some((repository, _)) = repository_prefix(url).or_else(|| repository_id_prefix(url))
        else {
            return;
        };
        let spec = RequestSpec::get(repository);
        if self
            .saved(&spec, context)
            .is_some_and(|saved| saved.saved.revalidation().is_some())
        {
            // The answer or failure matters only as proof, recorded inside.
            let _ = self.obtain_kept(spec, context, followed, true).await;
        }
    }

    /// `spec`'s answer, kept when the origin sent it; `revalidate` asks the
    /// origin to confirm a saved answer whatever lifetime it stated.
    async fn obtain_kept(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
        followed: &mut Vec<(Url, Url)>,
        revalidate: bool,
    ) -> Result<Obtained, ProviderError> {
        let result = self
            .obtain(spec.clone(), context, &mut 0, followed, revalidate)
            .await;
        if let Ok(obtained) = &result
            && obtained.from_origin
        {
            self.keep(&spec, &obtained.page, context);
        }
        result
    }

    /// Keeps the rename routes this call followed for later requests.
    fn keep_routes(&self, followed: &[(Url, Url)], context: &RequestContext) {
        if let (Some(store), Some(scope)) = (&self.answers, self.answer_scope(context)) {
            for (old, new) in followed {
                store.save(&scope, &route_key(old), route_answer(old, new, now_ms()));
            }
        }
    }

    /// Holds a not-found answer for a path at a commit SHA until the commit
    /// is proven: a miss the commit or repository may explain is not kept,
    /// nor one for the object itself, which can be pushed later.
    fn hold_missing(&self, spec: &RequestSpec, error: &ProviderError, context: &RequestContext) {
        let Some(commit) = commit_read(&spec.url).filter(|read| read.path) else {
            return;
        };
        if error.kind != ProviderErrorKind::NotFound
            || error.status != Some(404)
            || error.reason.is_some()
            || !matches!(spec.method, HttpMethod::Get)
            || self.answers.is_none()
        {
            return;
        }
        let Some(scope) = self.answer_scope(context) else {
            return;
        };
        // Under the name asked, and under each other name of the repository
        // this request saw, as a success followed through a rename is kept.
        let mut requests = vec![request_key(spec)];
        for alias in context.aliases(&commit.repository) {
            if let Some(url) = Url::parse(&alias)
                .ok()
                .and_then(|alias| routed(&spec.url, &alias))
            {
                let mut other = spec.clone();
                other.url = url;
                let key = request_key(&other);
                if !requests.contains(&key) {
                    requests.push(key);
                }
            }
        }
        context.hold_missing(Missing::new(
            &spec.url,
            commit,
            scope,
            requests,
            error.clone(),
            now_ms(),
        ));
    }

    /// Keeps the held not-found answers whose commit has been proven.
    fn keep_proved_missing(&self, context: &RequestContext) {
        let Some(store) = &self.answers else {
            return;
        };
        for missing in context.take_proved_missing() {
            for request in &missing.requests {
                let mut saved = missing.saved.clone();
                saved.answer.request.clone_from(request);
                store.save(&missing.scope, request, saved);
            }
        }
    }

    /// Execute within the enclosing operation's shared physical-attempt budget.
    pub(super) async fn execute_counted(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
        attempt: &mut u8,
        followed: &mut Vec<(Url, Url)>,
    ) -> Result<Obtained, ProviderError> {
        self.obtain(spec, context, attempt, followed, false).await
    }

    /// [`Self::execute_counted`]; `revalidate` asks the origin to confirm a
    /// saved answer it would otherwise replay unasked.
    async fn obtain(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
        attempt: &mut u8,
        followed: &mut Vec<(Url, Url)>,
        revalidate: bool,
    ) -> Result<Obtained, ProviderError> {
        // A resumed continuation's evidence answers what it recorded.
        if let Some(answer) = context.journal.replay(&spec) {
            return answer.map(|page| Obtained {
                page,
                from_origin: false,
                asked: false,
            });
        }
        let mut saved = self.saved(&spec, context);
        if revalidate
            && let Some(saved) = &mut saved
            && matches!(saved.reuse, Reuse::Pinned | Reuse::Fresh | Reuse::Window)
            && let Some((name, value)) = saved.saved.revalidation()
        {
            saved.reuse = Reuse::Ask(name, value);
        }
        let mut sent = spec.clone();
        if let Some(saved) = &saved {
            match &saved.reuse {
                Reuse::Pinned | Reuse::Fresh | Reuse::Window => {
                    context.check()?;
                    // A cached answer proves no current repository or commit access.
                    if let Some(error) = &saved.saved.answer.not_found {
                        context.journal.record(&spec, &Err(error.clone()));
                        return Err(error.clone());
                    }
                    let page = page_of(&saved.saved.answer);
                    context.journal.record_page(&spec, &page);
                    if saved.reuse == Reuse::Window {
                        context.record_reused(saved.saved.observed_ms);
                    }
                    return Ok(Obtained {
                        page,
                        from_origin: false,
                        asked: false,
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
        let result = self
            .run_attempts(sent, context, &mut failed, attempt, followed)
            .await;
        // The origin refused the credential itself (a 401): nothing kept for
        // it may answer again, pinned answers included.
        if let (Err(error), Some(store), Some(_)) =
            (&result, &self.answers, context.resolved_credential())
            && error.kind == ProviderErrorKind::Authentication
            && let Some(scope) = self.answer_scope(context)
        {
            store.revoked(&scope);
        }
        let result = self.settle(result, saved, context);
        let page = result
            .as_ref()
            .map(|obtained| obtained.page.clone())
            .map_err(Clone::clone);
        context.journal.record(&spec, &page);
        match &result {
            Ok(obtained) if obtained.asked => {
                context.record_proven(&spec.url);
                if !obtained.page.redirected {
                    context.record_answered(&spec.url);
                }
            }
            Ok(_) => {}
            Err(error) => self.hold_missing(&spec, error, context),
        }
        self.keep_proved_missing(context);
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
                    if worth_keeping(&saved.saved.answer.headers, false, None) {
                        store.save(&saved.scope, &saved.request, saved.saved.clone());
                    } else {
                        store.forget(&saved.scope, &saved.request);
                    }
                }
                let mut answer = page_of(&saved.saved.answer);
                answer.redirected = page.redirected;
                Ok(Obtained {
                    page: answer,
                    from_origin: false,
                    asked: true,
                })
            }
            (Ok(page), _) => Ok(Obtained {
                page,
                from_origin: true,
                asked: true,
            }),
            // GitHub refused to answer now: the last answer it gave, disclosed
            // by the time it was observed, beats no answer.
            (Err(error), Some(saved))
                if error.kind == ProviderErrorKind::RateLimited && saved.saved.allows_stale() =>
            {
                context.record_reused(saved.saved.observed_ms);
                Ok(Obtained {
                    page: page_of(&saved.saved.answer),
                    from_origin: false,
                    asked: false,
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

    /// Load this credential's answer and reuse policy; oversized bodies require a new read.
    fn saved(&self, spec: &RequestSpec, context: &RequestContext) -> Option<Saved> {
        let store = self.answers.as_ref()?;
        if !matches!(spec.method, HttpMethod::Get)
            && !spec.reads_pull_request
            && !spec.reads_objects
        {
            return None;
        }
        let scope = self.answer_scope(context)?;
        let request = request_key(spec);
        let saved = store.load(&scope, &request)?;
        if saved.answer.body.len() > context.budget.max_body_bytes {
            return None;
        }
        let reuse = saved.reuse(spec.is_pinned(), self.reuse_window(spec, context), now_ms());
        Some(Saved {
            scope,
            request,
            saved,
            reuse,
        })
    }

    /// Whether `spec` would be answered with nothing sent: a resumed
    /// continuation's journal or a saved answer that needs no asking. The
    /// saved answer is probed, not loaded ([`super::SavedAnswers::probe`]): the
    /// question reads no body.
    pub(crate) fn answers_unasked(&self, spec: &RequestSpec, context: &RequestContext) -> bool {
        context.journal_answers(spec) || self.saved_unasked(spec, context)
    }

    /// Whether [`Self::saved`] would find a saved answer that needs no
    /// asking, from the store's probe of it.
    fn saved_unasked(&self, spec: &RequestSpec, context: &RequestContext) -> bool {
        let Some(store) = self.answers.as_ref() else {
            return false;
        };
        if !matches!(spec.method, HttpMethod::Get)
            && !spec.reads_pull_request
            && !spec.reads_objects
        {
            return false;
        }
        let Some(scope) = self.answer_scope(context) else {
            return false;
        };
        let Some((saved, length)) = store.probe(&scope, &request_key(spec)) else {
            return false;
        };
        if length.is_some_and(|length| length > context.budget.max_body_bytes) {
            return false;
        }
        matches!(
            saved.reuse(spec.is_pinned(), self.reuse_window(spec, context), now_ms()),
            Reuse::Pinned | Reuse::Fresh | Reuse::Window
        )
    }

    /// Whether this transport keeps answers at all: what makes a read
    /// another request proved worth seeding ([`Self::seed`]).
    pub(crate) fn keeps_answers(&self) -> bool {
        self.answers.is_some()
    }

    /// Keeps `page` as the answer to `spec`, which another request of this
    /// credential proved to be what GitHub answers it byte for byte (a
    /// GraphQL read of the same immutable object), as if `spec` had been
    /// sent; whether it now answers `spec` with nothing sent.
    pub(crate) fn seed(
        &self,
        spec: &RequestSpec,
        page: &ResponsePage,
        context: &RequestContext,
    ) -> bool {
        self.keep(spec, page, context);
        self.answers_unasked(spec, context)
    }

    /// Keep reusable origin answers. Partial or SAML-withheld answers are not cached.
    pub(super) fn keep(&self, spec: &RequestSpec, page: &ResponsePage, context: &RequestContext) {
        let Some(store) = &self.answers else {
            return;
        };
        let Some(scope) = self.answer_scope(context) else {
            return;
        };
        let request = request_key(spec);
        let cache_headers = page
            .headers
            .iter()
            .filter(|(name, _)| matches!(name.as_str(), "cache-control" | "vary"))
            .filter_map(|(name, value)| {
                Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned()))
            })
            .collect::<Vec<_>>();
        if super::saved::forbids_reuse(&cache_headers) {
            store.forget(&scope, &request);
            return;
        }
        if page.status != 200
            || is_partial(page, self.endpoint.resource(&spec.url))
            || spec.kept_by_caller
        {
            return;
        }
        let Some(answer) = answer_of(&request, page) else {
            return;
        };
        if worth_keeping(
            &answer.headers,
            spec.is_pinned(),
            self.reuse_window(spec, context),
        ) {
            store.save(
                &scope,
                &request,
                SavedAnswer {
                    answer,
                    observed_ms: now_ms(),
                    redirected: page.redirected,
                },
            );
        } else {
            store.forget(&scope, &request);
        }
    }

    /// Drop unreadable answers from the journal and cache, including renamed routes.
    /// A retry must acquire a new body.
    pub(crate) fn discard(&self, spec: &RequestSpec, context: &RequestContext) {
        let scope = self.answer_scope(context);
        let mut specs = vec![spec.clone()];
        specs.extend(context.routed_urls(&spec.url).into_iter().map(|url| {
            let mut routed = spec.clone();
            routed.url = url;
            routed
        }));
        for spec in &specs {
            context.journal.forget(spec);
            if let (Some(store), Some(scope)) = (&self.answers, &scope) {
                store.forget(scope, &request_key(spec));
            }
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

    /// How long an answer `spec` gets with no validator or lifetime may stand:
    /// a search's chosen window; for a read of a pull request, what GitHub
    /// states for that pull request over REST once this key has seen it, else
    /// [`PULL_REQUEST_REUSE_WINDOW`].
    fn reuse_window(&self, spec: &RequestSpec, context: &RequestContext) -> Option<Duration> {
        match self.endpoint.resource(&spec.url) {
            GitHubResource::Search | GitHubResource::CodeSearch => Some(SEARCH_REUSE_WINDOW),
            _ if spec.reads_pull_request => Some(
                self.key_state(context.resolved_credential())
                    .pull_request_lifetime()
                    .unwrap_or(PULL_REQUEST_REUSE_WINDOW),
            ),
            _ => None,
        }
    }

    /// Note the `max-age` GitHub stated for this REST read of a pull request:
    /// the window a read of one by a route with no validator gets. No stated
    /// lifetime is a fact too: it allows no window.
    fn learn_pull_request_lifetime(
        &self,
        spec: &RequestSpec,
        page: &ResponsePage,
        context: &RequestContext,
    ) {
        if page.status != 200 || !names_pull_request(&spec.url) {
            return;
        }
        let Some(answer) = answer_of(&request_key(spec), page) else {
            return;
        };
        self.key_state(context.resolved_credential())
            .learn_pull_request_max_age(
                super::saved::stated_max_age(&answer.headers).unwrap_or_default(),
            );
    }

    /// The attempts of one request; `failed` is the circuit-breaker cause of
    /// the last attempt, cleared whenever GitHub answers without one.
    async fn run_attempts(
        &self,
        spec: RequestSpec,
        context: &RequestContext,
        failed: &mut Option<CircuitCause>,
        attempt: &mut u8,
        followed: &mut Vec<(Url, Url)>,
    ) -> Result<ResponsePage, ProviderError> {
        if !self.endpoint.permits(&spec.url) {
            return Err(ProviderError::new(
                ProviderErrorKind::RedirectDenied,
                "request URL is outside the configured GitHub API origin",
            ));
        }
        Attempts::new(self, spec, context, failed, attempt, followed)
            .run()
            .await
    }
}
