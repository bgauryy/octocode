//! What a failed read or listing names in its leads: the commit it resolved, its canonical name, a submodule's repository.
use super::{GhFailure, RenameCheck, RepoPath, canonical_repo, tree_recovery};
use octocode_github::{GitHubProvider, RequestContext};
use serde_json::json;

/// What a failed read or listing names in its leads, both from the
/// memos the failed request filled and read at once: the commit its ref
/// resolved (when the path failed after the ref did: leads pin it, as the
/// request's own continuations would) and its repository's canonical name
/// (leads name a renamed repository as it is now).
pub(crate) struct FailureScope {
    pub commit: Option<String>,
    pub canonical: Option<(String, String)>,
}

pub(crate) async fn failure_scope(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    reference: Option<&str>,
    path_failed: bool,
    context: &RequestContext,
) -> FailureScope {
    let (commit, canonical) = futures_util::future::join(
        async {
            if path_failed {
                failed_at_commit(provider, owner, repo, reference, context).await
            } else {
                None
            }
        },
        canonical_repo(provider, owner, repo, context, RenameCheck::Redirected),
    )
    .await;
    FailureScope {
        commit,
        canonical: canonical.ok().flatten(),
    }
}

/// The commit a failed read or listing resolved before it failed (the ref
/// memo answers; a full SHA is itself), so its recovery leads pin the same
/// commit its success leads would, not the movable ref. Called only after
/// the repository and ref resolved (a path failure); `None` otherwise.
pub(crate) async fn failed_at_commit(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    reference: Option<&str>,
    context: &RequestContext,
) -> Option<String> {
    provider
        .resolve_reference(owner, repo, reference, false, context)
        .await
        .ok()
}

/// The failure of a request that named a submodule path (a read or a
/// listing), with its recovery: where the parent's contents entry at the
/// request's commit points (one request), then [`submodule_recovery`].
pub(crate) async fn submodule_failure(
    provider: &GitHubProvider,
    at: &RepoPath<'_>,
    context: &RequestContext,
    failure: GhFailure,
) -> GhFailure {
    let submodule = match failed_at_commit(provider, at.owner, at.repo, at.reference, context).await
    {
        Some(commit) => provider
            .submodule(at.owner, at.repo, at.path, &commit, context)
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
pub(crate) fn hosted_repository(
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
