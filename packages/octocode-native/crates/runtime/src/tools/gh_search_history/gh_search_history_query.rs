//! Operation-independent views over the generated wire query.
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
        GhSearchHistoryQuery::Issue { owner, .. } | GhSearchHistoryQuery::Commit { owner, .. } => {
            Some(owner.as_str())
        }
    }
}
pub(crate) fn repo(query: &GhSearchHistoryQuery) -> Option<&str> {
    match query {
        GhSearchHistoryQuery::PullRequest { repo, .. } => repo.as_deref().map(String::as_str),
        GhSearchHistoryQuery::Issue { repo, .. } | GhSearchHistoryQuery::Commit { repo, .. } => {
            Some(repo.as_str())
        }
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
        GhSearchHistoryQuery::Issue { qualifiers, .. } => qualifiers.as_deref().map(String::as_str),
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
        | GhSearchHistoryQuery::Issue { since, .. } => since.as_ref().map(|value| value.as_str()),
        GhSearchHistoryQuery::Commit { since, .. } => since.as_ref().map(|value| value.as_str()),
    }
}
pub(crate) fn until(query: &GhSearchHistoryQuery) -> Option<&str> {
    match query {
        GhSearchHistoryQuery::PullRequest { until, .. }
        | GhSearchHistoryQuery::Issue { until, .. } => until.as_ref().map(|value| value.as_str()),
        GhSearchHistoryQuery::Commit { until, .. } => until.as_ref().map(|value| value.as_str()),
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
