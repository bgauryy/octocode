//! GitHub history *search* — pull requests, issues, and commits.
//!
//! The transport revalidates mutable REST listings and caches complete indexed
//! search pages within its volatile TTL. Incomplete search pages are not cached.
mod failures;
mod fetch;
mod filters;
mod leads;
mod query;
pub(crate) mod rows;
mod shape;
#[cfg(test)]
mod tests;

use crate::security::scan::ContentScan;
use failures::{at_default_branch, follow_rename, missing_scope_failure, window_failure};
use fetch::{Fetched, fetch};
use filters::Filters;
use octocode_contract_types::GhSearchHistoryQuery;
use octocode_github::{
    GitHubTransport, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use serde_json::Value;

/// The operation discriminant of a [`GhSearchHistoryQuery`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryOperation {
    PullRequest,
    Issue,
    Commit,
}

/// Operation-independent views over the generated wire query.
pub(crate) mod gh_search_history_query;

/// A history search with provider predicates derived once from its typed
/// filters. The generated query retains those filters for saved continuations.
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
    /// Derives provider predicates from the generated typed filters.
    pub fn new(query: GhSearchHistoryQuery) -> Result<Self, ProviderError> {
        let filters = Filters::of(&query);
        let search = Self { query, filters };
        query::validate_history_scope(&search)?;
        Ok(search)
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
        self.filters.state.clone()
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
    pub calls: Option<Value>,
}

impl From<ProviderError> for HistoryFailure {
    fn from(error: ProviderError) -> Self {
        Self {
            error: Box::new(error),
            calls: None,
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
    crate::tools::gh_shared::finish_calls(&mut shaped);
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
fn page_size(_query: &HistorySearch) -> usize {
    crate::id::gh_search_history_policy::PAGE_SIZE
}

/// Output facts the shared response stages ask about.
pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {}
