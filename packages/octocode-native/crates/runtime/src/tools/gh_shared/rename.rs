//! A renamed repository: its memoized canonical name and the rescope of a read or search.
use super::{push_warning_front, repository_not_found};
use crate::response::result::ToolData;
use octocode_github::{
    GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use serde_json::json;

/// How long a repository's canonical name is remembered.
const CANONICAL_TTL: std::time::Duration = std::time::Duration::from_secs(60);

/// The memo of a repository's canonical `owner/repo`.
fn canonical_key(owner: &str, repo: &str) -> String {
    format!("repo-canonical:{owner}/{repo}").to_ascii_lowercase()
}

/// `full_name` split into owner and repository when it names another
/// repository than `owner/repo` (case-insensitive).
fn renamed_to(owner: &str, repo: &str, full_name: &str) -> Option<(String, String)> {
    let (to_owner, to_repo) = full_name.split_once('/')?;
    let same = to_owner.eq_ignore_ascii_case(owner) && to_repo.eq_ignore_ascii_case(repo);
    (!same && !to_owner.is_empty() && !to_repo.is_empty())
        .then(|| (to_owner.to_owned(), to_repo.to_owned()))
}

/// Remember `owner/repo`'s canonical `full_name` (from repository metadata
/// a tool already read) for later calls and processes.
pub(crate) async fn remember_canonical(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    full_name: &str,
    context: &RequestContext,
) {
    if let Ok(partition) = provider.transport.cache_partition(context, None) {
        provider
            .transport
            .cache
            .put(
                &partition,
                canonical_key(owner, repo),
                octocode_github::CachedContent {
                    etag: None,
                    bytes: full_name.as_bytes().to_vec().into(),
                    resolved_ref: String::new(),
                    has_more: false,
                },
                octocode_github::Retention::Expires(CANONICAL_TTL),
            )
            .await;
    }
}

/// How [`canonical_repo`] learns a rename its memo does not hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RenameCheck {
    /// Only when a request to the repository already followed GitHub's
    /// rename redirect: free for a repository GitHub never redirected.
    Redirected,
    /// Read the repository's metadata: a search never redirects, so an
    /// empty search scoped to an old name only learns the rename here.
    Metadata,
}

/// The canonical name of a renamed repository, `None` when `owner/repo`
/// stands (case aside), does not exist, or cannot be checked
/// ([`canonical_scope`] tells a missing repository apart).
pub(crate) async fn canonical_repo(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    context: &RequestContext,
    check: RenameCheck,
) -> Result<Option<(String, String)>, ProviderError> {
    Ok(
        match canonical_scope(provider, owner, repo, context, check).await? {
            CanonicalScope::Renamed(canonical) => Some(canonical),
            CanonicalScope::Stands | CanonicalScope::Missing(_) => None,
        },
    )
}

/// What [`canonical_scope`] learned of a repository's name.
enum CanonicalScope {
    /// The name stands (case aside), or it cannot be checked.
    Stands,
    /// GitHub serves it under this `(owner, repo)`.
    Renamed((String, String)),
    /// The metadata read answered 404: the caller's not-found failure, so
    /// a later check of the same repository need not read it again.
    Missing(ProviderError),
}

/// [`canonical_repo`], telling a missing repository apart. The memo answers
/// first; past it, `check` decides whether one (memoized) metadata read is
/// spent. Only a cancelled or timed-out read fails.
async fn canonical_scope(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    context: &RequestContext,
    check: RenameCheck,
) -> Result<CanonicalScope, ProviderError> {
    let Ok(partition) = provider.transport.cache_partition(context, None) else {
        return Ok(CanonicalScope::Stands);
    };
    if let Some(cached) = provider
        .transport
        .cache
        .get(&partition, &canonical_key(owner, repo))
        .await
    {
        return Ok(std::str::from_utf8(&cached.bytes)
            .ok()
            .and_then(|full_name| renamed_to(owner, repo, full_name))
            .map_or(CanonicalScope::Stands, CanonicalScope::Renamed));
    }
    if check == RenameCheck::Redirected && !provider.transport.followed_rename(owner, repo) {
        return Ok(CanonicalScope::Stands);
    }
    let full_name = match provider
        .transport
        .repository_metadata(owner, repo, context)
        .await
    {
        Ok(meta) => meta.full_name,
        Err(error) if error.is_cancelled_or_timed_out() => {
            return Err(error);
        }
        Err(error) if error.kind == ProviderErrorKind::NotFound => {
            return Ok(CanonicalScope::Missing(
                error.with_reason(ProviderErrorReason::RepositoryNotFound),
            ));
        }
        Err(_) => None,
    };
    let Some(full_name) = full_name else {
        return Ok(CanonicalScope::Stands);
    };
    remember_canonical(provider, owner, repo, &full_name, context).await;
    Ok(renamed_to(owner, repo, &full_name).map_or(CanonicalScope::Stands, CanonicalScope::Renamed))
}

/// Whether a search scoped to one repository answered what a rename could
/// explain: no rows on any page (`empty`), or GitHub's typed "cannot be
/// searched" for a repository it could not find. Rows on another page
/// mean the scope exists.
pub(crate) fn search_may_be_renamed<T>(
    answer: &Result<T, ProviderError>,
    empty: impl FnOnce(&T) -> bool,
) -> bool {
    match answer {
        Ok(page) => empty(page),
        Err(error) => repository_not_found(error),
    }
}

/// What a scoped search learned of its repository's name
/// ([`rescope_on_rename`]).
pub(crate) enum Rescope {
    /// GitHub serves the repository as `owner/repo`; `warning` states the
    /// rename once.
    Renamed {
        owner: String,
        repo: String,
        warning: String,
    },
    /// The repository does not exist: the search's not-found failure.
    Missing(ProviderError),
    /// The name stands, or cannot be checked.
    Stands,
}

/// The rescope of a search scoped to `owner/repo` whose answer
/// [`search_may_be_renamed`] could explain. GitHub search never follows a
/// rename, so one (memoized) metadata read names the canonical repository
/// the search reruns against. Only a cancelled or timed-out read fails.
pub(crate) async fn rescope_on_rename(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    context: &RequestContext,
) -> Result<Rescope, ProviderError> {
    Ok(
        match canonical_scope(provider, owner, repo, context, RenameCheck::Metadata).await? {
            CanonicalScope::Renamed((to_owner, to_repo)) => Rescope::Renamed {
                warning: renamed_warning(owner, repo, &(to_owner.clone(), to_repo.clone())),
                owner: to_owner,
                repo: to_repo,
            },
            CanonicalScope::Missing(error) => Rescope::Missing(error),
            CanonicalScope::Stands => Rescope::Stands,
        },
    )
}

/// `query` naming the repository `owner/repo`: every GitHub tool query names
/// its repository by `owner` and `repo`, so one rewrite serves them all. A
/// name the contract rejects is a decode error.
pub(crate) fn with_repo<Q: serde::Serialize + serde::de::DeserializeOwned>(
    query: &Q,
    owner: &str,
    repo: &str,
) -> Result<Q, ProviderError> {
    let decode =
        |error: serde_json::Error| ProviderError::new(ProviderErrorKind::Decode, error.to_string());
    let mut renamed = serde_json::to_value(query).map_err(decode)?;
    renamed["owner"] = json!(owner);
    renamed["repo"] = json!(repo);
    serde_json::from_value(renamed).map_err(decode)
}

/// `query` under its renamed repository's canonical name, with the warning
/// that states the rename once; `None` when `owner/repo` stands. A read or
/// listing (which GitHub redirects) learns the rename from the redirect.
pub(crate) async fn canonical_query<Q: serde::Serialize + serde::de::DeserializeOwned>(
    provider: &GitHubProvider,
    query: &Q,
    owner: &str,
    repo: &str,
    context: &RequestContext,
) -> Result<Option<(String, Q)>, ProviderError> {
    let Some(to) = canonical_repo(provider, owner, repo, context, RenameCheck::Redirected).await?
    else {
        return Ok(None);
    };
    Ok(Some((
        renamed_warning(owner, repo, &to),
        with_repo(query, &to.0, &to.1)?,
    )))
}

/// A search scoped to `owner/repo` whose empty answer a rename could
/// explain ([`search_may_be_renamed`]), run again by `rerun` under the
/// canonical name, its output stating the rename first; `None` when the
/// name stands. A missing repository is the search's not-found failure.
pub(crate) async fn rerun_renamed<Q, F, Fut>(
    provider: &GitHubProvider,
    query: &Q,
    owner: &str,
    repo: &str,
    context: &RequestContext,
    rerun: F,
) -> Result<Option<ToolData>, ProviderError>
where
    Q: serde::Serialize + serde::de::DeserializeOwned,
    F: FnOnce(Q) -> Fut,
    Fut: std::future::Future<Output = Result<ToolData, ProviderError>>,
{
    match rescope_on_rename(provider, owner, repo, context).await? {
        Rescope::Renamed {
            owner,
            repo,
            warning,
        } => {
            let mut output = rerun(with_repo(query, &owner, &repo)?).await?;
            push_warning_front(&mut output.data, warning);
            Ok(Some(output))
        }
        // The repository check already failed: the empty page's own check
        // of it would read the same 404 again.
        Rescope::Missing(error) => Err(error),
        Rescope::Stands => Ok(None),
    }
}
/// The one warning a read states for a renamed repository.
pub(crate) fn renamed_warning(owner: &str, repo: &str, to: &(String, String)) -> String {
    format!(
        "Repository {owner}/{repo} was renamed to {}/{}; leads use the canonical name.",
        to.0, to.1
    )
}
