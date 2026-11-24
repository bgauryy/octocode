//! Fields retained by explicit projection or by the deciding sort value.
use super::{HistorySearch, gh_search_history_query};

/// What a history row leaves to a `fields` pick. A default row names the
/// item (number or SHA), dates it and states its title or headline (a pull
/// request or issue also its `state`, a commit its `merge` mark and its
/// author, which the listing already carries): enough to choose the one to
/// read. A pull request's or issue's author, its labels, its comment counts,
/// its last update and its target branch, and a commit's committer, are read
/// by `fields`, or by ghGetHistoryItem.
pub(super) const PULL_REQUEST_DETAIL: &[&str] = &[
    "author",
    "labels",
    "commentsCount",
    "totalCommentsCount",
    "reactionCount",
    "updatedAt",
    "baseRef",
];
pub(super) const ISSUE_DETAIL: &[&str] = &[
    "author",
    "labels",
    "commentsCount",
    "reactionCount",
    "updatedAt",
];
pub(super) const COMMIT_DETAIL: &[&str] = &["committer"];

/// `detail` less the fields the rows are sorted or selected by: a page
/// ranked or filtered by comments states each row's counts (a pull
/// request's whole count, which GitHub ranks and selects by, beside its
/// conversation count), and a page sorted by reactions or by update states
/// that value, since they decided the rows.
pub(super) fn detail_unless_sorted(
    query: &HistorySearch,
    detail: &'static [&'static str],
) -> Vec<&'static str> {
    let mut sorted = Vec::new();
    if super::super::comment_counts::by_comments(query) {
        sorted.extend(["commentsCount", "totalCommentsCount"]);
    }
    match gh_search_history_query::sort(query).as_deref() {
        Some("reactions") => sorted.push("reactionCount"),
        Some("updated") => sorted.push("updatedAt"),
        _ => {}
    }
    detail
        .iter()
        .copied()
        .filter(|field| !sorted.contains(field))
        .collect()
}
