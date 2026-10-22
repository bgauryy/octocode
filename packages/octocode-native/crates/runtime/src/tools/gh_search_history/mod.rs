//! GitHub history *search* — pull requests, issues, and commits.
//!
//! The transport revalidates mutable REST listings and caches complete indexed
//! search pages within its volatile TTL. Incomplete search pages are not cached.
mod errors;
mod filters;
mod leads;
mod query;
pub(crate) mod rows;
mod shape;
#[cfg(test)]
mod tests;

use crate::id::ToolId;
use crate::num::usize_of;
use crate::response::paging::clamp_page_size;
use crate::security::scan::ContentScan;
use crate::tools::gh_shared::field_names;
pub(crate) use errors::history_failure;
use filters::Filters;
pub use octocode_contract_types::GhSearchHistoryQuery;
use octocode_github::{
    CommitListRequest, GitHubTransport, HistoryPage, HistoryRequest, ProviderError,
    ProviderErrorKind, ProviderErrorReason, PullListRequest, RequestContext,
};
use serde_json::{Value, json};

/// The operation discriminant of a [`GhSearchHistoryQuery`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryOperation {
    PullRequest,
    Issue,
    Commit,
}

/// Operation-independent views over the generated wire query.
pub(crate) mod gh_search_history_query {
    use super::*;
    pub(crate) fn operation(query: &GhSearchHistoryQuery) -> HistoryOperation {
        match query {
            GhSearchHistoryQuery::PullRequest { .. } => HistoryOperation::PullRequest,
            GhSearchHistoryQuery::Issue { .. } => HistoryOperation::Issue,
            GhSearchHistoryQuery::Commit { .. } => HistoryOperation::Commit,
        }
    }
    pub(crate) fn owner(query: &GhSearchHistoryQuery) -> Option<&str> {
        match query {
            GhSearchHistoryQuery::PullRequest { owner, .. } => owner.as_deref().map(String::as_str),
            GhSearchHistoryQuery::Issue { owner, .. }
            | GhSearchHistoryQuery::Commit { owner, .. } => Some(owner.as_str()),
        }
    }
    pub(crate) fn repo(query: &GhSearchHistoryQuery) -> Option<&str> {
        match query {
            GhSearchHistoryQuery::PullRequest { repo, .. } => repo.as_deref().map(String::as_str),
            GhSearchHistoryQuery::Issue { repo, .. }
            | GhSearchHistoryQuery::Commit { repo, .. } => Some(repo.as_str()),
        }
    }
    pub(crate) fn keywords(query: &GhSearchHistoryQuery) -> Vec<&str> {
        match query {
            GhSearchHistoryQuery::PullRequest { keywords, .. }
            | GhSearchHistoryQuery::Issue { keywords, .. } => {
                keywords.iter().map(String::as_str).collect()
            }
            GhSearchHistoryQuery::Commit { keywords, .. } => {
                keywords.iter().map(|k| k.as_str()).collect()
            }
        }
    }
    pub(crate) fn qualifiers(query: &GhSearchHistoryQuery) -> Option<&str> {
        match query {
            GhSearchHistoryQuery::PullRequest { qualifiers, .. } => {
                qualifiers.as_deref().map(String::as_str)
            }
            GhSearchHistoryQuery::Issue { qualifiers, .. } => {
                qualifiers.as_deref().map(String::as_str)
            }
            GhSearchHistoryQuery::Commit { qualifiers, .. } => {
                qualifiers.as_deref().map(String::as_str)
            }
        }
    }
    /// The `fields` pick; empty keeps rows whole.
    pub(crate) fn fields(query: &GhSearchHistoryQuery) -> Vec<String> {
        match query {
            GhSearchHistoryQuery::PullRequest { fields, .. } => field_names(fields),
            GhSearchHistoryQuery::Issue { fields, .. } => field_names(fields),
            GhSearchHistoryQuery::Commit { fields, .. } => field_names(fields),
        }
    }
    pub(crate) fn page(query: &GhSearchHistoryQuery) -> Option<usize> {
        match query {
            GhSearchHistoryQuery::PullRequest { page, .. }
            | GhSearchHistoryQuery::Issue { page, .. }
            | GhSearchHistoryQuery::Commit { page, .. } => Some(usize_of(*page)),
        }
    }
    pub(crate) fn page_size(query: &GhSearchHistoryQuery) -> Option<usize> {
        match query {
            GhSearchHistoryQuery::PullRequest { page_size, .. }
            | GhSearchHistoryQuery::Issue { page_size, .. }
            | GhSearchHistoryQuery::Commit { page_size, .. } => {
                page_size.as_ref().map(|size| usize_of(size.0))
            }
        }
    }
    pub(crate) fn path(query: &GhSearchHistoryQuery) -> Option<&str> {
        match query {
            GhSearchHistoryQuery::Commit { path, .. } => path.as_ref().map(|value| value.as_str()),
            _ => None,
        }
    }
    pub(crate) fn since(query: &GhSearchHistoryQuery) -> Option<&str> {
        match query {
            GhSearchHistoryQuery::PullRequest { since, .. }
            | GhSearchHistoryQuery::Issue { since, .. } => {
                since.as_ref().map(|value| value.as_str())
            }
            GhSearchHistoryQuery::Commit { since, .. } => {
                since.as_ref().map(|value| value.as_str())
            }
        }
    }
    pub(crate) fn until(query: &GhSearchHistoryQuery) -> Option<&str> {
        match query {
            GhSearchHistoryQuery::PullRequest { until, .. }
            | GhSearchHistoryQuery::Issue { until, .. } => {
                until.as_ref().map(|value| value.as_str())
            }
            GhSearchHistoryQuery::Commit { until, .. } => {
                until.as_ref().map(|value| value.as_str())
            }
        }
    }
    /// The ref a commit listing walks.
    pub(crate) fn reference(query: &GhSearchHistoryQuery) -> Option<&str> {
        match query {
            GhSearchHistoryQuery::Commit { ref_, .. } => ref_.as_ref().map(|value| value.as_str()),
            _ => None,
        }
    }
    /// The wire `sort` is `field` or `field:direction`; this is the field.
    pub(crate) fn sort(query: &GhSearchHistoryQuery) -> Option<String> {
        sort_spec(query).map(|spec| {
            spec.split_once(':')
                .map_or(spec.as_str(), |(f, _)| f)
                .to_owned()
        })
    }
    /// The direction half of the wire `sort`, when it names one.
    pub(crate) fn order(query: &GhSearchHistoryQuery) -> Option<String> {
        sort_spec(query).and_then(|spec| spec.split_once(':').map(|(_, order)| order.to_owned()))
    }
    fn sort_spec(query: &GhSearchHistoryQuery) -> Option<String> {
        match query {
            GhSearchHistoryQuery::PullRequest { sort, .. }
            | GhSearchHistoryQuery::Issue { sort, .. } => sort.as_ref().map(ToString::to_string),
            GhSearchHistoryQuery::Commit { .. } => None,
        }
    }
}

/// A history search: the wire query plus the GitHub filters its
/// `qualifiers` string sets, parsed once. The wire query keeps `qualifiers`
/// as sent, so a continuation replays the same filters.
#[derive(Clone, Debug)]
pub struct HistorySearch {
    pub query: GhSearchHistoryQuery,
    filters: Filters,
}

impl std::ops::Deref for HistorySearch {
    type Target = GhSearchHistoryQuery;
    fn deref(&self) -> &GhSearchHistoryQuery {
        &self.query
    }
}

impl HistorySearch {
    /// Parses the query's `qualifiers` into filters.
    pub fn new(query: GhSearchHistoryQuery) -> Result<Self, ProviderError> {
        let filters = Filters::parse(&query)?;
        Ok(Self { query, filters })
    }
    fn filters(&self) -> &Filters {
        &self.filters
    }
    pub(crate) fn author(&self) -> Option<&str> {
        self.filters.author.as_deref()
    }
    /// The commit committer filter (`committer:`).
    pub(crate) fn committer(&self) -> Option<&str> {
        self.filters.committer.as_deref()
    }
    /// The PR source branch filter (`head:`).
    pub(crate) fn head(&self) -> Option<&str> {
        self.filters.head.as_deref()
    }
    /// The PR target branch filter (`base:`).
    pub(crate) fn base(&self) -> Option<&str> {
        self.filters.base.as_deref()
    }
    pub(crate) fn state(&self) -> Option<String> {
        match &self.query {
            GhSearchHistoryQuery::PullRequest { state, .. }
            | GhSearchHistoryQuery::Issue { state, .. } => state.as_ref().map(ToString::to_string),
            GhSearchHistoryQuery::Commit { .. } => None,
        }
        .or_else(|| self.filters.state.clone())
    }
    /// Labels set by `label:` qualifiers; all must match.
    pub(crate) fn label(&self) -> &[String] {
        &self.filters.label
    }
}

/// A failed history search: the provider error and, when one call repairs
/// it, that lead (keyed by name).
#[derive(Debug)]
pub struct HistoryFailure {
    /// Boxed, as in `GhFailure`: it is most of the failure's size.
    pub error: Box<ProviderError>,
    pub next: Option<Value>,
}

impl From<ProviderError> for HistoryFailure {
    fn from(error: ProviderError) -> Self {
        Self {
            error: Box::new(error),
            next: None,
        }
    }
}

/// Runs one history search: GitHub search for keywords and search-only
/// filters, else the REST list endpoint.
pub async fn execute(
    provider: &octocode_github::GitHubProvider,
    query: GhSearchHistoryQuery,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<Value, HistoryFailure> {
    let transport = &provider.transport;
    let mut query = HistorySearch::new(query)?;
    crate::tools::gh_shared::reject_boolean_keywords(&gh_search_history_query::keywords(&query))?;
    let page = gh_search_history_query::page(&query).unwrap_or(1);
    let per = page_size(&query);
    let searching = query::uses_search(&query);
    if searching
        && let Err(error) = crate::tools::gh_shared::SearchWindow::PageStart.reject(page, per)
    {
        return Err(window_failure(error, &query, per));
    }
    let mut unconfirmed = Vec::new();
    if searching && gh_search_history_query::operation(&query) == HistoryOperation::Commit {
        unconfirmed = confirm_commit_users(transport, &query, context).await?;
    }
    let mut fetched = fetch(transport, &query, page, per, searching, context).await;
    let mut rename_warnings = Vec::new();
    // GitHub search does not follow renames, so a renamed repository answers
    // with no rows or a 422; only then is the canonical name looked up.
    if searching
        && crate::tools::gh_shared::search_may_be_renamed(&fetched, |fetched| {
            fetched.result.items.is_empty() && fetched.result.total_count == 0
        })
        && let Some(warnings) = follow_rename(provider, &mut query, context).await?
    {
        rename_warnings = warnings;
        fetched = fetch(transport, &query, page, per, searching, context).await;
    }
    let Fetched {
        mut result,
        terms,
        warnings,
    } = match fetched {
        Ok(fetched) => fetched,
        Err(error) => {
            let failure = missing_scope_failure(error, &query);
            return Err(at_default_branch(provider, failure, &query, context).await);
        }
    };
    // A REST list follows a rename redirect itself: the rows are the
    // renamed repository's, so its leads and pages name the canonical one.
    let redirected = !searching
        && gh_search_history_query::owner(&query)
            .zip(gh_search_history_query::repo(&query))
            .is_some_and(|(owner, repo)| transport.followed_rename(owner, repo));
    if redirected && let Some(warnings) = follow_rename(provider, &mut query, context).await? {
        rename_warnings = warnings;
    }
    // A commit list answers an unknown author with no rows; only an empty
    // list pays for confirming the login.
    if !searching
        && gh_search_history_query::operation(&query) == HistoryOperation::Commit
        && result.items.is_empty()
    {
        unconfirmed = confirm_commit_users(transport, &query, context).await?;
    }
    result.warnings.splice(0..0, unconfirmed);
    result.warnings.extend(warnings);
    rows::sanitize_items(&mut result.items, security)?;
    let paging = shape::Paging::new(&result, page, per);
    let mut shaped = shape::shape(&query, result, &paging, terms);
    // A rename reads the same on every row and page of the repository: a
    // static notice, said once per response.
    for notice in rename_warnings {
        crate::response::paging::push_notice(&mut shaped, notice);
    }
    Ok(shaped)
}

/// Confirms the logins a commit search or an empty commit list names
/// (`author:`, `committer:`; an email has no profile to look up), each once.
/// GitHub's commit search drops an unknown login and answers the unfiltered
/// total, and the commit list answers it with no rows, where issue and PR
/// search answer a 422; a missing login is that same typed failure. A
/// lookup that fails otherwise never blocks the answer: it is a warning.
async fn confirm_commit_users(
    transport: &GitHubTransport,
    query: &HistorySearch,
    context: &RequestContext,
) -> Result<Vec<String>, HistoryFailure> {
    let mut logins: Vec<&str> = Vec::new();
    for login in [query.author(), query.committer()].into_iter().flatten() {
        // Apps (`app/name`) have no `/users` route either.
        if !login.contains(['@', '/']) && !logins.iter().any(|l| l.eq_ignore_ascii_case(login)) {
            logins.push(login);
        }
    }
    let lookups = logins
        .iter()
        .map(|login| transport.confirm_user(login, context));
    let mut warnings = Vec::new();
    for (login, outcome) in logins
        .iter()
        .zip(futures_util::future::join_all(lookups).await)
    {
        match outcome {
            Ok(()) => {}
            Err(error) if error.reason == Some(ProviderErrorReason::UserNotFound) => {
                return Err(missing_scope_failure(error, query));
            }
            Err(error) if error.kind == ProviderErrorKind::Cancelled => return Err(error.into()),
            Err(error) => warnings.push(format!(
                "Could not confirm that user {login} exists ({}); GitHub does not report an unknown login for commits.",
                error.message
            )),
        }
    }
    Ok(warnings)
}

/// The page size: the stamped contract default, else the schema default,
/// within the schema maximum.
fn page_size(query: &HistorySearch) -> usize {
    use crate::contracts::query_limits::gh_search_history::PAGE_SIZE_DEFAULT;
    clamp_page_size(
        ToolId::GhSearchHistory,
        None,
        gh_search_history_query::page_size(query).unwrap_or(PAGE_SIZE_DEFAULT),
    )
}

/// One provider page and the search terms it ran.
struct Fetched {
    result: HistoryPage,
    terms: String,
    warnings: Vec<String>,
}

async fn fetch(
    transport: &GitHubTransport,
    query: &HistorySearch,
    page: usize,
    per: usize,
    searching: bool,
    context: &RequestContext,
) -> Result<Fetched, ProviderError> {
    let (terms, warnings) = query::build_query_with_warnings(query)?;
    let (sort, order) = query::search_order(query);
    let request = HistoryRequest {
        query: terms,
        page,
        per_page: per,
        sort,
        order,
    };
    let result = match gh_search_history_query::operation(query) {
        HistoryOperation::Commit if searching => {
            transport.search_commits(&request, context).await?
        }
        HistoryOperation::Commit => list_commits(transport, query, page, per, context).await?,
        HistoryOperation::PullRequest if !searching => {
            list_pull_requests(transport, query, page, per, context).await?
        }
        _ => transport.search_issues(&request, context).await?,
    };
    Ok(Fetched {
        result,
        terms: request.query,
        warnings,
    })
}

async fn list_commits(
    transport: &GitHubTransport,
    query: &HistorySearch,
    page: usize,
    per: usize,
    context: &RequestContext,
) -> Result<HistoryPage, ProviderError> {
    let (o, r) = query::required_repo(query)?;
    // Invalid-value warnings were already collected by the terms.
    let (since, until) = query::resolve_commit_window(query, &mut Vec::new())?;
    transport
        .list_commits_by_committer(
            &CommitListRequest {
                owner: o.into(),
                repo: r.into(),
                branch: gh_search_history_query::reference(query).map(str::to_owned),
                path: gh_search_history_query::path(query).map(str::to_owned),
                author: query.author().map(str::to_owned),
                since,
                until: until.map(|until| query::inclusive_until(&until)),
                page,
                per_page: per,
            },
            query.committer(),
            context,
        )
        .await
}

async fn list_pull_requests(
    transport: &GitHubTransport,
    query: &HistorySearch,
    page: usize,
    per: usize,
    context: &RequestContext,
) -> Result<HistoryPage, ProviderError> {
    let (o, r) = query::required_repo(query)?;
    transport
        .list_pull_requests(
            &PullListRequest {
                owner: o.into(),
                repo: r.into(),
                state: query.state(),
                head: query.head().map(str::to_owned),
                base: query.base().map(str::to_owned),
                sort: gh_search_history_query::sort(query),
                order: gh_search_history_query::order(query),
                page,
                per_page: per,
            },
            context,
        )
        .await
}

/// Points the query at the repository's canonical name when it was renamed
/// ([`crate::tools::gh_shared::rescope_on_rename`]), returning the rename
/// warnings; `None` when the name stands. A missing repository keeps the
/// original answer, which states it.
async fn follow_rename(
    provider: &octocode_github::GitHubProvider,
    query: &mut HistorySearch,
    context: &RequestContext,
) -> Result<Option<Vec<String>>, ProviderError> {
    use crate::tools::gh_shared::Rescope;
    let Some((owner, repo)) =
        gh_search_history_query::owner(query).zip(gh_search_history_query::repo(query))
    else {
        return Ok(None);
    };
    Ok(
        match crate::tools::gh_shared::rescope_on_rename(provider, owner, repo, context).await? {
            Rescope::Renamed {
                owner,
                repo,
                warning,
            } => {
                // A name the contract would reject leaves the query as it is.
                if let Ok(renamed) = crate::tools::gh_shared::with_repo(&query.query, &owner, &repo)
                {
                    query.query = renamed;
                }
                Some(vec![warning])
            }
            Rescope::Missing(_) | Rescope::Stands => None,
        },
    )
}

/// A page past GitHub's 1,000-result search window, with its one repair:
/// the last reachable page (`repair`).
fn window_failure(error: ProviderError, query: &HistorySearch, per: usize) -> HistoryFailure {
    let failure = crate::tools::gh_shared::with_search_repair(
        crate::tools::gh_shared::GhFailure::new(error, String::new()),
        crate::id::ToolId::GhSearchHistory,
        &query.query,
        per,
        None,
    );
    HistoryFailure {
        error: failure.error,
        next: failure.next.map(|next| *next),
    }
}

/// A bare GitHub 404 of a listing, or a search scoped to a missing
/// repository. A commit listing at a `ref` cannot tell a missing ref from a
/// missing repository: the message names the ref and the refs listing is the
/// one lead (it fails on a missing repository). Otherwise the repository
/// itself did not resolve: the message names it and a search for its name is
/// the lead.
fn missing_scope_failure(mut error: ProviderError, query: &HistorySearch) -> HistoryFailure {
    if error.reason == Some(ProviderErrorReason::UserNotFound) {
        let next = leads::without_users(query).map(|lead| json!({ "withoutUsers": lead }));
        return HistoryFailure {
            error: Box::new(error),
            next,
        };
    }
    // A search scoped to a missing repository fails typed (GitHub's 422);
    // a listing's 404 does not say which of repository and ref is missing.
    let repository_missing = crate::tools::gh_shared::repository_not_found(&error);
    if error.kind != ProviderErrorKind::NotFound || (error.reason.is_some() && !repository_missing)
    {
        return error.into();
    }
    let (Some(owner), Some(repo)) = (
        gh_search_history_query::owner(query),
        gh_search_history_query::repo(query),
    ) else {
        error.message = crate::tools::gh_shared::provider_message(&error).into_boxed_str();
        return error.into();
    };
    let reference = gh_search_history_query::reference(query).filter(|_| !repository_missing);
    let Some(reference) = reference else {
        // The repository itself did not resolve: the message names it, and
        // its recovery is the owner/repo spelling, not a history filter.
        error.message = format!(
            "Repository {owner}/{repo} not found, or private and not accessible to this token"
        )
        .into_boxed_str();
        return HistoryFailure {
            error: Box::new(error),
            next: Some(json!({
                "findRepository": crate::tools::gh_shared::repository_search(repo)
            })),
        };
    };
    error.message =
        format!("Ref \"{reference}\" or repository {owner}/{repo} not found").into_boxed_str();
    HistoryFailure {
        error: Box::new(error),
        next: Some(
            serde_json::json!({"viewRefs": crate::tools::gh_shared::ref_recovery(owner, repo)}),
        ),
    }
}

/// A commit listing at a `ref` that did not resolve ([`missing_scope_failure`]
/// leads with `viewRefs`): when the repository itself resolves, the failure
/// names its default branch and offers the same listing at it.
async fn at_default_branch(
    provider: &octocode_github::GitHubProvider,
    mut failure: HistoryFailure,
    query: &HistorySearch,
    context: &RequestContext,
) -> HistoryFailure {
    let (Some(owner), Some(repo), Some(reference)) = (
        gh_search_history_query::owner(query),
        gh_search_history_query::repo(query),
        gh_search_history_query::reference(query),
    ) else {
        return failure;
    };
    let Some(next) = failure
        .next
        .as_mut()
        .filter(|next| next.get("viewRefs").is_some())
    else {
        return failure;
    };
    let row = crate::response::result::query_row(&query.query, &[]);
    if let Some(retry) = crate::tools::gh_shared::default_branch_retry(
        provider,
        crate::id::ToolId::GhSearchHistory,
        row,
        &["ref"],
        context,
    )
    .await
    {
        // The repository resolved, so only the ref is missing.
        failure.error.message = retry
            .message(&format!("Ref \"{reference}\" not found in {owner}/{repo}"))
            .into_boxed_str();
        let (name, lead) = retry.lead();
        next[name] = lead;
    }
    failure
}

/// Output facts the shared response stages ask about.
pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {
    fn fallback_hint(&self, query: &Value) -> &'static str {
        if crate::response::output::has_value(query, "keywords") {
            "Broaden keywords or remove history filters."
        } else {
            "Relax or remove a history filter, or add keywords."
        }
    }
    fn evidence_kind(&self, _query: &Value, _data: &Value) -> &'static str {
        "provider"
    }
}
