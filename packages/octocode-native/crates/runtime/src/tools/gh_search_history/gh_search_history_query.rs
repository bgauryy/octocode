//! Operation-independent projections of the generated canonical history query.
use super::{GhSearchHistoryQuery, HistoryOperation};
use crate::tools::variant_field::variant_field;
use octocode_contract_types::{DateRange, GhSearchHistoryQueryPullRequest};
use octocode_engine::num::saturating_usize;

/// Binds `$row` to the query struct of every operation.
macro_rules! each_query {
    ($query:expr, $row:ident => $body:expr) => {
        variant_field!($query, each [
            GhSearchHistoryQuery::PullRequest, GhSearchHistoryQuery::Issue,
            GhSearchHistoryQuery::CommitKeywords, GhSearchHistoryQuery::Commit3,
        ] $row => $body)
    };
}

/// Binds `$row` to the query struct of an operation that names its
/// repository (all but pull requests, where it is optional); `$other`
/// answers for pull requests.
macro_rules! repo_query {
    ($query:expr, $row:ident => $body:expr, else => $other:expr) => {
        variant_field!($query, each [
            GhSearchHistoryQuery::Issue, GhSearchHistoryQuery::CommitKeywords,
            GhSearchHistoryQuery::Commit3,
        ] $row => $body, else => $other)
    };
}

fn pull_request(query: &GhSearchHistoryQuery) -> Option<&GhSearchHistoryQueryPullRequest> {
    variant_field!(query, each [GhSearchHistoryQuery::PullRequest] row => Some(row), else => None)
}
pub(crate) fn operation(query: &GhSearchHistoryQuery) -> HistoryOperation {
    match query {
        GhSearchHistoryQuery::PullRequest(_) => HistoryOperation::PullRequest,
        GhSearchHistoryQuery::Issue(_) => HistoryOperation::Issue,
        GhSearchHistoryQuery::CommitKeywords(_) | GhSearchHistoryQuery::Commit3(_) => {
            HistoryOperation::Commit
        }
    }
}
pub(crate) fn owner(query: &GhSearchHistoryQuery) -> Option<&str> {
    repo_query!(query, row => Some(row.owner.as_str()),
        else => pull_request(query)?.owner.as_deref().map(String::as_str))
}
pub(crate) fn repo(query: &GhSearchHistoryQuery) -> Option<&str> {
    repo_query!(query, row => Some(row.repo.as_str()),
        else => pull_request(query)?.repo.as_deref().map(String::as_str))
}
pub(crate) fn keywords(query: &GhSearchHistoryQuery) -> Vec<&str> {
    variant_field!(query, each [
        GhSearchHistoryQuery::PullRequest, GhSearchHistoryQuery::Issue,
        GhSearchHistoryQuery::CommitKeywords,
    ] row => row.keywords.iter().map(|value| value.as_str()).collect(), else => Vec::new())
}
pub(crate) fn fields(query: &GhSearchHistoryQuery) -> Vec<String> {
    each_query!(query, row => row.fields.iter().map(ToString::to_string).collect())
}
pub(crate) fn page(query: &GhSearchHistoryQuery) -> Option<usize> {
    each_query!(query, row => Some(saturating_usize(row.page.get())))
}
/// Rows per page: the query's `limit`, its contract default when omitted.
pub(crate) fn limit(query: &GhSearchHistoryQuery) -> usize {
    each_query!(query, row => saturating_usize(row.limit.get()))
}
pub(crate) fn path(query: &GhSearchHistoryQuery) -> Option<&str> {
    variant_field!(query, each [GhSearchHistoryQuery::Commit3]
        row => row.path.as_deref().map(String::as_str), else => None)
}
pub(crate) fn reference(query: &GhSearchHistoryQuery) -> Option<&str> {
    variant_field!(query, each [GhSearchHistoryQuery::Commit3]
        row => row.ref_.as_deref().map(String::as_str), else => None)
}
pub(crate) fn committed_at(query: &GhSearchHistoryQuery) -> Option<&DateRange> {
    variant_field!(query, each [GhSearchHistoryQuery::Commit3]
        row => row.filters.as_ref()?.committed_at.as_ref(), else => None)
}
fn sort_value(query: &GhSearchHistoryQuery) -> Option<serde_json::Value> {
    variant_field!(query, each [GhSearchHistoryQuery::PullRequest, GhSearchHistoryQuery::Issue]
        row => serde_json::to_value(row.sort.as_ref()?).ok(), else => None)
}
pub(crate) fn sort(query: &GhSearchHistoryQuery) -> Option<String> {
    let value = sort_value(query)?;
    Some(
        match value.get("by")?.as_str()? {
            "bestMatch" => "best-match",
            "createdAt" => "created",
            "updatedAt" => "updated",
            "commentCount" => "comments",
            "reactionCount" => "reactions",
            _ => return None,
        }
        .into(),
    )
}
pub(crate) fn order(query: &GhSearchHistoryQuery) -> Option<String> {
    sort_value(query)?.get("order")?.as_str().map(str::to_owned)
}
