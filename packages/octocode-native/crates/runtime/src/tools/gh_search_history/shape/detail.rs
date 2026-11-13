//! Fields retained by explicit projection or by the deciding sort value.
use super::{HistorySearch, gh_search_history_query};

/// What a history row leaves to a `fields` pick. A default row names the
/// item (number or SHA), dates it and states its title or headline (a pull
/// request or issue also its `state`, a commit its `merge` mark): enough to
/// choose the one to read. Who wrote it, its labels, its comment count and
/// its target branch are read by `fields`, or by ghGetHistoryItem.
pub(super) const PULL_REQUEST_DETAIL: &[&str] = &[
    "author",
    "labels",
    "commentsCount",
    "reactionCount",
    "baseRef",
];
pub(super) const ISSUE_DETAIL: &[&str] = &["author", "labels", "commentsCount", "reactionCount"];
pub(super) const COMMIT_DETAIL: &[&str] = &["author", "committer"];

/// `detail` less the field the rows are sorted by: a page ranked by comment
/// count states each row's count, since the order is the answer.
pub(super) fn detail_unless_sorted(
    query: &HistorySearch,
    detail: &'static [&'static str],
) -> Vec<&'static str> {
    let sorted = match gh_search_history_query::sort(query).as_deref() {
        Some("comments") => Some("commentsCount"),
        Some("reactions") => Some("reactionCount"),
        _ => None,
    };
    detail
        .iter()
        .copied()
        .filter(|field| Some(*field) != sorted)
        .collect()
}
