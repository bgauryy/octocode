//! Failures and their recovery: a renamed repository, the search window, a missing scope or ref.
use super::{HistoryFailure, HistorySearch, gh_search_history_query, leads};
use octocode_github::{ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext};
use serde_json::json;

/// Points the query at the repository's canonical name when it was renamed
/// ([`crate::tools::gh_shared::rescope_on_rename`]), returning the rename
/// warnings; `None` when the name stands. A missing repository keeps the
/// original answer, which states it.
pub(super) async fn follow_rename(
    provider: &octocode_github::GitHubProvider,
    query: &mut HistorySearch,
    context: &RequestContext,
) -> Result<Option<Vec<String>>, ProviderError> {
    use crate::tools::gh_shared::Rescope;
    let Some((owner, repo)) =
        gh_search_history_query::owner(query).zip(gh_search_history_query::repo(query))
    else {
        return Ok(None);
    };
    Ok(
        match crate::tools::gh_shared::rescope_on_rename(provider, owner, repo, context).await? {
            Rescope::Renamed {
                owner,
                repo,
                warning,
            } => {
                // A name the contract would reject leaves the query as it is.
                if let Ok(renamed) = crate::tools::gh_shared::with_repo(&query.query, &owner, &repo)
                {
                    query.query = renamed;
                }
                Some(vec![warning])
            }
            Rescope::Missing(_) | Rescope::Stands => None,
        },
    )
}

/// A page past GitHub's 1,000-result search window, with its one repair:
/// the last reachable page (`repair`).
pub(super) fn window_failure(
    error: ProviderError,
    query: &HistorySearch,
    per: usize,
) -> HistoryFailure {
    let failure = crate::tools::gh_shared::with_search_repair(
        crate::tools::gh_shared::GhFailure::new(error, String::new()),
        crate::id::ToolId::GhSearchHistory,
        &query.query,
        per,
        None,
        None,
    );
    HistoryFailure {
        error: failure.error,
        calls: failure.calls.map(|next| *next),
        hints: failure.hints,
    }
}

/// A bare GitHub 404 of a listing, or a search scoped to a missing
/// repository. A commit listing at a `ref` cannot tell a missing ref from a
/// missing repository: the message names the ref and the refs listing is the
/// one lead (it fails on a missing repository). Otherwise the repository
/// itself did not resolve: the message names it and a search for its name is
/// the lead.
pub(super) fn missing_scope_failure(
    mut error: ProviderError,
    query: &HistorySearch,
) -> HistoryFailure {
    if error.reason == Some(ProviderErrorReason::UserNotFound) {
        let next = leads::without_users(query).map(|lead| json!({ "withoutUsers": lead }));
        return HistoryFailure {
            error: Box::new(error),
            calls: next,
            hints: Vec::new(),
        };
    }
    // A search scoped to a missing repository fails typed (GitHub's 422);
    // a listing's 404 does not say which of repository and ref is missing.
    let repository_missing = crate::tools::gh_shared::repository_not_found(&error);
    if error.kind != ProviderErrorKind::NotFound || (error.reason.is_some() && !repository_missing)
    {
        return error.into();
    }
    let (Some(owner), Some(repo)) = (
        gh_search_history_query::owner(query),
        gh_search_history_query::repo(query),
    ) else {
        error.message = crate::tools::gh_shared::provider_message(&error).into_boxed_str();
        return error.into();
    };
    let reference = gh_search_history_query::reference(query).filter(|_| !repository_missing);
    let Some(reference) = reference else {
        // The repository itself did not resolve: the message names it, and
        // its recovery is the owner/repo spelling, not a history filter.
        error.message =
            crate::tools::gh_shared::repository_not_found_message(owner, repo).into_boxed_str();
        return HistoryFailure {
            error: Box::new(error),
            calls: Some(crate::tools::gh_shared::repository_recovery(
                Some(owner),
                repo,
            )),
            hints: vec![crate::tools::gh_shared::repository_search_hint(
                Some(owner),
                repo,
            )],
        };
    };
    error.message =
        format!("Ref \"{reference}\" or repository {owner}/{repo} not found").into_boxed_str();
    HistoryFailure {
        error: Box::new(error),
        calls: Some(
            serde_json::json!({"viewRefs": crate::tools::gh_shared::ref_recovery(owner, repo)}),
        ),
        hints: Vec::new(),
    }
}

/// A commit listing at a `ref` that did not resolve ([`missing_scope_failure`]
/// leads with `viewRefs`): when the repository itself resolves, the failure
/// names its default branch and offers the same listing at it.
pub(super) async fn at_default_branch(
    provider: &octocode_github::GitHubProvider,
    mut failure: HistoryFailure,
    query: &HistorySearch,
    context: &RequestContext,
) -> HistoryFailure {
    let (Some(owner), Some(repo), Some(reference)) = (
        gh_search_history_query::owner(query),
        gh_search_history_query::repo(query),
        gh_search_history_query::reference(query),
    ) else {
        return failure;
    };
    let Some(next) = failure
        .calls
        .as_mut()
        .filter(|next| next.get("viewRefs").is_some())
    else {
        return failure;
    };
    let row = crate::response::result::query_row(&query.query, &[]);
    if let Some(retry) = crate::tools::gh_shared::default_branch_retry(
        provider,
        crate::id::ToolId::GhSearchHistory,
        row,
        &["ref"],
        context,
    )
    .await
    {
        // The repository resolved, so only the ref is missing.
        failure.error.message = retry
            .message(&format!("Ref \"{reference}\" not found in {owner}/{repo}"))
            .into_boxed_str();
        let (name, lead) = retry.lead();
        next[name] = lead;
    }
    failure
}
