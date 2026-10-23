//! One provider page: a search, or the REST commit or pull-request list.
use super::{HistoryOperation, HistorySearch, gh_search_history_query, query};
use octocode_github::{
    CommitListRequest, GitHubTransport, HistoryPage, HistoryRequest, ProviderError,
    PullListRequest, RequestContext,
};

/// One provider page and the search terms it ran.
pub(super) struct Fetched {
    pub(super) result: HistoryPage,
    pub(super) terms: String,
    pub(super) warnings: Vec<String>,
}

pub(super) async fn fetch(
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
