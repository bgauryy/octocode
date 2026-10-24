//! ghSearchRepo: repository search, or an owner-only repository listing.
mod access;
mod language;
mod leads;
mod owner_listing;
mod page;
#[cfg(test)]
mod provider_tests;
mod query;
mod rows;
#[cfg(test)]
mod tests;

use crate::id::ToolId;
use crate::response::result::{Continuation, ToolData};
use crate::tools::gh_shared::{
    GhFailure, SearchWindow, add_next, pick_rows, rerun_renamed, search_failure,
    search_may_be_renamed, with_search_repair,
};
use octocode_contract_types::{ErrorCode, GhSearchRepoQuery, GitHubArchive};
use octocode_github::{
    GitHubProvider, ProviderError, RepositorySearchPage, RepositorySearchRequest, RequestContext,
};
use serde_json::json;

use language::ignored_language;
use leads::{include_archived, repository_output, top_leads};
use owner_listing::{OwnerListing, owner_listing};
use page::RepoPage;
use rows::{repository_row, wanted_topics};

/// Run one ghSearchRepo row.
pub async fn run(
    provider: &GitHubProvider,
    query: &GhSearchRepoQuery,
    request: Result<&RequestContext, ProviderError>,
) -> Result<ToolData, GhFailure> {
    let result = match request {
        Ok(context) => execute(provider, query, context).await,
        Err(error) => Err(error),
    };
    result.map_err(|error| {
        // An owner listing's bare 404 is the login itself.
        let missing_owner = owner_only(query).filter(|_| {
            error.kind == octocode_github::ProviderErrorKind::NotFound && error.reason.is_none()
        });
        let failure = with_search_repair(
            search_failure(error),
            ToolId::GhSearchRepo,
            query,
            page_size(query),
            access::repo(query),
        );
        let failure = match access::owner(query) {
            Some(owner) => crate::tools::gh_shared::with_owner_listing(failure, owner),
            None => failure,
        };
        // The message names the login; no repair lead exists for it.
        match missing_owner {
            Some(owner) => GhFailure {
                message: crate::tools::gh_shared::owner_not_found_message(owner),
                hints: Vec::new(),
                ..failure
            },
            None => failure,
        }
    })
}

/// Repositories per page.
fn page_size(_query: &GhSearchRepoQuery) -> usize {
    crate::id::gh_search_repo_policy::PAGE_SIZE
}

pub(crate) async fn execute(
    provider: &GitHubProvider,
    query: &GhSearchRepoQuery,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    if let GhSearchRepoQuery::Get(q) = query {
        return exact_repository(provider, query, &q.owner, &q.repo, context).await;
    }
    crate::tools::gh_shared::reject_boolean_keywords(&access::keywords(query))?;
    let current = access::page(query);
    let per = page_size(query);
    let fetched = fetch_page(provider, query, current, per, context).await;
    // GitHub search does not follow renames: a search inside one
    // repository that a rename could explain reruns once under the
    // canonical name.
    if let (Some(owner), Some(repo)) = (access::owner(query), access::repo(query))
        && search_may_be_renamed(&fetched, |(page, _)| {
            page.items.is_empty() && page.total_count == 0
        })
    {
        let rerun = |renamed: GhSearchRepoQuery| async move {
            Box::pin(execute(provider, &renamed, context)).await
        };
        if let Some(output) = rerun_renamed(provider, query, owner, repo, context, rerun).await? {
            return Ok(output);
        }
    }
    let (data, listing) = fetched?;
    let page = RepoPage::of(&data, listing.as_ref(), current, per);
    let mut leads = top_leads(data.items.first());
    // With archive exclusion, a listing that skipped archived repositories
    // offers a fresh search whose typed archive filter includes them.
    if access::archive(query) == GitHubArchive::Exclude && page.archived_skipped > 0 {
        leads.insert(
            leads.len().min(1),
            ("includeArchived", include_archived(query)),
        );
    }
    let wanted = wanted_topics(query);
    let repositories = data
        .items
        .into_iter()
        .map(|item| repository_row(item, &wanted))
        .collect::<Vec<_>>();
    let ignored_language = match &listing {
        None => ignored_language(provider, query, &repositories, page.matched, context).await,
        Some(_) => None,
    };
    let empty = repositories.is_empty();
    let mut value = page.value(repositories);
    add_next(
        &mut value,
        ToolId::GhSearchRepo,
        query,
        page.next_from,
        page.more,
    );
    for (name, lead) in leads {
        value["calls"][name] = lead;
    }
    page.disclose(&mut value, query, ignored_language);
    pick_rows(&mut value, "repositories", REPO_ID, &access::fields(query));
    Ok(repository_output(
        value,
        query,
        empty && !page.more && !page.past_end,
        page.incomplete,
        page.matched > page.reachable,
    ))
}

/// One page of repositories: an owner's REST listing (no 1,000-result
/// search window, no total) or a search page.
async fn fetch_page(
    provider: &GitHubProvider,
    query: &GhSearchRepoQuery,
    current: usize,
    per: usize,
    context: &RequestContext,
) -> Result<(RepositorySearchPage, Option<OwnerListing>), ProviderError> {
    if let Some(owner) = owner_only(query) {
        let archive = access::archive(query);
        let (data, listing) = owner_listing(
            provider,
            owner,
            archive,
            access::order(query),
            current,
            per,
            context,
        )
        .await?;
        return Ok((data, Some(listing)));
    }
    SearchWindow::PageStart.reject(current, per)?;
    let data = provider
        .transport
        .search_repositories(
            &RepositorySearchRequest {
                query: query::repositories(query),
                sort: access::search_sort(query),
                page: current,
                per_page: per,
            },
            context,
        )
        .await?;
    Ok((data, None))
}

/// `owner`+`repo` alone: that repository's row, with its default branch, from the
/// repository endpoint; no search request and no page.
async fn exact_repository(
    provider: &GitHubProvider,
    query: &GhSearchRepoQuery,
    owner: &str,
    repo: &str,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    let item = provider.transport.repository(owner, repo, context).await?;
    // GitHub answers a renamed repository's old name with the new one.
    let renamed = item
        .full_name
        .split_once('/')
        .filter(|(to_owner, to_repo)| {
            !to_owner.eq_ignore_ascii_case(owner) || !to_repo.eq_ignore_ascii_case(repo)
        })
        .map(|(to_owner, to_repo)| (to_owner.to_owned(), to_repo.to_owned()));
    let row = repository_row(item, &[]);
    let fields = access::fields(query);
    let mut value = json!({ "repositories": [row] });
    pick_rows(&mut value, "repositories", REPO_ID, &fields);
    // The repository's natural next reads, on its default branch, under
    // the name it has now: its root tree and its refs.
    let (at_owner, at_repo) = renamed
        .as_ref()
        .map_or((owner, repo), |(to_owner, to_repo)| {
            (to_owner.as_str(), to_repo.as_str())
        });
    let root = crate::tools::gh_shared::RepoPath::root(at_owner, at_repo);
    let mut refs = root.row();
    refs["operation"] = json!("refs");
    value["calls"] = json!({
        "viewTree": root.lead(ToolId::GhStructure).build(),
        "viewRefs": Continuation::related(ToolId::GhStructure, refs).build(),
    });
    if let Some(to) = renamed {
        value["warnings"] = json!([crate::tools::gh_shared::renamed_warning(owner, repo, &to)]);
    }
    crate::tools::gh_shared::finish_calls(&mut value);
    Ok(ToolData::from(value))
}

/// A repository row's id: a `fields` pick keeps it.
const REPO_ID: &[&str] = &["owner", "repo"];

/// The filters a query can set beside its keywords, by wire name, one
/// group each: the one table every narrowing check reads.
const FILTERS: &[&[&str]] = &[&["filters"], &["matchIn"]];

fn owner_only(query: &GhSearchRepoQuery) -> Option<&str> {
    match query {
        GhSearchRepoQuery::List(q) => Some(&q.owner),
        _ => None,
    }
}

/// This tool's output facts for the shared response stages.
pub(crate) struct Output;
impl crate::response::output::ToolOutput for Output {
    /// A missing owner names the login and has no repair; an exact
    /// repository that did not resolve carries its own `findRepository`
    /// lead. Neither is helped by the search fallback.
    fn error_hint(&self, code: ErrorCode, _query: &serde_json::Value) -> Option<&'static str> {
        (code == ErrorCode::NotFound).then_some(crate::response::output::NO_HINT)
    }
}
