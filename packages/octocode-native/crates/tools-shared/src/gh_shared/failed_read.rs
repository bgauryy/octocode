//! What a failed read or listing names in its leads: the commit it resolved, its canonical name, a submodule's repository.
use super::rename::{CanonicalScope, canonical_scope};
use super::{
    GhFailure, OWNER_SPELLING_HINT, RenameCheck, RepoPath, missing_path, owner_not_found_message,
    repository_not_found, tree_recovery, with_default_branch, with_ref_recovery,
    with_search_repair,
};
use octocode_github::{
    GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use octocode_runtime_base::id::ToolId;
use serde_json::{Value, json};

/// The failed operation's resolved commit and current canonical repository,
/// and its error as the repository's state explains it. The caller carries
/// the commit directly from the attempted read.
pub struct FailureScope {
    pub commit: Option<String>,
    pub canonical: Option<(String, String)>,
    pub error: ProviderError,
}

/// The scope of a read or listing of `owner/repo` that failed with `error`.
/// GitHub answers a missing repository, item, ref or path with the same
/// bare 404: unless this request already saw the repository answer, one
/// metadata read tells them apart, and a missing repository becomes the
/// error (`RepositoryNotFound`), whose leads search for it. Any other
/// failure reads metadata only after a rename redirect.
pub async fn failure_scope(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    commit: Option<String>,
    error: ProviderError,
    context: &RequestContext,
) -> FailureScope {
    let bare_not_found = error.kind == ProviderErrorKind::NotFound && error.reason.is_none();
    let check = if bare_not_found {
        RenameCheck::Metadata
    } else {
        RenameCheck::Redirected
    };
    let (canonical, error) = match canonical_scope(provider, owner, repo, context, check).await {
        Ok(CanonicalScope::Renamed(canonical)) => (Some(canonical), error),
        Ok(CanonicalScope::Missing(_)) if bare_not_found => (
            None,
            error.with_reason(ProviderErrorReason::RepositoryNotFound),
        ),
        Ok(CanonicalScope::Missing(_) | CanonicalScope::Stands(_)) | Err(_) => (None, error),
    };
    FailureScope {
        commit,
        canonical,
        error,
    }
}

/// [`failure_scope`] of a read or listing of `path`: the commit its ref
/// resolved to is kept only for a failure about the path itself (missing,
/// a directory, a file, a submodule), the failures whose leads pin it.
pub async fn read_failure_scope(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    path: &str,
    read_commit: Option<String>,
    error: ProviderError,
    context: &RequestContext,
) -> FailureScope {
    let about_path = matches!(
        error.reason,
        Some(
            ProviderErrorReason::PathIsDirectory
                | ProviderErrorReason::PathIsFile
                | ProviderErrorReason::Submodule
        )
    ) || missing_path(&error, Some(path));
    failure_scope(
        provider,
        owner,
        repo,
        read_commit.filter(|_| about_path),
        error,
        context,
    )
    .await
}

/// The recovery every read or listing of `owner/repo` shares for a failure
/// that is not about its path: a missing ref lists the refs and names the
/// default branch, a repository that did not resolve leads to a search for
/// it (or names its owner when no such login exists:
/// [`name_missing_owner`]), a page past the window to its repair. Any other
/// failure is returned unchanged.
pub async fn with_read_recovery(
    provider: &GitHubProvider,
    failure: GhFailure,
    tool: ToolId,
    query: &impl serde::Serialize,
    owner: &str,
    repo: &str,
    context: &RequestContext,
) -> GhFailure {
    let failure = with_ref_recovery(failure, owner, repo);
    let failure = with_default_branch(provider, failure, tool, query, context).await;
    let failure = with_search_repair(failure, tool, query, 1, Some(owner), Some(repo));
    name_missing_owner(provider, failure, owner, context).await
}

/// Whether no GitHub user or organization has the login `owner` (one
/// request). GitHub answers a repository under such a login exactly like a
/// missing repository, so every tool whose repository did not resolve asks
/// this before it words the failure. A login that exists, or whose read did
/// not answer, is not missing.
pub async fn owner_missing(
    provider: &GitHubProvider,
    owner: &str,
    context: &RequestContext,
) -> bool {
    matches!(
        provider.transport.confirm_user(owner, context).await,
        Err(error) if error.reason == Some(ProviderErrorReason::UserNotFound)
    )
}

/// The failure of a repository that did not resolve, restated for an
/// `owner` that is no login ([`owner_missing`]): the `message` names the
/// owner, and no lead in `calls` asks for that owner (its listing, the same
/// request under another repository name, its refs): each would fail the
/// same way. The other leads and the `hints` that label them stay; with no
/// lead left, the one hint is the owner's spelling.
pub fn state_missing_owner(
    message: &mut String,
    hints: &mut Vec<String>,
    calls: &mut Option<Box<Value>>,
    owner: &str,
) {
    *message = owner_not_found_message(owner);
    *calls = calls.take().and_then(|mut calls| {
        let leads = calls.as_object_mut()?;
        leads.retain(|_, lead| !asks_for(lead, owner));
        (!leads.is_empty()).then_some(calls)
    });
    if calls.is_none() {
        *hints = vec![OWNER_SPELLING_HINT.to_owned()];
    }
}

/// `failure` of a request scoped to `owner`'s repository, every GitHub
/// tool's one answer to a missing owner: a repository that did not resolve
/// (`RepositoryNotFound`) reads the login once, and under a login nobody
/// has it is [`state_missing_owner`]'s failure. Any other failure, and a
/// login that exists, is returned unchanged.
pub async fn name_missing_owner(
    provider: &GitHubProvider,
    mut failure: GhFailure,
    owner: &str,
    context: &RequestContext,
) -> GhFailure {
    if repository_not_found(&failure.error) && owner_missing(provider, owner, context).await {
        state_missing_owner(
            &mut failure.message,
            &mut failure.hints,
            &mut failure.calls,
            owner,
        );
    }
    failure
}

/// Whether a `lead`'s call names `owner` in any of its queries.
fn asks_for(lead: &Value, owner: &str) -> bool {
    lead.pointer("/input/queries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|query| {
            query
                .get("owner")
                .and_then(Value::as_str)
                .is_some_and(|asked| asked.eq_ignore_ascii_case(owner))
        })
}

/// The failure of a request that named a submodule path (a read or a
/// listing), with its recovery: where the parent's contents entry at the
/// request's commit points (one request), then [`submodule_recovery`].
pub async fn submodule_failure(
    provider: &GitHubProvider,
    at: &RepoPath<'_>,
    context: &RequestContext,
    failure: GhFailure,
) -> GhFailure {
    let submodule = match at
        .reference
        .filter(|reference| octocode_github::is_full_sha(reference))
    {
        Some(commit) => provider
            .submodule(at.owner, at.repo, at.path, commit, context)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let repository = submodule
        .as_ref()
        .and_then(|submodule| submodule.url.as_deref())
        .and_then(|url| hosted_repository(url, provider.transport.endpoint()));
    let pinned = submodule.and_then(|submodule| submodule.commit);
    submodule_recovery(failure, repository, pinned.as_deref())
}

/// `owner`/`repo` of a submodule clone URL (https, http, ssh or git scheme,
/// or scp-like `git@host:owner/repo`) hosted where `endpoint` serves:
/// github.com for the public API, else the Enterprise host itself.
pub fn hosted_repository(
    url: &str,
    endpoint: &octocode_github::GitHubEndpoint,
) -> Option<(String, String)> {
    let (host, path) = match url::Url::parse(url) {
        Ok(parsed) if parsed.host_str().is_some() => (
            parsed.host_str()?.to_owned(),
            parsed.path().trim_start_matches('/').to_owned(),
        ),
        // scp-like syntax: `[user@]host:path`.
        _ => {
            let (authority, path) = url.split_once(':')?;
            let host = authority.rsplit('@').next()?;
            (host.to_owned(), path.trim_start_matches('/').to_owned())
        }
    };
    if !host.eq_ignore_ascii_case(endpoint.credential_host()) {
        return None;
    }
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut segments = path.rsplit('/');
    let repo = segments.next().filter(|part| !part.is_empty())?;
    let owner = segments.next().filter(|part| !part.is_empty())?;
    Some((owner.to_owned(), repo.to_owned()))
}

/// The recovery of a submodule path: the submodule's GitHub repository at
/// the commit the parent pins (when the listing named it), else, for a
/// submodule hosted elsewhere, the fact that it is not on GitHub.
fn submodule_recovery(
    mut failure: GhFailure,
    repository: Option<(String, String)>,
    pinned: Option<&str>,
) -> GhFailure {
    let Some((owner, repo)) = repository else {
        return failure.hint("The submodule's repository is not on GitHub.");
    };
    let at = RepoPath {
        owner: &owner,
        repo: &repo,
        path: "",
        reference: pinned,
    };
    failure.hints.clear();
    failure.calls = Some(Box::new(json!({ "viewTree": tree_recovery(&at, None) })));
    failure
}
