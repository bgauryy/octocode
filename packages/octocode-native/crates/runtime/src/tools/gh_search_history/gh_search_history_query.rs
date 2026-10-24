//! Operation-independent projections of the generated canonical history query.
use super::{GhSearchHistoryQuery, HistoryOperation};
use crate::num::usize_of;
use octocode_contract_types::DateRange;

macro_rules! each_query {
    ($query:expr, $row:ident => $body:expr) => {
        match $query {
            GhSearchHistoryQuery::PullRequest($row) => $body,
            GhSearchHistoryQuery::Issue($row) => $body,
            GhSearchHistoryQuery::CommitKeywords($row) => $body,
            GhSearchHistoryQuery::Commit3($row) => $body,
        }
    };
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
    match query {
        GhSearchHistoryQuery::PullRequest(query) => query.owner.as_deref().map(String::as_str),
        GhSearchHistoryQuery::Issue(query) => Some(query.owner.as_str()),
        GhSearchHistoryQuery::CommitKeywords(query) => Some(query.owner.as_str()),
        GhSearchHistoryQuery::Commit3(query) => Some(query.owner.as_str()),
    }
}
pub(crate) fn repo(query: &GhSearchHistoryQuery) -> Option<&str> {
    match query {
        GhSearchHistoryQuery::PullRequest(query) => query.repo.as_deref().map(String::as_str),
        GhSearchHistoryQuery::Issue(query) => Some(query.repo.as_str()),
        GhSearchHistoryQuery::CommitKeywords(query) => Some(query.repo.as_str()),
        GhSearchHistoryQuery::Commit3(query) => Some(query.repo.as_str()),
    }
}
pub(crate) fn keywords(query: &GhSearchHistoryQuery) -> Vec<&str> {
    match query {
        GhSearchHistoryQuery::PullRequest(query) => {
            query.keywords.iter().map(|value| value.as_str()).collect()
        }
        GhSearchHistoryQuery::Issue(query) => {
            query.keywords.iter().map(|value| value.as_str()).collect()
        }
        GhSearchHistoryQuery::CommitKeywords(query) => {
            query.keywords.iter().map(|value| value.as_str()).collect()
        }
        GhSearchHistoryQuery::Commit3(_) => Vec::new(),
    }
}
pub(crate) fn fields(query: &GhSearchHistoryQuery) -> Vec<String> {
    each_query!(query, row => row.fields.iter().map(ToString::to_string).collect())
}
pub(crate) fn page(query: &GhSearchHistoryQuery) -> Option<usize> {
    each_query!(query, row => Some(usize_of(row.page)))
}
pub(crate) fn path(query: &GhSearchHistoryQuery) -> Option<&str> {
    match query {
        GhSearchHistoryQuery::Commit3(query) => query.path_prefix.as_deref().map(String::as_str),
        _ => None,
    }
}
pub(crate) fn reference(query: &GhSearchHistoryQuery) -> Option<&str> {
    match query {
        GhSearchHistoryQuery::Commit3(query) => query.ref_.as_deref().map(String::as_str),
        _ => None,
    }
}
pub(crate) fn committed_at(query: &GhSearchHistoryQuery) -> Option<&DateRange> {
    match query {
        GhSearchHistoryQuery::Commit3(query) => query.filters.as_ref()?.committed_at.as_ref(),
        GhSearchHistoryQuery::CommitKeywords(query) => {
            query.filters.as_ref()?.committed_at.as_ref()
        }
        _ => None,
    }
}
fn sort_value(query: &GhSearchHistoryQuery) -> Option<serde_json::Value> {
    match query {
        GhSearchHistoryQuery::PullRequest(query) => serde_json::to_value(query.sort.as_ref()?).ok(),
        GhSearchHistoryQuery::Issue(query) => serde_json::to_value(query.sort.as_ref()?).ok(),
        _ => None,
    }
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
