//! Routing (search or REST list) and the GitHub search terms a query runs.
use super::{GhSearchHistoryQuery, HistoryOperation, HistorySearch};
use crate::providers::github::resolve_date_window;
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
        || gh_search_history_query::since(q).is_some()
        || gh_search_history_query::until(q).is_some()
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
fn validate_history_scope(q: &HistorySearch) -> Result<(), ProviderError> {
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

/// Resolves `since`/`until`, rejecting an unparseable value or an inverted
/// window (since after until) as a validation error: a dropped bound would
/// list unfiltered rows as if they matched.
pub(super) fn resolve_commit_window(
    q: &GhSearchHistoryQuery,
    warnings: &mut Vec<String>,
) -> Result<(Option<String>, Option<String>), ProviderError> {
    for (field, value) in [
        ("since", gh_search_history_query::since(q)),
        ("until", gh_search_history_query::until(q)),
    ] {
        if let Some(value) = value
            && resolve_date_window(value).value.is_none()
        {
            return Err(ProviderError::new(
                ProviderErrorKind::Validation,
                format!(
                    "{field} \"{}\" is not a date or relative window; use e.g. \"30d\", \"2w\", \"6m\", \"1y\", or an ISO date like \"2026-01-01\".",
                    value.trim()
                ),
            ));
        }
    }
    let since = gh_search_history_query::since(q).map(resolve_date_window);
    let until = gh_search_history_query::until(q).map(resolve_date_window);
    if let (Some(since), Some(until)) = (&since, &until)
        && since.is_after(until)
    {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            format!(
                "since ({}) is after until ({}); swap them or widen the window",
                gh_search_history_query::since(q).unwrap_or_default().trim(),
                gh_search_history_query::until(q).unwrap_or_default().trim()
            ),
        ));
    }
    let mut values = [None, None];
    for (slot, window) in values.iter_mut().zip([since, until]) {
        if let Some(window) = window {
            warnings.extend(window.warning);
            *slot = window.value;
        }
    }
    let [since, until] = values;
    Ok((since, until))
}

fn commit_terms(
    q: &HistorySearch,
    out: &mut SearchTerms,
    warnings: &mut Vec<String>,
) -> Result<(), ProviderError> {
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
    let (since, until) = resolve_commit_window(q, warnings)?;
    if let Some(range) = window_range(since.as_deref(), until.as_deref()) {
        out.push(format!("committer-date:{range}"));
    }
    Ok(())
}

/// A date-only `until` as the end of that day: the REST commit listing reads
/// a bare date as its midnight, which would drop the day search's
/// `committer-date:<=` range keeps. A timestamp stays as given.
pub(super) fn inclusive_until(until: &str) -> String {
    if until.len() == "2026-01-01".len() {
        format!("{until}T23:59:59Z")
    } else {
        until.to_owned()
    }
}

/// A resolved `since`/`until` window as one search range value.
pub(super) fn window_range(since: Option<&str>, until: Option<&str>) -> Option<String> {
    match (since, until) {
        (Some(since), Some(until)) => Some(format!("{since}..{until}")),
        (Some(since), None) => Some(format!(">={since}")),
        (None, Some(until)) => Some(format!("<={until}")),
        (None, None) => None,
    }
}

/// The `created:` range a pull-request or issue `since`/`until` window
/// maps to. Search has no generic date, so the window is the creation date
/// (never a guessed `merged:`); a
/// `created:` qualifier set too is a conflict, never silently overridden.
fn created_window(
    q: &HistorySearch,
    warnings: &mut Vec<String>,
) -> Result<Option<String>, ProviderError> {
    if gh_search_history_query::since(q).is_none() && gh_search_history_query::until(q).is_none() {
        return Ok(None);
    }
    if q.filters().created.is_some() {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            "since/until set the created: window; remove created: from qualifiers, or drop since/until.",
        ));
    }
    // The published `since` names the creation date, so no per-call note.
    let (since, until) = resolve_commit_window(q, warnings)?;
    Ok(window_range(since.as_deref(), until.as_deref()))
}

fn issue_terms(
    q: &HistorySearch,
    out: &mut SearchTerms,
    warnings: &mut Vec<String>,
) -> Result<(), ProviderError> {
    let f = q.filters();
    let created = match created_window(q, warnings)? {
        Some(range) => Some(range),
        None => f.created.clone(),
    };
    if !f.match_kinds.is_empty() {
        out.checked("in", Some(&f.match_kinds.join(",")))?;
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
    out.checked("is", q.state().as_deref())?;
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
            ("created", created.as_deref()),
            ("updated", f.updated.as_deref()),
            ("merged", f.merged.as_deref()),
            ("closed", f.closed.as_deref()),
            ("comments", f.comments.as_deref()),
            ("reactions", f.reactions.as_deref()),
            ("linked", f.linked.as_deref()),
            ("review", f.review.as_deref()),
        ])
    {
        out.checked(k, v)?;
    }
    for label in q.label() {
        // A label is one quoted name; an interior quote or backslash
        // would close it early and splice the rest into the query.
        if label.contains(['"', '\\']) || label.trim().is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::Validation,
                format!("label: \"{label}\" cannot contain quotes or backslashes"),
            ));
        }
        out.push(format!("label:\"{}\"", label.trim()));
    }
    if let Some(archived) = f.archived {
        out.push(format!("archived:{archived}"));
    }
    out.checked("status", f.status.as_deref())?;
    for (key, value) in &f.excluded {
        out.excluded(key, value)?;
    }
    Ok(())
}

/// The search terms and the invalid-value warnings collected on the way.
pub(super) fn build_query_with_warnings(
    q: &HistorySearch,
) -> Result<(String, Vec<String>), ProviderError> {
    let mut warnings = Vec::new();
    validate_history_scope(q)?;
    let mut out = SearchTerms::new(gh_search_history_query::keywords(q));
    match gh_search_history_query::operation(q) {
        HistoryOperation::Commit => commit_terms(q, &mut out, &mut warnings)?,
        HistoryOperation::PullRequest | HistoryOperation::Issue => {
            issue_terms(q, &mut out, &mut warnings)?;
        }
    }
    Ok((out.finish(), warnings))
}
