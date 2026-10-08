//! Routing (search or REST list) and the GitHub search terms a query runs.
use super::{GhSearchHistoryQuery, HistoryOperation, HistorySearch};
use crate::providers::github::{
    ProviderError, ProviderErrorKind, SearchName, quote_search_keyword, resolve_date_window,
    validate_qualifier_value, validate_search_name,
};

/// A keyword-less issue listing keeps the REST list order (newest first)
/// instead of search's unordered best-match.
pub(super) fn lists_issues_newest_first(q: &GhSearchHistoryQuery) -> bool {
    matches!(q.operation(), HistoryOperation::Issue)
        && q.keywords().is_empty()
        && q.sort().is_none_or(|sort| sort == "best-match")
}

/// Issue-style qualifiers only the search API understands.
pub(super) fn needs_issue_search_qualifiers(q: &HistorySearch) -> bool {
    let f = q.filters();
    !q.keywords().is_empty()
        || q.author().is_some()
        || f.assignee.is_some()
        || !q.label().is_empty()
        || f.mentions.is_some()
        || f.commenter.is_some()
        || f.reactions.is_some()
        || f.linked.is_some()
        || f.comments.is_some()
        || f.created.is_some()
        || q.since().is_some()
        || q.until().is_some()
        || f.updated.is_some()
        || f.closed.is_some()
        || !f.match_kinds.is_empty()
        || matches!(q.sort().as_deref(), Some("comments" | "reactions"))
}

pub(super) fn should_use_search_for_prs(q: &HistorySearch) -> bool {
    let f = q.filters();
    // The REST list endpoint needs owner+repo; anything broader is search.
    q.owner().is_none()
        || q.repo().is_none()
        || needs_issue_search_qualifiers(q)
        || f.draft.is_some()
        || f.reviewed_by.is_some()
        || f.review_requested.is_some()
        || f.checks.is_some()
        || f.review.is_some()
        || q.head().is_some()
        || q.base().is_some()
        || f.merged_at.is_some()
        || q.state().as_deref() == Some("merged")
        // The pulls list cannot filter on repository archive state.
        || f.archived.is_some()
}

/// Whether the operation runs the search API (else a REST list endpoint).
/// Issues always use search: GitHub's REST /issues list interleaves pull
/// requests, so filtering them out of provider pages underfills pages and
/// makes page numbers skip. `is:issue` search pages count issues only.
pub(super) fn uses_search(q: &HistorySearch) -> bool {
    match q.operation() {
        HistoryOperation::Commit => !q.keywords().is_empty(),
        HistoryOperation::Issue => true,
        HistoryOperation::PullRequest => should_use_search_for_prs(q),
    }
}

pub(super) fn required_repo(q: &GhSearchHistoryQuery) -> Result<(&str, &str), ProviderError> {
    q.owner().zip(q.repo()).ok_or_else(|| {
        ProviderError::new(ProviderErrorKind::Validation, "owner and repo are required")
    })
}

/// Owner/repo become `repo:`/`user:` scopes and person fields become
/// `author:`-style qualifiers: reject anything that is not a GitHub name
/// before it can rewrite the search scope.
fn validate_history_scope(q: &HistorySearch) -> Result<(), ProviderError> {
    if let Some(owner) = q.owner() {
        validate_search_name("owner", owner, SearchName::Owner)?;
    }
    if let Some(repo) = q.repo() {
        validate_search_name("repo", repo, SearchName::Repository)?;
    }
    let f = q.filters();
    for (field, value) in [("author", q.author()), ("committer", q.committer())]
        .into_iter()
        .chain(f.people())
    {
        if let Some(value) = value {
            validate_search_name(field, value, SearchName::Person)?;
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
    for (field, value) in [("since", q.since()), ("until", q.until())] {
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
    let since = q.since().map(resolve_date_window);
    let until = q.until().map(resolve_date_window);
    if let (Some(since), Some(until)) = (&since, &until)
        && since.is_after(until)
    {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            format!(
                "since ({}) is after until ({}); swap them or widen the window",
                q.since().unwrap_or_default().trim(),
                q.until().unwrap_or_default().trim()
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

/// Range and enum qualifiers are single terms: whitespace inside a range
/// (`> 5`, `a .. b`) is dropped, anything that could open a new term is
/// rejected.
fn push_qualifier(
    out: &mut Vec<String>,
    key: &str,
    value: Option<&str>,
) -> Result<(), ProviderError> {
    if let Some(value) = value {
        let range = value.trim_start().starts_with(['<', '>', '=']) || value.contains("..");
        let compact = if range {
            value
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        } else {
            value.trim().to_owned()
        };
        validate_qualifier_value(key, &compact)?;
        out.push(format!("{key}:{compact}"));
    }
    Ok(())
}

fn commit_terms(
    q: &HistorySearch,
    out: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<(), ProviderError> {
    let (o, r) = required_repo(q)?;
    out.push(format!("repo:{o}/{r}"));
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
/// and a warning names the mapping (never a guessed `merged:`); a
/// `created:` qualifier set too is a conflict, never silently overridden.
fn created_window(
    q: &HistorySearch,
    warnings: &mut Vec<String>,
) -> Result<Option<String>, ProviderError> {
    if q.since().is_none() && q.until().is_none() {
        return Ok(None);
    }
    if q.filters().created.is_some() {
        return Err(ProviderError::new(
            ProviderErrorKind::Validation,
            "since/until set the created: window; remove created: from qualifiers, or drop since/until.",
        ));
    }
    let (since, until) = resolve_commit_window(q, warnings)?;
    let range = window_range(since.as_deref(), until.as_deref());
    if let Some(range) = &range {
        warnings.push(format!(
            "since/until → created:{range}; for the merge date use qualifiers merged:{range}"
        ));
    }
    Ok(range)
}

fn issue_terms(
    q: &HistorySearch,
    out: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<(), ProviderError> {
    let f = q.filters();
    let created = match created_window(q, warnings)? {
        Some(range) => Some(range),
        None => f.created.clone(),
    };
    if !f.match_kinds.is_empty() {
        push_qualifier(out, "in", Some(&f.match_kinds.join(",")))?;
    }
    let pull_request = matches!(q.operation(), HistoryOperation::PullRequest);
    out.push(if pull_request { "is:pr" } else { "is:issue" }.into());
    match (q.owner(), q.repo(), q.operation()) {
        (Some(o), Some(r), _) => out.push(format!("repo:{o}/{r}")),
        // Pull-request search is cross-repo capable (contract: owner and
        // repo optional); issue search stays repository-scoped.
        (Some(o), None, HistoryOperation::PullRequest) => out.push(format!("user:{o}")),
        (None, _, HistoryOperation::PullRequest) => {}
        _ => {
            required_repo(q)?;
        }
    }
    push_qualifier(out, "is", q.state().as_deref())?;
    if let Some(draft) = f.draft {
        out.push(if draft { "is:draft" } else { "-is:draft" }.into());
    }
    for (k, v) in [("author", q.author())]
        .into_iter()
        .chain(f.people())
        .chain([
            ("head", q.head()),
            ("base", q.base()),
            ("created", created.as_deref()),
            ("updated", f.updated.as_deref()),
            ("merged", f.merged_at.as_deref()),
            ("closed", f.closed.as_deref()),
            ("comments", f.comments.as_deref()),
            ("reactions", f.reactions.as_deref()),
            ("linked", f.linked.as_deref()),
            ("review", f.review.as_deref()),
        ])
    {
        push_qualifier(out, k, v)?;
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
    push_qualifier(out, "status", f.checks.as_deref())
}

/// The search terms and the invalid-value warnings collected on the way.
pub(super) fn build_query_with_warnings(
    q: &HistorySearch,
) -> Result<(String, Vec<String>), ProviderError> {
    let mut warnings = Vec::new();
    validate_history_scope(q)?;
    let mut out = q
        .keywords()
        .into_iter()
        .map(quote_search_keyword)
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    match q.operation() {
        HistoryOperation::Commit => commit_terms(q, &mut out, &mut warnings)?,
        HistoryOperation::PullRequest | HistoryOperation::Issue => {
            issue_terms(q, &mut out, &mut warnings)?;
        }
    }
    Ok((out.join(" "), warnings))
}
