//! One provider page: a search, or the REST commit or pull-request list.
use super::{HistoryOperation, HistorySearch, gh_search_history_query, query};
use octocode_github::{
    CommitListRequest, GitHubTransport, HistoryPage, HistoryRequest, ProviderError,
    PullListRequest, RequestContext, RequestSpec,
};
use serde_json::Value;

/// One provider page and the search terms it ran.
pub(super) struct Fetched {
    pub(super) result: HistoryPage,
    pub(super) terms: String,
    pub(super) warnings: Vec<String>,
    /// The commit the path's history ends at, when it renamed the path.
    pub(super) rename: Option<RenameEnd>,
}

/// The commit a path-scoped listing ends at, when that commit gave the path
/// its name: `path` was `previous_path` before it.
pub(super) struct RenameEnd {
    pub(super) commit: String,
    pub(super) previous_path: String,
    /// The commit's first parent: the last state under the previous path.
    pub(super) parent: Option<String>,
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
    let rename = match (
        gh_search_history_query::operation(query),
        gh_search_history_query::path(query),
    ) {
        (HistoryOperation::Commit, Some(path)) if !searching && !result.has_more => {
            rename_end(transport, query, path, &result.items, context).await
        }
        _ => None,
    };
    Ok(Fetched {
        result,
        terms: request.query,
        warnings,
        rename,
    })
}

/// A path's history ends where the path began: when its oldest listed commit
/// renamed it, the commit's own file list says from what. One read of that
/// commit; a read that fails or finds no rename says nothing.
async fn rename_end(
    transport: &GitHubTransport,
    query: &HistorySearch,
    path: &str,
    items: &[Value],
    context: &RequestContext,
) -> Option<RenameEnd> {
    let commit = items.last()?.get("sha")?.as_str()?;
    let (owner, repo) = query::required_repo(query).ok()?;
    let url = transport
        .endpoint()
        .rest(&["repos", owner, repo, "commits", commit])
        .ok()?;
    let page = transport
        .execute(RequestSpec::get(url), context)
        .await
        .ok()?;
    let read: Value = serde_json::from_slice(&page.body).ok()?;
    let file = read.get("files")?.as_array()?.iter().find(|file| {
        file["filename"] == path
            && matches!(file["status"].as_str(), Some("renamed" | "added"))
            && file["previous_filename"].is_string()
    })?;
    Some(RenameEnd {
        commit: commit.to_owned(),
        previous_path: file["previous_filename"].as_str()?.to_owned(),
        parent: read["parents"][0]["sha"].as_str().map(str::to_owned),
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
    let (since, until) = query::resolve_commit_window(query);
    transport
        .list_commits_by_committer(
            &CommitListRequest {
                owner: o.into(),
                repo: r.into(),
                branch: gh_search_history_query::reference(query).map(str::to_owned),
                path: gh_search_history_query::path(query).map(str::to_owned),
                author: query.author().map(str::to_owned),
                since,
                until,
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
