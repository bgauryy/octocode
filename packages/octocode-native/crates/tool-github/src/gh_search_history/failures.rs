//! Failures and their recovery: a renamed repository, the search window, a missing scope or ref.
use super::{HistorySearch, gh_search_history_query, leads};
use octocode_github::{ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext};
use octocode_tools_shared::gh_shared::GhFailure;
use serde_json::{Value, json};

/// A history search's failure for `error`, as [`octocode_tools_shared::gh_shared::history`] words it.
pub(super) fn history_failure(error: ProviderError) -> GhFailure {
    octocode_tools_shared::gh_shared::history::history_tool_failure(error, true)
}

/// [`history_failure`] with its recovery `calls`, and `hints` that label them
/// (empty: the history hint for its error).
fn with_leads(error: ProviderError, calls: Option<Value>, hints: Vec<String>) -> GhFailure {
    let mut failure = history_failure(error);
    if !hints.is_empty() {
        failure.hints = hints;
    }
    failure.calls = calls.map(Box::new);
    failure
}

/// Points the query at the repository's canonical name when it was renamed
/// ([`octocode_tools_shared::gh_shared::rescope_on_rename`]), returning the rename
/// warnings; `None` when the name stands. A missing repository keeps the
/// original answer, which states it.
pub(super) async fn follow_rename(
    provider: &octocode_github::GitHubProvider,
    query: &mut HistorySearch,
    context: &RequestContext,
) -> Result<Option<Vec<String>>, ProviderError> {
    use octocode_tools_shared::gh_shared::Rescope;
    let Some((owner, repo)) =
        gh_search_history_query::owner(query).zip(gh_search_history_query::repo(query))
    else {
        return Ok(None);
    };
    Ok(
        match octocode_tools_shared::gh_shared::rescope_on_rename(provider, owner, repo, context)
            .await?
        {
            Rescope::Renamed {
                owner,
                repo,
                warning,
            } => {
                // A name the contract would reject leaves the query as it is.
                if let Ok(renamed) =
                    octocode_tools_shared::gh_shared::with_repo(&query.query, &owner, &repo)
                {
                    query.query = renamed;
                }
                Some(vec![warning])
            }
            Rescope::Missing(_) | Rescope::Stands(_) => None,
        },
    )
}

/// A page past GitHub's 1,000-result search window, with its one repair:
/// the last reachable page (`repair`).
pub(super) fn window_failure(error: ProviderError, query: &HistorySearch, per: usize) -> GhFailure {
    octocode_tools_shared::gh_shared::with_search_repair(
        history_failure(error),
        octocode_runtime_base::id::ToolId::GhSearchHistory,
        &query.query,
        per,
        None,
        None,
    )
}

/// A bare GitHub 404 of a listing, or a search scoped to a missing
/// repository. A commit listing at a `ref` cannot tell a missing ref from a
/// missing repository: the message names the ref and the refs listing is the
/// one lead (it fails on a missing repository). Otherwise the repository
/// itself did not resolve: the message names it and a search for its name is
/// the lead.
pub(super) fn missing_scope_failure(mut error: ProviderError, query: &HistorySearch) -> GhFailure {
    if error.reason == Some(ProviderErrorReason::UserNotFound) {
        let next = leads::without_users(query).map(|lead| json!({ "withoutUsers": lead }));
        return with_leads(error, next, Vec::new());
    }
    // A search scoped to a missing repository fails typed (GitHub's 422);
    // a listing's 404 does not say which of repository and ref is missing.
    let repository_missing = octocode_tools_shared::gh_shared::repository_not_found(&error);
    if error.kind != ProviderErrorKind::NotFound || (error.reason.is_some() && !repository_missing)
    {
        return history_failure(error);
    }
    let (Some(owner), Some(repo)) = (
        gh_search_history_query::owner(query),
        gh_search_history_query::repo(query),
    ) else {
        error.message = octocode_tools_shared::gh_shared::provider_message(&error).into_boxed_str();
        return history_failure(error);
    };
    let reference = gh_search_history_query::reference(query).filter(|_| !repository_missing);
    let Some(reference) = reference else {
        // The repository itself did not resolve: the message names it, and
        // its recovery is the owner/repo spelling, not a history filter.
        error.message = octocode_tools_shared::gh_shared::repository_not_found_message(owner, repo)
            .into_boxed_str();
        return with_leads(
            error,
            Some(octocode_tools_shared::gh_shared::repository_recovery(
                Some(owner),
                repo,
            )),
            vec![octocode_tools_shared::gh_shared::REPOSITORY_SEARCH_HINT.to_owned()],
        );
    };
    error.message =
        format!("Ref \"{reference}\" or repository {owner}/{repo} not found").into_boxed_str();
    with_leads(
        error,
        Some(json!({"viewRefs": octocode_tools_shared::gh_shared::ref_recovery(owner, repo)})),
        Vec::new(),
    )
}

/// A commit listing at a `ref` that did not resolve ([`missing_scope_failure`]
/// leads with `viewRefs`): when the repository itself resolves, the failure
/// names its default branch and offers the same listing at it.
pub(super) async fn at_default_branch(
    provider: &octocode_github::GitHubProvider,
    mut failure: GhFailure,
    query: &HistorySearch,
    context: &RequestContext,
) -> GhFailure {
    let (Some(owner), Some(repo), Some(reference)) = (
        gh_search_history_query::owner(query),
        gh_search_history_query::repo(query),
        gh_search_history_query::reference(query),
    ) else {
        return failure;
    };
    if !failure
        .calls
        .as_ref()
        .is_some_and(|next| next.get("viewRefs").is_some())
    {
        return failure;
    }
    let row = octocode_runtime_base::response::result::query_row(&query.query, &[]);
    if let Some(retry) = octocode_tools_shared::gh_shared::default_branch_retry(
        provider,
        octocode_runtime_base::id::ToolId::GhSearchHistory,
        row,
        &["ref"],
        context,
    )
    .await
    {
        // The repository resolved, so only the ref is missing.
        failure.message =
            retry.message(&format!("Ref \"{reference}\" not found in {owner}/{repo}"));
        let (name, lead) = retry.lead();
        if let Some(next) = failure.calls.as_mut() {
            next[name] = lead;
        }
    }
    failure
}
