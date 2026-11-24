//! ghSearchRepo: repository search, or an owner-only repository listing.
mod access;
mod language;
mod leads;
pub mod owner_listing;
mod page;
#[cfg(test)]
mod provider_tests;
mod query;
mod rows;
#[cfg(test)]
mod tests;

use octocode_contract_types::{GhSearchRepoQuery, GitHubArchive};
use octocode_github::{
    GitHubProvider, ProviderError, RepositorySearchPage, RepositorySearchRequest, RequestContext,
};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::ToolData;
use octocode_tools_shared::gh_shared::{
    GhFailure, Rerun, SearchWindow, add_next, lean_rows, pick_rows, rerun_renamed, search_failure,
    search_may_be_renamed, with_search_repair,
};
use serde_json::json;

use language::ignored_language;
use leads::{include_archived, repository_output};
use owner_listing::{OwnerListing, owner_listing};
use page::RepoPage;
use rows::{repository_row, wanted_topics};

/// Run one ghSearchRepo row.
pub async fn run(
    provider: &GitHubProvider,
    query: &GhSearchRepoQuery,
    request: Result<&RequestContext, ProviderError>,
) -> Result<ToolData, GhFailure> {
    let context = request.as_ref().ok().copied();
    let result = match request {
        Ok(context) => execute(provider, query, context).await,
        Err(error) => Err(error),
    };
    let error = match result {
        Ok(output) => return Ok(output),
        Err(error) => error,
    };
    // An owner listing's bare 404 is the login itself.
    let missing_owner = owner_only(query).filter(|_| {
        error.kind == octocode_github::ProviderErrorKind::NotFound && error.reason.is_none()
    });
    let failure = with_search_repair(
        search_failure(error),
        ToolId::GhSearchRepo,
        query,
        access::limit(query),
        access::owner(query),
        access::repo(query),
    );
    // The message names the login; no repair lead exists for it.
    if let Some(owner) = missing_owner {
        return Err(GhFailure {
            message: octocode_tools_shared::gh_shared::owner_not_found_message(owner),
            ..failure
        }
        .hint(octocode_tools_shared::gh_shared::OWNER_SPELLING_HINT));
    }
    // A repository that did not resolve names its owner when no such login
    // exists, as every read does.
    Err(match (context, access::owner(query)) {
        (Some(context), Some(owner)) => {
            octocode_tools_shared::gh_shared::failed_read::name_missing_owner(
                provider, failure, owner, context,
            )
            .await
        }
        _ => failure,
    })
}

pub(crate) async fn execute(
    provider: &GitHubProvider,
    query: &GhSearchRepoQuery,
    context: &RequestContext,
) -> Result<ToolData, ProviderError> {
    if let GhSearchRepoQuery::Get(q) = query {
        return exact_repository(provider, query, &q.owner, &q.repo, context).await;
    }
    octocode_tools_shared::gh_shared::reject_boolean_keywords(&access::keywords(query))?;
    leads::reject_qualifier_keywords(&access::keywords(query))?;
    let current = access::page(query);
    let per = access::limit(query);
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
        if let Rerun::Renamed(output) =
            rerun_renamed(provider, query, owner, repo, context, rerun).await?
        {
            return Ok(output);
        }
    }
    let (data, listing) = fetched?;
    let page = RepoPage::of(&data, listing.as_ref(), current, per);
    // With archive exclusion, a listing page left empty by the archived
    // repositories it left out offers the same listing including them: the
    // recovery of an empty row (a row with repositories is the answer).
    let archived_lead = (access::archive(query) == GitHubArchive::Exclude
        && page.archive_filtered > 0
        && data.items.is_empty())
    .then(|| include_archived(query));
    let wanted = wanted_topics(query);
    let fields = access::fields(query);
    let repositories = data
        .items
        .into_iter()
        .map(|item| repository_row(item, &wanted, &fields))
        .collect::<Result<Vec<_>, _>>()?;
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
    if let Some(lead) = archived_lead {
        value["calls"]["includeArchived"] = lead;
    }
    page.disclose(&mut value, query, ignored_language);
    // A page ordered by a row field (an owner listing by push time, a search
    // by forks or update time) states that field on each row.
    let sorted = access::sorted_detail(query);
    let detail = LISTED_DETAIL
        .iter()
        .copied()
        .filter(|field| Some(*field) != sorted)
        .collect::<Vec<_>>();
    lean_rows(&mut value, "repositories", &detail, &fields);
    pick_rows(&mut value, "repositories", REPO_ID, &fields);
    let owner_has_no_repositories = empty
        && current == 1
        && !page.incomplete
        && listing
            .as_ref()
            .is_some_and(|listing| !listing.more && listing.filtered_out == 0);
    let mut output = repository_output(
        value,
        query,
        empty && !page.more && !page.past_end,
        page.incomplete,
        page.matched > page.reachable,
    );
    // The owner exists (the listing answered) and lists nothing, with no
    // archive filter to blame: say so instead of leaving a bare empty.
    if owner_has_no_repositories && let Some(owner) = access::owner(query) {
        octocode_runtime_base::response::result::add_hints(
            &mut output.data,
            [format!(
                "`{owner}` exists but lists no repositories visible to this token; no filter hid any."
            )],
        );
    }
    Ok(output)
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
    let fields = access::fields(query);
    let row = repository_row(item, &[], &fields)?;
    let mut value = json!({ "repositories": [row] });
    lean_rows(&mut value, "repositories", EXACT_DETAIL, &fields);
    pick_rows(&mut value, "repositories", REPO_ID, &fields);
    // The row is the answer: no lead (the tool description routes to
    // ghStructure), and a renamed repository's warning names it as it is now.
    if let Some(to) = renamed {
        value["warnings"] = json!([octocode_tools_shared::gh_shared::renamed_warning(
            owner, repo, &to
        )]);
    }
    Ok(ToolData::from(value))
}

/// A repository row's id and its `visibility` mark: a `fields` pick keeps
/// them, so a private row stays marked.
const REPO_ID: &[&str] = &["owner", "repo", "visibility"];

/// What a listed row leaves to a `fields` pick: a search or an owner listing
/// answers with each repository's name, stars and description (and its
/// `archived`, `fork` and `visibility` marks), which choose the repository;
/// its branch, language, license, dates, forks, homepage, open issue count
/// and size are read by `fields` or by `operation: get`.
const LISTED_DETAIL: &[&str] = &[
    "defaultBranch",
    "language",
    "license",
    "pushedAt",
    "createdAt",
    "topics",
    "forks",
    "updatedAt",
    "homepage",
    "openIssuesCount",
    "sizeKiB",
];

/// What one repository's own row leaves to a `fields` pick: `get` names the
/// repository, its default branch, its size in stars, its language and
/// license, and when it was last pushed.
const EXACT_DETAIL: &[&str] = &[
    "createdAt",
    "topics",
    "forks",
    "updatedAt",
    "homepage",
    "openIssuesCount",
    "sizeKiB",
];

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
pub struct Output;
impl octocode_runtime_base::response::output::ToolOutput for Output {}
