//! Pieces every GitHub tool shares: the failure a tool states, search paging,
//! and the missing-path recovery of ghStructure and ghGetFileContent.
pub(crate) mod query;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;

use crate::providers::github::{
    ConditionalCache, CredentialResolver, GitHubProvider, ProviderError, ProviderErrorKind,
    ProviderErrorReason, RequestContext,
};
use crate::tools::id::ToolId;
use crate::tools::result::{Continuation, remove_null_fields};
use serde_json::{Value, json};

/// Hints are rendered whole only up to the response guidance limit (120
/// chars); keep every GitHub recovery hint within it.
pub(crate) const GITHUB_AUTH_RECOVERY_HINT: &str = "Run octocode auth login or set GH_TOKEN/GITHUB_TOKEN; an invalid env token overrides stored login.";

/// A repository that did not resolve: GitHub answers a private repository
/// the token cannot see exactly like a missing one.
pub(crate) const REPOSITORY_ACCESS_HINT: &str = "The repository is missing, private, or hidden from this token; check owner/repo spelling and token access.";

/// A GitHub provider failure as the tool states it: its message, hints and
/// recovery `next`; the runtime adds the provider's status and rate-limit
/// metadata.
#[derive(Debug)]
pub struct GhFailure {
    pub error: ProviderError,
    pub message: String,
    pub hints: Vec<String>,
    pub next: Option<Value>,
    /// Fields the error row names besides the error (a file's identity).
    pub fields: Vec<(&'static str, Value)>,
}

impl GhFailure {
    pub(crate) fn new(error: ProviderError, message: impl Into<String>) -> Self {
        Self {
            error,
            message: message.into(),
            hints: Vec::new(),
            next: None,
            fields: Vec::new(),
        }
    }

    pub(crate) fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hints = vec![hint.into()];
        self
    }
}

pub(crate) fn repository_not_found(error: &ProviderError) -> bool {
    error.reason == Some(ProviderErrorReason::RepositoryNotFound)
}

/// The message every GitHub tool states for a provider failure its own
/// not-found, ref and validation wording does not cover: authentication, an
/// unresolved repository, a missing resource, an outage, a network failure or
/// timeout, else the provider's own message.
pub(crate) fn provider_message(error: &ProviderError) -> String {
    match error.kind {
        ProviderErrorKind::Authentication => "GitHub authentication required".to_owned(),
        ProviderErrorKind::NotFound if repository_not_found(error) => {
            "Repository not found, or private and not accessible to this token".to_owned()
        }
        ProviderErrorKind::NotFound => "Repository, resource, or path not found".to_owned(),
        ProviderErrorKind::Server if matches!(error.status, Some(502..=504)) => {
            "GitHub API temporarily unavailable".to_owned()
        }
        ProviderErrorKind::Transport => "Network connection failed".to_owned(),
        ProviderErrorKind::Timeout => "Request timeout".to_owned(),
        _ => error.message.to_string(),
    }
}

/// A GitHub 422 as `summary: detail`, keeping GitHub's own detail (e.g.
/// `"abc" is not a numeric value`): it names the bad input.
pub(crate) fn validation_message(error: &ProviderError, summary: &str) -> String {
    let detail = error.message.trim();
    let detail = detail.strip_prefix("Validation Failed: ").unwrap_or(detail);
    if detail.is_empty() || detail.eq_ignore_ascii_case("Validation Failed") {
        summary.to_owned()
    } else {
        format!("{summary}: {detail}")
    }
}

/// The hint every GitHub tool gives for a failure whose recovery does not
/// depend on the tool: authenticate, or check the repository's access.
/// Other failures take the runtime's one hint per error kind.
pub(crate) fn provider_hint(error: &ProviderError) -> Option<&'static str> {
    if error.kind == ProviderErrorKind::Authentication {
        Some(GITHUB_AUTH_RECOVERY_HINT)
    } else if repository_not_found(error) {
        Some(REPOSITORY_ACCESS_HINT)
    } else {
        None
    }
}

/// The failure of a search tool (ghSearchRepo, ghSearchCode, ghStructure).
/// `window_hint` names the tool's own filters that reach results past the
/// 1,000-result search window.
pub(crate) fn search_failure(error: ProviderError, window_hint: &str) -> GhFailure {
    let message = match error.kind {
        // A typed missing ref names the ref it could not resolve.
        ProviderErrorKind::NotFound if error.reason == Some(ProviderErrorReason::RefNotFound) => {
            error.message.to_string()
        }
        ProviderErrorKind::Validation if error.status == Some(422) => {
            validation_message(&error, "Invalid search query or request parameters")
        }
        _ => provider_message(&error),
    };
    let hint = if let Some(hint) = provider_hint(&error) {
        Some(hint)
    } else if error.reason == Some(ProviderErrorReason::RefNotFound) {
        Some("Verify the branch, tag, or SHA exists, or omit ref to use the default branch.")
    } else if error.reason == Some(ProviderErrorReason::SearchWindowExceeded) {
        Some(window_hint)
    } else {
        None
    };
    let failure = GhFailure::new(error, message);
    match hint {
        Some(hint) => failure.hint(hint),
        None => failure,
    }
}

/// The hint of a ref that did not resolve; `hints.viewStructure` lists the
/// repository's branches and tags.
pub(crate) const REF_RECOVERY_HINT: &str = "Verify the branch, tag, or SHA (hints.viewStructure lists refs), or omit ref for the default branch.";

/// A ref that did not resolve: ghStructure `operation:"refs"` lists the
/// branches and tags it could name.
pub(crate) fn ref_recovery(owner: &str, repo: &str) -> Value {
    Continuation::new(
        ToolId::GhStructure,
        json!({"owner": owner, "repo": repo, "operation": "refs"}),
    )
    .confidence("high")
    .build()
}

/// A missing ref's failure gets the runnable refs listing and its hint;
/// any other failure is returned unchanged.
pub(crate) fn with_ref_recovery(mut failure: GhFailure, owner: &str, repo: &str) -> GhFailure {
    if failure.error.reason != Some(ProviderErrorReason::RefNotFound) {
        return failure;
    }
    let mut next = failure.next.take().unwrap_or_else(|| json!({}));
    next["viewStructure"] = ref_recovery(owner, repo);
    failure.next = Some(next);
    failure.hint(REF_RECOVERY_HINT)
}

/// The memo of a repository's canonical `owner/repo` (volatile cache class).
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
pub(crate) async fn remember_canonical<R: CredentialResolver, C: ConditionalCache>(
    provider: &GitHubProvider<R, C>,
    owner: &str,
    repo: &str,
    full_name: &str,
    context: &RequestContext,
) {
    if let Ok(partition) = provider.transport.cache_partition(context, None).await {
        provider
            .cache
            .put(
                &partition,
                canonical_key(owner, repo),
                crate::providers::github::CachedContent {
                    etag: None,
                    bytes: full_name.as_bytes().to_vec(),
                    resolved_ref: full_name.to_owned(),
                },
            )
            .await;
    }
}

/// The canonical name of a renamed repository, `None` when `owner/repo`
/// stands. Costs no request for a repository GitHub never redirected: the
/// memo answers first, then a followed rename redirect triggers one
/// metadata read, which is memoized.
pub(crate) async fn canonical_repo<R: CredentialResolver, C: ConditionalCache>(
    provider: &GitHubProvider<R, C>,
    owner: &str,
    repo: &str,
    context: &RequestContext,
) -> Option<(String, String)> {
    let partition = provider
        .transport
        .cache_partition(context, None)
        .await
        .ok()?;
    if let Some(cached) = provider
        .cache
        .get(&partition, &canonical_key(owner, repo))
        .await
    {
        return renamed_to(owner, repo, std::str::from_utf8(&cached.bytes).ok()?);
    }
    if !provider.transport.followed_rename(owner, repo) {
        return None;
    }
    let full_name = provider
        .transport
        .repository_metadata(owner, repo, context)
        .await
        .ok()?
        .full_name?;
    remember_canonical(provider, owner, repo, &full_name, context).await;
    renamed_to(owner, repo, &full_name)
}

/// The one warning a read states for a renamed repository.
pub(crate) fn renamed_warning(owner: &str, repo: &str, to: &(String, String)) -> String {
    format!(
        "Repository {owner}/{repo} was renamed to {}/{}; leads use the canonical name.",
        to.0, to.1
    )
}

/// Results GitHub search reaches for one query, however many match.
pub(crate) const SEARCH_RESULT_CAP: usize = 1000;

/// A search page past GitHub's 1,000-result window cannot be read.
pub(crate) fn reject_window(page: usize, per: usize) -> Result<(), ProviderError> {
    if (page - 1).saturating_mul(per) >= SEARCH_RESULT_CAP {
        Err(ProviderError::new(
            ProviderErrorKind::Validation,
            "GitHub search page exceeds the 1,000-result search window",
        )
        .with_reason(ProviderErrorReason::SearchWindowExceeded))
    } else {
        Ok(())
    }
}

/// `next.nextPage`: the same query at the page after `page`; the contract
/// `page` maximum is the last a search can name, so past it the page is a
/// terminal limit.
pub(crate) fn add_next(
    value: &mut Value,
    tool: ToolId,
    query: &impl serde::Serialize,
    page: usize,
    has_more: bool,
) {
    if !has_more {
        return;
    }
    if page >= crate::contracts::query_schema_max(tool, None, "page") {
        value["terminalLimit"] = json!(true);
        return;
    }
    let mut next = serde_json::to_value(query).unwrap_or_default();
    remove_null_fields(&mut next);
    next["page"] = json!(page + 1);
    value["next"] = json!({"nextPage": Continuation::new(tool, next).confidence("exact").build()});
}

/// Provider-side incompleteness of a search page: the 1,000-result cap and
/// GitHub's own incomplete-results flag (with a `retry` of the same page).
#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_partial(
    value: &mut Value,
    tool: ToolId,
    query: &impl serde::Serialize,
    incomplete: bool,
    capped: bool,
    page: usize,
    has_more: bool,
    subject: &str,
) {
    let mut reasons = Vec::new();
    if capped {
        reasons.push("providerResultCap");
        // terminalLimit means no executable continuation remains: only the
        // last reachable page of a capped search ends coverage.
        if !has_more {
            value["terminalLimit"] = json!(true);
        }
    }
    if incomplete {
        reasons.push("providerIncompleteResults");
        let mut retry = serde_json::to_value(query).unwrap_or_default();
        remove_null_fields(&mut retry);
        retry["page"] = json!(page);
        value["next"]["retry"] = Continuation::new(tool, retry)
            .why(format!(
                "Retry the same {subject} provider page because the provider reported incomplete results."
            ))
            .confidence("exact")
            .build();
    }
    if !reasons.is_empty() {
        value["isPartial"] = json!(true);
        value["partialReasons"] = json!(reasons);
    }
}

/// A non-empty repository-relative path that stays below the directory it is
/// joined to: no root, drive, `.`/`..` segment or backslash.
pub(crate) fn is_repo_relative(path: &str) -> bool {
    let value = std::path::Path::new(path);
    !path.trim().is_empty()
        && !path.contains('\\')
        && !value.is_absolute()
        && value
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

/// A read or listing whose path (not repository or ref) did not resolve.
pub(crate) fn missing_path(error: &ProviderError, path: Option<&str>) -> bool {
    error.kind == ProviderErrorKind::NotFound
        && !repository_not_found(error)
        && error.reason != Some(ProviderErrorReason::RefNotFound)
        && !error.message.starts_with("No commit found")
        && path.is_some_and(|path| !path.trim_matches('/').is_empty())
}

/// Directory listings a missing-path recovery may spend.
const PATH_RECOVERY_LISTINGS: usize = 6;

/// Where a missing path's nearest existing parts are.
pub(crate) struct PathRecovery {
    /// Deepest existing directory on the requested path (case-corrected).
    pub(crate) directory: String,
    /// The requested file itself when only its case differed.
    pub(crate) file: Option<String>,
}

/// The repository coordinates a missing-path recovery lists under.
pub(crate) struct RepoPath<'a> {
    pub(crate) owner: &'a str,
    pub(crate) repo: &'a str,
    pub(crate) path: &'a str,
    pub(crate) reference: Option<&'a str>,
}

/// Walk up to the nearest directory that exists, then back down matching each
/// remaining segment case-insensitively, within [`PATH_RECOVERY_LISTINGS`].
pub(crate) async fn locate_path<R: CredentialResolver, C: ConditionalCache>(
    provider: &GitHubProvider<R, C>,
    at: &RepoPath<'_>,
    context: &RequestContext,
) -> Option<PathRecovery> {
    let (owner, repo) = (at.owner, at.repo);
    let segments: Vec<&str> = at.path.split('/').filter(|part| !part.is_empty()).collect();
    let reference = provider
        .resolve_reference(owner, repo, at.reference, false, context)
        .await
        .ok()?;
    let mut listings = 0;
    let mut depth = segments.len().saturating_sub(1);
    let mut listing = loop {
        listings += 1;
        let directory = segments[..depth].join("/");
        match provider
            .repository_contents(owner, repo, &directory, &reference, context)
            .await
        {
            Ok(listing) => break listing,
            Err(error)
                if error.kind == ProviderErrorKind::NotFound
                    && depth > 0
                    && listings < PATH_RECOVERY_LISTINGS =>
            {
                depth -= 1;
            }
            Err(_) => return None,
        }
    };
    let mut directory = segments[..depth].join("/");
    for (index, wanted) in segments.iter().enumerate().skip(depth) {
        let matches: Vec<_> = listing
            .entries
            .iter()
            .filter(|entry| entry.name.eq_ignore_ascii_case(wanted))
            .collect();
        let entry = match matches.as_slice() {
            [entry] => *entry,
            many => match many.iter().find(|entry| entry.name == *wanted) {
                Some(entry) => *entry,
                None => break,
            },
        };
        let (kind, path) = (entry.kind.clone(), entry.path.clone());
        let last = index + 1 == segments.len();
        if last && kind == "file" {
            return Some(PathRecovery {
                directory,
                file: Some(path),
            });
        }
        if kind != "dir" {
            break;
        }
        if last {
            directory = path;
            break;
        }
        if listings >= PATH_RECOVERY_LISTINGS {
            break;
        }
        listings += 1;
        match provider
            .repository_contents(owner, repo, &path, &reference, context)
            .await
        {
            Ok(next) => {
                listing = next;
                directory = path;
            }
            Err(_) => break,
        }
    }
    Some(PathRecovery {
        directory,
        file: None,
    })
}

/// The parent directory of a repository path, `.` at the root.
pub(crate) fn parent_dir(path: &str) -> String {
    std::path::Path::new(path.trim_matches('/'))
        .parent()
        .map(|path| path.to_string_lossy().into_owned())
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| ".".into())
}

/// A ghStructure listing of `path` at the caller's ref (and depth, when the
/// caller set one): a fresh query from page 1, not a page of the failed call.
pub(crate) fn tree_recovery(at: &RepoPath<'_>, max_depth: Option<i64>, confidence: &str) -> Value {
    let mut tree_query = json!({"owner": at.owner, "repo": at.repo, "path": at.path});
    if let Some(reference) = at.reference {
        tree_query["ref"] = json!(reference);
    }
    if let Some(depth) = max_depth {
        tree_query["maxDepth"] = json!(depth);
    }
    Continuation::new(ToolId::GhStructure, tree_query)
        .confidence(confidence)
        .build()
}
