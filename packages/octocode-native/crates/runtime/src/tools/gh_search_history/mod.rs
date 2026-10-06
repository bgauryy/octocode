//! GitHub history *search* — pull requests, issues, and commits.
//!
//! **Cache bypass is intentional.**  This tool uses `&provider.transport`
//! directly rather than the `GitHubProvider<_, GitHubContentCache>` wrapper, so
//! none of the `ConditionalCache` ETag / disk-tier machinery applies.  History
//! search results are inherently mutable (new PRs/issues appear, existing ones
//! are updated, merged, or closed), so caching them would serve stale state;
//! the GitHub API's own rate-limit budget is the right throttle here.
mod errors;
mod filters;
mod leads;
mod query;
mod rows;
mod shape;
#[cfg(test)]
mod tests;

pub use crate::contracts::tool_types::{
    GhSearchHistoryQuery, GhSearchHistoryQueryOwner, GhSearchHistoryQueryRepo,
};
use crate::providers::github::{
    CommitListRequest, CredentialResolver, GitHubTransport, HistoryPage, HistoryRequest,
    ProviderError, ProviderErrorKind, ProviderErrorReason, PullListRequest, RequestContext,
};
use crate::security::scan::ContentScan;
use crate::tools::num::usize_of;
pub(crate) use errors::history_failure;
use filters::Filters;
use serde_json::Value;

/// The operation discriminant of a [`GhSearchHistoryQuery`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryOperation {
    PullRequest,
    Issue,
    Commit,
}

/// Operation-independent views over the generated wire query.
impl GhSearchHistoryQuery {
    pub fn operation(&self) -> HistoryOperation {
        match self {
            Self::PullRequest { .. } => HistoryOperation::PullRequest,
            Self::Issue { .. } => HistoryOperation::Issue,
            Self::Commit { .. } => HistoryOperation::Commit,
        }
    }
    pub fn owner(&self) -> Option<&str> {
        match self {
            Self::PullRequest { owner, .. } => owner.as_deref().map(String::as_str),
            Self::Issue { owner, .. } | Self::Commit { owner, .. } => Some(owner.as_str()),
        }
    }
    pub fn repo(&self) -> Option<&str> {
        match self {
            Self::PullRequest { repo, .. } => repo.as_deref().map(String::as_str),
            Self::Issue { repo, .. } | Self::Commit { repo, .. } => Some(repo.as_str()),
        }
    }
    /// Points the query at a repository's canonical (post-rename) name.
    /// Names the contract would reject leave the query unchanged.
    pub fn set_scope(&mut self, new_owner: &str, new_repo: &str) {
        let (Ok(new_owner), Ok(new_repo)) = (
            new_owner.parse::<GhSearchHistoryQueryOwner>(),
            new_repo.parse::<GhSearchHistoryQueryRepo>(),
        ) else {
            return;
        };
        match self {
            Self::PullRequest { owner, repo, .. } => {
                *owner = Some(new_owner);
                *repo = Some(new_repo);
            }
            Self::Issue { owner, repo, .. } | Self::Commit { owner, repo, .. } => {
                *owner = new_owner;
                *repo = new_repo;
            }
        }
    }
    pub fn keywords(&self) -> Vec<&str> {
        match self {
            Self::PullRequest { keywords, .. } | Self::Issue { keywords, .. } => {
                keywords.iter().map(String::as_str).collect()
            }
            Self::Commit { keywords, .. } => keywords.iter().map(|k| k.as_str()).collect(),
        }
    }
    pub fn qualifiers(&self) -> Option<&str> {
        match self {
            Self::PullRequest { qualifiers, .. } | Self::Issue { qualifiers, .. } => {
                qualifiers.as_deref().map(String::as_str)
            }
            Self::Commit { .. } => None,
        }
    }
    pub fn concise(&self) -> Option<bool> {
        match self {
            Self::PullRequest { concise, .. } | Self::Issue { concise, .. } => *concise,
            Self::Commit { .. } => None,
        }
    }
    pub fn page(&self) -> Option<usize> {
        match self {
            Self::PullRequest { page, .. }
            | Self::Issue { page, .. }
            | Self::Commit { page, .. } => Some(usize_of(*page)),
        }
    }
    pub fn page_size(&self) -> Option<usize> {
        match self {
            Self::PullRequest { page_size, .. }
            | Self::Issue { page_size, .. }
            | Self::Commit { page_size, .. } => page_size.as_ref().map(|size| usize_of(size.0)),
        }
    }
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Commit { path, .. } => path.as_ref().map(|value| value.as_str()),
            _ => None,
        }
    }
    pub fn since(&self) -> Option<&str> {
        match self {
            Self::Commit { since, .. } => since.as_deref(),
            _ => None,
        }
    }
    pub fn until(&self) -> Option<&str> {
        match self {
            Self::Commit { until, .. } => until.as_deref(),
            _ => None,
        }
    }
    /// The ref a commit listing walks.
    pub fn reference(&self) -> Option<&str> {
        match self {
            Self::Commit { ref_, .. } => ref_.as_deref(),
            _ => None,
        }
    }
    pub fn committer(&self) -> Option<&str> {
        match self {
            Self::Commit { committer, .. } => committer.as_deref(),
            _ => None,
        }
    }
    pub fn sort(&self) -> Option<String> {
        match self {
            Self::PullRequest { sort, .. } | Self::Issue { sort, .. } => {
                sort.as_ref().map(ToString::to_string)
            }
            Self::Commit { .. } => None,
        }
    }
    pub fn order(&self) -> Option<String> {
        match self {
            Self::PullRequest { order, .. } | Self::Issue { order, .. } => {
                order.as_ref().map(ToString::to_string)
            }
            Self::Commit { .. } => None,
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
    pub fn author(&self) -> Option<&str> {
        match &self.query {
            GhSearchHistoryQuery::Commit { author, .. } => author.as_deref(),
            _ => self.filters.author.as_deref(),
        }
    }
    /// The PR source branch filter (`head:`).
    pub fn head(&self) -> Option<&str> {
        self.filters.head.as_deref()
    }
    /// The PR target branch filter (`base:`).
    pub fn base(&self) -> Option<&str> {
        self.filters.base.as_deref()
    }
    pub fn state(&self) -> Option<String> {
        match &self.query {
            GhSearchHistoryQuery::PullRequest { state, .. }
            | GhSearchHistoryQuery::Issue { state, .. } => state.as_ref().map(ToString::to_string),
            GhSearchHistoryQuery::Commit { .. } => None,
        }
        .or_else(|| self.filters.state.clone())
    }
    /// Labels set by `label:` qualifiers; all must match.
    pub fn label(&self) -> &[String] {
        &self.filters.label
    }
}

/// Runs one history search: GitHub search for keywords and search-only
/// filters, else the REST list endpoint.
pub async fn execute<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: GhSearchHistoryQuery,
    context: &RequestContext,
    security: &impl ContentScan,
) -> Result<Value, ProviderError> {
    let mut query = HistorySearch::new(query)?;
    let page = query.page().unwrap_or(1);
    let per = page_size(&query);
    let searching = query::uses_search(&query);
    if searching {
        crate::tools::gh_shared::reject_window(page, per)?;
    }
    let mut fetched = fetch(transport, &query, page, per, searching, context).await;
    let mut rename_warnings = Vec::new();
    // GitHub search does not follow renames, so a renamed repository answers
    // with no rows or a 422; only then is the canonical name looked up.
    if searching
        && may_be_renamed(&fetched)
        && let Some(warnings) = follow_rename(transport, &mut query, context).await?
    {
        rename_warnings = warnings;
        fetched = fetch(transport, &query, page, per, searching, context).await;
    }
    let Fetched {
        mut result,
        terms,
        warnings,
    } = fetched.map_err(unsearchable_repository)?;
    result.warnings.splice(0..0, rename_warnings);
    result.warnings.extend(warnings);
    rows::sanitize_items(&mut result.items, security)?;
    let paging = shape::Paging::new(&result, page, per);
    Ok(shape::shape(&query, result, &paging, terms))
}

/// The page size: the stamped contract default, else the schema default,
/// within the schema maximum.
fn page_size(query: &HistorySearch) -> usize {
    use crate::tools::id::query_limits::gh_search_history::{PAGE_SIZE_DEFAULT, PAGE_SIZE_MAXIMUM};
    query
        .page_size()
        .unwrap_or(PAGE_SIZE_DEFAULT)
        .min(PAGE_SIZE_MAXIMUM)
}

/// One provider page and the search terms it ran.
struct Fetched {
    result: HistoryPage,
    terms: String,
    warnings: Vec<String>,
}

async fn fetch<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistorySearch,
    page: usize,
    per: usize,
    searching: bool,
    context: &RequestContext,
) -> Result<Fetched, ProviderError> {
    let (terms, warnings) = query::build_query_with_warnings(query)?;
    let commit_search = searching && query.operation() == HistoryOperation::Commit;
    let newest_first = query::lists_issues_newest_first(query);
    let request = HistoryRequest {
        query: terms,
        page,
        per_page: per,
        sort: if commit_search {
            Some("committer-date".into())
        } else if newest_first {
            Some("created".into())
        } else {
            query.sort().filter(|v| v != "best-match")
        },
        order: if commit_search {
            Some("desc".into())
        } else if newest_first {
            Some(query.order().unwrap_or_else(|| "desc".into()))
        } else {
            query.order()
        },
    };
    let result = match query.operation() {
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

async fn list_commits<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &HistorySearch,
    page: usize,
    per: usize,
    context: &RequestContext,
) -> Result<HistoryPage, ProviderError> {
    let (o, r) = query::required_repo(query)?;
    // Invalid-value warnings were already collected by the terms.
    let (since, until) = query::resolve_commit_window(query, &mut Vec::new())?;
    let mut listed = transport
        .list_commits_by_committer(
            &CommitListRequest {
                owner: o.into(),
                repo: r.into(),
                branch: query.reference().map(str::to_owned),
                path: query.path().map(str::to_owned),
                author: query.author().map(str::to_owned),
                since,
                until,
                page,
                per_page: per,
            },
            query.committer(),
            context,
        )
        .await?;
    if listed.items.is_empty() && (query.since().is_some() || query.until().is_some()) {
        listed.warnings.push(
            "since/until matched no commits (GitHub commit listing uses committer date and does not follow renames).".into(),
        );
    }
    Ok(listed)
}

async fn list_pull_requests<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
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
                sort: query.sort(),
                order: query.order(),
                page,
                per_page: per,
            },
            context,
        )
        .await
}

/// A scoped search a rename could explain: no rows, or GitHub's 422
/// "cannot be searched".
fn may_be_renamed(fetched: &Result<Fetched, ProviderError>) -> bool {
    match fetched {
        Ok(fetched) => fetched.result.items.is_empty(),
        Err(error) => unsearchable(error),
    }
}

/// Points the query at the repository's canonical name when it was renamed,
/// returning the rename warnings; `None` when the name stands. A failed
/// lookup keeps the original answer.
async fn follow_rename<R: CredentialResolver>(
    transport: &GitHubTransport<R>,
    query: &mut HistorySearch,
    context: &RequestContext,
) -> Result<Option<Vec<String>>, ProviderError> {
    let Some((owner, repo)) = query.owner().zip(query.repo()) else {
        return Ok(None);
    };
    match transport.canonical_owner_repo(owner, repo, context).await {
        Ok((canonical_owner, canonical_repo, true, warnings)) => {
            query.query.set_scope(&canonical_owner, &canonical_repo);
            Ok(Some(warnings))
        }
        Err(error)
            if matches!(
                error.kind,
                ProviderErrorKind::Cancelled | ProviderErrorKind::Timeout
            ) =>
        {
            Err(error)
        }
        Ok(_) | Err(_) => Ok(None),
    }
}

fn unsearchable(error: &ProviderError) -> bool {
    error.kind == ProviderErrorKind::Validation && error.message.contains("cannot be searched")
}

/// Search over a missing or invisible repository answers 422 ("cannot be
/// searched"): it is the repository that was not found, not a bad query.
fn unsearchable_repository(error: ProviderError) -> ProviderError {
    if unsearchable(&error) {
        let mut not_found = ProviderError::new(ProviderErrorKind::NotFound, error.message.clone());
        not_found.status = error.status;
        not_found.with_reason(ProviderErrorReason::RepositoryNotFound)
    } else {
        error
    }
}

/// Output facts the shared response stages ask about.
pub(crate) struct Output;
impl crate::tools::output::ToolOutput for Output {
    fn fallback_hint(&self, _query: &Value) -> &'static str {
        "Broaden keywords or remove history filters."
    }
    fn evidence_kind(&self, _query: &Value, _data: &Value) -> &'static str {
        "provider"
    }
    fn clasify_items(
        &self,
        source: &crate::tools::clasify::resource::ResourceSource,
        state: &Value,
    ) -> Option<Vec<crate::tools::clasify::items::Item>> {
        let crate::tools::clasify::resource::ResourceSource::GhSearchHistory(query) = source else {
            return None;
        };
        clasify_history(state, query)
    }
}

/// One clasify candidate per pull request, issue, or commit, read through
/// ghGetHistoryItem when its repository is known.
fn clasify_history(
    state: &Value,
    query: &GhSearchHistoryQuery,
) -> Option<Vec<crate::tools::clasify::items::Item>> {
    use crate::tools::clasify::items;
    let data = items::page_data(state)?;
    let (key, operation) = [
        ("pullRequests", "pullRequest"),
        ("issues", "issue"),
        ("commits", "commit"),
    ]
    .into_iter()
    .find(|(key, _)| data.get(*key).is_some_and(Value::is_array))?;
    let pointer = format!("/results/0/data/{key}");
    let found = data[key]
        .as_array()?
        .iter()
        .map(|row| {
            let located = clasify_item_repository(row, query);
            let mut fetch = serde_json::Map::new();
            fetch.insert("operation".into(), serde_json::json!(operation));
            let identity = match (operation, &located) {
                ("commit", Some((owner, repo))) => {
                    row.get("sha").and_then(Value::as_str).map(|sha| {
                        fetch.insert("ref".into(), serde_json::json!(sha));
                        format!(
                            "{owner}/{repo}@{}",
                            sha.chars().take(12).collect::<String>()
                        )
                    })
                }
                (_, Some((owner, repo))) => {
                    row.get("number").and_then(Value::as_u64).map(|number| {
                        fetch.insert("number".into(), serde_json::json!(number));
                        format!("{owner}/{repo}#{number}")
                    })
                }
                _ => None,
            };
            let read = identity.as_ref().and(located).map(|(owner, repo)| {
                fetch.insert("owner".into(), serde_json::json!(owner));
                fetch.insert("repo".into(), serde_json::json!(repo));
                items::read(crate::tools::id::ToolId::GhGetHistoryItem, fetch)
            });
            items::Item {
                state: items::narrowed(state, &pointer, vec![row.clone()]),
                read,
                path: None,
                item: identity,
            }
        })
        .collect();
    Some(found)
}

/// Owner and repository of a history item: its own fields when a search spans
/// repositories, else the searched repository.
fn clasify_item_repository(row: &Value, query: &GhSearchHistoryQuery) -> Option<(String, String)> {
    if let Some(full) = row.get("repository").and_then(|repository| {
        repository
            .as_str()
            .or_else(|| repository.get("fullName")?.as_str())
    }) && let Some((owner, repo)) = full.split_once('/')
    {
        return Some((owner.to_owned(), repo.to_owned()));
    }
    let owner = row
        .get("owner")
        .and_then(Value::as_str)
        .or_else(|| query.owner())?;
    let repo = row
        .get("repo")
        .and_then(Value::as_str)
        .or_else(|| query.repo())?;
    Some((owner.to_owned(), repo.to_owned()))
}
