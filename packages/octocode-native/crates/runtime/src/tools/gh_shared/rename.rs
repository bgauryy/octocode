//! Resolve a renamed repository and rescope a read or search.
use super::{push_warning_front, repository_not_found};
use crate::response::result::ToolData;
use octocode_github::{
    GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason, RepositoryMetadata,
    RequestContext,
};
use serde_json::{Value, json};

/// `full_name` split into owner and repository when it names another
/// repository than `owner/repo` (case-insensitive).
fn renamed_to(owner: &str, repo: &str, full_name: &str) -> Option<(String, String)> {
    let (to_owner, to_repo) = full_name.split_once('/')?;
    let same = to_owner.eq_ignore_ascii_case(owner) && to_repo.eq_ignore_ascii_case(repo);
    (!same && !to_owner.is_empty() && !to_repo.is_empty())
        .then(|| (to_owner.to_owned(), to_repo.to_owned()))
}

/// How [`canonical_repo`] learns a rename.
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
            CanonicalScope::Stands(_) | CanonicalScope::Missing(_) => None,
        },
    )
}

/// What [`canonical_scope`] learned of a repository's name.
pub(super) enum CanonicalScope {
    /// The name stands (case aside), or it cannot be checked; with the
    /// metadata the check read, when it read it.
    Stands(Option<RepositoryMetadata>),
    /// GitHub serves it under this `(owner, repo)`.
    Renamed((String, String)),
    /// The metadata read answered 404: the caller's not-found failure, so
    /// a later check of the same repository need not read it again.
    Missing(ProviderError),
}

/// [`canonical_repo`], telling a missing repository apart. A metadata read
/// this request already made names the repository, and an answer GitHub
/// gave under the name without a redirect proves it stands; past them,
/// `check` decides whether metadata is needed. Only a cancelled or
/// timed-out read fails.
pub(super) async fn canonical_scope(
    provider: &GitHubProvider,
    owner: &str,
    repo: &str,
    context: &RequestContext,
    check: RenameCheck,
) -> Result<CanonicalScope, ProviderError> {
    if let Some(full_name) = context.read_full_name(owner, repo) {
        return Ok(renamed_to(owner, repo, &full_name)
            .map_or(CanonicalScope::Stands(None), CanonicalScope::Renamed));
    }
    let renamed = context.followed_rename(owner, repo);
    if (check == RenameCheck::Redirected || context.repository_resolves(owner, repo)) && !renamed {
        return Ok(CanonicalScope::Stands(None));
    }
    let metadata = match provider
        .transport
        .repository_metadata(owner, repo, context)
        .await
    {
        Ok(metadata) => metadata,
        Err(error) if error.is_cancelled_or_timed_out() => {
            return Err(error);
        }
        Err(error) if error.kind == ProviderErrorKind::NotFound => {
            return Ok(CanonicalScope::Missing(
                error.with_reason(ProviderErrorReason::RepositoryNotFound),
            ));
        }
        Err(_) => return Ok(CanonicalScope::Stands(None)),
    };
    match metadata
        .full_name
        .as_deref()
        .and_then(|full_name| renamed_to(owner, repo, full_name))
    {
        Some(canonical) => Ok(CanonicalScope::Renamed(canonical)),
        None => Ok(CanonicalScope::Stands(Some(metadata))),
    }
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
    /// The name stands, or cannot be checked; with the metadata the check
    /// read, so the search's own scope check need not read it again.
    Stands(Option<RepositoryMetadata>),
}

/// What [`rerun_renamed`] made of a search a rename could explain.
pub(crate) enum Rerun {
    /// The search's output under the canonical name, stating the rename.
    Renamed(ToolData),
    /// The name stands: the search's own answer is final; with the
    /// metadata the check read, when it read it.
    Stands(Option<RepositoryMetadata>),
}

/// The rescope of a search scoped to `owner/repo` whose answer
/// [`search_may_be_renamed`] could explain. GitHub search never follows a
/// rename, so one metadata read names the canonical repository
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
            CanonicalScope::Stands(metadata) => Rescope::Stands(metadata),
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

/// Every call in `data` that names the repository `owner/repo` (case aside)
/// names `to` instead: each query row of each `input.queries`, wherever the
/// row holds the call. A call that names another repository, and a cursor
/// (which holds no row), stand.
fn lead_canonically(data: &mut Value, owner: &str, repo: &str, to: &(String, String)) {
    let names = |row: &Value, field: &str, name: &str| {
        row.get(field)
            .and_then(Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case(name))
    };
    let mut pending = vec![data];
    while let Some(value) = pending.pop() {
        match value {
            Value::Array(items) => pending.extend(items),
            Value::Object(fields) => {
                let rows = fields
                    .get_mut("input")
                    .and_then(|input| input.get_mut("queries"))
                    .and_then(Value::as_array_mut);
                for row in rows.into_iter().flatten() {
                    if names(row, "owner", owner) && names(row, "repo", repo) {
                        row["owner"] = json!(to.0);
                        row["repo"] = json!(to.1);
                    }
                }
                pending.extend(fields.values_mut());
            }
            _ => {}
        }
    }
}

/// The repository a finished row of `owner/repo` is named by: `Some` when
/// GitHub answered the row's requests through a rename redirect, with every
/// call the row holds renamed to it ([`lead_canonically`]) and the rename
/// stated once, first. For a read whose leads are built while it fetches,
/// so no canonical query exists before them ([`canonical_query`] is that
/// point for a read that fetches first). A repository GitHub never
/// redirected costs nothing.
pub(crate) async fn name_canonically(
    provider: &GitHubProvider,
    data: &mut Value,
    owner: &str,
    repo: &str,
    context: &RequestContext,
) -> Result<Option<(String, String)>, ProviderError> {
    let Some(to) = canonical_repo(provider, owner, repo, context, RenameCheck::Redirected).await?
    else {
        return Ok(None);
    };
    lead_canonically(data, owner, repo, &to);
    push_warning_front(data, renamed_warning(owner, repo, &to));
    Ok(Some(to))
}

/// A search scoped to `owner/repo` whose empty answer a rename could
/// explain ([`search_may_be_renamed`]), run again by `rerun` under the
/// canonical name, its output stating the rename first ([`Rerun`]). A
/// missing repository is the search's not-found failure.
pub(crate) async fn rerun_renamed<Q, F, Fut>(
    provider: &GitHubProvider,
    query: &Q,
    owner: &str,
    repo: &str,
    context: &RequestContext,
    rerun: F,
) -> Result<Rerun, ProviderError>
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
            Ok(Rerun::Renamed(output))
        }
        // The repository check already failed: the empty page's own check
        // of it would read the same 404 again.
        Rescope::Missing(error) => Err(error),
        Rescope::Stands(metadata) => Ok(Rerun::Stands(metadata)),
    }
}
/// The one warning a read states for a renamed repository.
pub(crate) fn renamed_warning(owner: &str, repo: &str, to: &(String, String)) -> String {
    format!(
        "Repository {owner}/{repo} was renamed to {}/{}; leads use the canonical name.",
        to.0, to.1
    )
}
