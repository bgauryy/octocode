//! Routing (search or REST list) and the GitHub search terms a query runs.
use super::{GhSearchHistoryQuery, HistoryOperation, HistorySearch};
use crate::tools::gh_search_history::gh_search_history_query;
use crate::tools::gh_shared::query::SearchTerms;
use octocode_github::{ProviderError, ProviderErrorKind, validate_person_name};

/// A keyword-less issue listing keeps the REST list order (newest first)
/// instead of search's unordered best-match.
fn lists_issues_newest_first(q: &GhSearchHistoryQuery) -> bool {
    matches!(
        gh_search_history_query::operation(q),
        HistoryOperation::Issue
    ) && gh_search_history_query::keywords(q).is_empty()
        && gh_search_history_query::sort(q).is_none_or(|sort| sort == "best-match")
}

/// The order GitHub search runs a query in, as `(sort, order)`; no sort is
/// best match. Commit search runs newest first by committer date, and a
/// keyword-less issue listing newest first by creation.
pub(super) fn search_order(q: &GhSearchHistoryQuery) -> (Option<String>, Option<String>) {
    if gh_search_history_query::operation(q) == HistoryOperation::Commit {
        return (Some("committer-date".into()), Some("desc".into()));
    }
    if lists_issues_newest_first(q) {
        return (Some("created".into()), Some("desc".into()));
    }
    (
        gh_search_history_query::sort(q).filter(|sort| sort != "best-match"),
        gh_search_history_query::order(q),
    )
}

/// Issue-style qualifiers only the search API understands.
pub(super) fn needs_issue_search_qualifiers(q: &HistorySearch) -> bool {
    let f = q.filters();
    !gh_search_history_query::keywords(q).is_empty()
        || q.author().is_some()
        || f.assignee.is_some()
        || !q.label().is_empty()
        || f.mentions.is_some()
        || f.commenter.is_some()
        || f.reactions.is_some()
        || f.linked.is_some()
        || f.comments.is_some()
        || f.created.is_some()
        || f.updated.is_some()
        || f.closed.is_some()
        || !f.match_kinds.is_empty()
        || matches!(
            gh_search_history_query::sort(q).as_deref(),
            Some("comments" | "reactions")
        )
}

pub(super) fn should_use_search_for_prs(q: &HistorySearch) -> bool {
    let f = q.filters();
    // The REST list endpoint needs owner+repo; anything broader is search.
    gh_search_history_query::owner(q).is_none()
        || gh_search_history_query::repo(q).is_none()
        || needs_issue_search_qualifiers(q)
        || f.draft.is_some()
        || f.unmerged.is_some()
        || f.reviewed_by.is_some()
        || f.review_requested.is_some()
        || f.status.is_some()
        || f.review.is_some()
        || q.head().is_some()
        || q.base().is_some()
        || f.merged.is_some()
        || !f.excluded.is_empty()
        || q.state().as_deref() == Some("merged")
        // The pulls list cannot filter on repository archive state.
        || f.archived.is_some()
}

/// Whether the operation runs the search API (else a REST list endpoint).
/// Issues always use search: GitHub's REST /issues list interleaves pull
/// requests, so filtering them out of provider pages underfills pages and
/// makes page numbers skip. `is:issue` search pages count issues only.
pub(super) fn uses_search(q: &HistorySearch) -> bool {
    match gh_search_history_query::operation(q) {
        HistoryOperation::Commit => !gh_search_history_query::keywords(q).is_empty(),
        HistoryOperation::Issue => true,
        HistoryOperation::PullRequest => should_use_search_for_prs(q),
    }
}

pub(super) fn required_repo(q: &GhSearchHistoryQuery) -> Result<(&str, &str), ProviderError> {
    gh_search_history_query::owner(q)
        .zip(gh_search_history_query::repo(q))
        .ok_or_else(|| {
            ProviderError::new(ProviderErrorKind::Validation, "owner and repo are required")
        })
}

/// Person fields become `author:`-style qualifiers: reject anything that is
/// not a GitHub name before it can rewrite the search scope (the contract
/// validates owner and repo).
pub(super) fn validate_history_scope(q: &HistorySearch) -> Result<(), ProviderError> {
    let f = q.filters();
    for (field, value) in [("author", q.author()), ("committer", q.committer())]
        .into_iter()
        .chain(f.people())
    {
        if let Some(value) = value {
            validate_person_name(field, value)?;
        }
    }
    Ok(())
}

pub(super) fn resolve_commit_window(q: &GhSearchHistoryQuery) -> (Option<String>, Option<String>) {
    match gh_search_history_query::committed_at(q) {
        Some(range) => (
            range.from.as_ref().map(ToString::to_string),
            range.to.as_ref().map(ToString::to_string),
        ),
        None => (None, None),
    }
}

fn commit_terms(q: &HistorySearch, out: &mut SearchTerms) -> Result<(), ProviderError> {
    let (o, r) = required_repo(q)?;
    out.scope(Some(o), Some(r));
    for (field, value) in [("author", q.author()), ("committer", q.committer())] {
        if let Some(value) = value {
            let key = if value.contains('@') {
                format!("{field}-email")
            } else {
                field.into()
            };
            out.push(format!("{key}:{value}"));
        }
    }
    if let Some(range) =
        gh_search_history_query::committed_at(q).and_then(crate::tools::gh_shared::ranges::date)
    {
        out.push(format!("committer-date:{range}"));
    }
    Ok(())
}

fn issue_terms(q: &HistorySearch, out: &mut SearchTerms) -> Result<(), ProviderError> {
    let f = q.filters();
    if !f.match_kinds.is_empty() {
        out.text("in", Some(&f.match_kinds.join(",")));
    }
    let pull_request = matches!(
        gh_search_history_query::operation(q),
        HistoryOperation::PullRequest
    );
    out.push(if pull_request { "is:pr" } else { "is:issue" });
    // Pull-request search is cross-repo capable (contract: owner and repo
    // optional); issue search stays repository-scoped.
    if !pull_request {
        required_repo(q)?;
    }
    out.scope(
        gh_search_history_query::owner(q),
        gh_search_history_query::repo(q),
    );
    out.text("is", q.state().as_deref());
    if let Some(draft) = f.draft {
        out.push(if draft { "is:draft" } else { "-is:draft" });
    }
    if f.unmerged == Some(true) {
        out.push("is:unmerged");
    }
    for (k, v) in [("author", q.author())]
        .into_iter()
        .chain(f.people())
        .chain([
            ("head", q.head()),
            ("base", q.base()),
            ("created", f.created.as_deref()),
            ("updated", f.updated.as_deref()),
            ("merged", f.merged.as_deref()),
            ("closed", f.closed.as_deref()),
            ("comments", f.comments.as_deref()),
            ("reactions", f.reactions.as_deref()),
            ("linked", f.linked.as_deref()),
            ("review", f.review.as_deref()),
        ])
    {
        out.text(k, v);
    }
    for label in q.label() {
        out.text("label", Some(label));
    }
    if let Some(archived) = f.archived {
        out.push(format!("archived:{archived}"));
    }
    out.text("status", f.status.as_deref());
    for (key, value) in &f.excluded {
        out.text(&format!("-{key}"), Some(value));
    }
    Ok(())
}

/// The search terms and the invalid-value warnings collected on the way.
pub(super) fn build_query_with_warnings(
    q: &HistorySearch,
) -> Result<(String, Vec<String>), ProviderError> {
    validate_history_scope(q)?;
    let mut out = SearchTerms::new(gh_search_history_query::keywords(q));
    match gh_search_history_query::operation(q) {
        HistoryOperation::Commit => commit_terms(q, &mut out)?,
        HistoryOperation::PullRequest | HistoryOperation::Issue => {
            issue_terms(q, &mut out)?;
        }
    }
    Ok((out.finish(), Vec::new()))
}
