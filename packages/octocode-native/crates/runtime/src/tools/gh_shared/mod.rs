//! Pieces every GitHub tool shares: the failure a tool states, search paging,
//! an empty search's broadening, a renamed repository's rescope, and the
//! missing-path recovery of ghStructure and ghGetFileContent.
pub(crate) mod commit;
mod default_branch;
pub(crate) mod query;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;

use crate::id::ToolId;
use crate::response::paging::push_warnings;
use crate::response::result::Confidence;
use crate::response::result::{Continuation, ToolData, query_row};
pub(crate) use default_branch::{
    DefaultBranchRetry, default_branch, default_branch_retry, repo_key, unresolved_refs,
    with_default_branch,
};
use octocode_github::{
    GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason, RequestContext,
};
use serde_json::{Value, json};

/// A SAML-protected organization refused the token: the row's `ssoUrl` is
/// where it is authorized.
pub(crate) const SSO_AUTHORIZATION_HINT: &str =
    "Authorize this token for the organization (SAML SSO) at the row's ssoUrl, then retry.";

/// A search qualifier named a login GitHub cannot resolve.
pub(crate) const USER_QUALIFIER_HINT: &str = "A login in a user qualifier (author, assignee, reviewed-by, …) does not resolve: fix its spelling or drop it.";

/// A repository that did not resolve: GitHub answers a private repository
/// the token cannot see exactly like a missing one (the error says so); the
/// hint is the action.
pub(crate) const REPOSITORY_ACCESS_HINT: &str =
    "Check the owner/repo spelling and this token's access.";

/// A GitHub provider failure as the tool states it: its message, hints and
/// recovery `next`; the runtime adds the provider's status and rate-limit
/// metadata.
#[derive(Debug)]
pub struct GhFailure {
    /// The error and the recovery lead are boxed: they are most of the
    /// failure's size, and every GitHub tool returns it on its error path.
    pub error: Box<ProviderError>,
    pub message: String,
    pub hints: Vec<String>,
    pub next: Option<Box<Value>>,
    /// Fields the error row names besides the error (a file's identity).
    pub fields: Vec<(&'static str, Value)>,
}

impl GhFailure {
    pub(crate) fn new(error: ProviderError, message: impl Into<String>) -> Self {
        Self {
            error: Box::new(error),
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
        // GitHub's own words (`Bad credentials`) say what is wrong with the token.
        ProviderErrorKind::Authentication if error.status == Some(401) => {
            let detail = error.message.trim();
            if detail.is_empty() {
                "GitHub authentication required".to_owned()
            } else {
                format!("GitHub authentication required: {detail}")
            }
        }
        ProviderErrorKind::Authentication => "GitHub authentication required".to_owned(),
        ProviderErrorKind::NotFound if repository_not_found(error) => {
            "Repository not found, or private and not accessible to this token".to_owned()
        }
        ProviderErrorKind::NotFound if error.reason == Some(ProviderErrorReason::UserNotFound) => {
            // GitHub's own words say which logins failed to resolve.
            format!(
                "A user named in the search qualifiers does not exist or is not visible to this token. GitHub: {}",
                error
                    .message
                    .trim()
                    .strip_prefix("Validation Failed: ")
                    .unwrap_or(error.message.trim())
            )
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
    if error.sso_url.is_some() {
        Some(SSO_AUTHORIZATION_HINT)
    } else if error.kind == ProviderErrorKind::Authentication {
        Some(crate::response::rows::GITHUB_AUTH_RECOVERY_HINT)
    } else if repository_not_found(error) {
        Some(REPOSITORY_ACCESS_HINT)
    } else if error.reason == Some(ProviderErrorReason::UserNotFound) {
        Some(USER_QUALIFIER_HINT)
    } else {
        None
    }
}

/// The failure of a search tool (ghSearchRepo, ghSearchCode, ghStructure).
/// `window_hint` names the tool's own filters that reach results past the
/// 1,000-result search window.
pub(crate) fn search_failure(error: ProviderError, window_hint: Option<&str>) -> GhFailure {
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
    } else if error.reason == Some(ProviderErrorReason::SearchWindowExceeded) {
        window_hint
    } else {
        None
    };
    let failure = GhFailure::new(error, message);
    match hint {
        Some(hint) => failure.hint(hint),
        None => failure,
    }
}

/// A ref that did not resolve: ghStructure `operation:"refs"` lists the
/// branches and tags it could name.
pub(crate) fn ref_recovery(owner: &str, repo: &str) -> Value {
    let mut refs = RepoPath::root(owner, repo).row();
    refs["operation"] = json!("refs");
    Continuation::new(ToolId::GhStructure, refs)
        .confidence(Confidence::High)
        .build()
}

/// A repository that did not resolve: a search for its name names the
/// owner that holds it.
pub(crate) fn repository_search(repo: &str) -> Value {
    Continuation::new(ToolId::GhSearchRepo, json!({"keywords": [repo]}))
        .confidence(Confidence::Medium)
        .build()
}

/// The error for an owner-wide search that found no such login. It names the
/// login; the caller adds the spelling hint only where one helps.
pub(crate) fn owner_not_found_message(owner: &str) -> String {
    format!("Owner \"{owner}\" not found: no GitHub user or organization has this login")
}

/// A missing ref's failure gets the runnable refs listing (the lead is the
/// fix: no hint beside it); any other failure is returned unchanged.
pub(crate) fn with_ref_recovery(mut failure: GhFailure, owner: &str, repo: &str) -> GhFailure {
    if failure.error.reason != Some(ProviderErrorReason::RefNotFound) {
        return failure;
    }
    let mut next = failure.next.take().unwrap_or_else(|| Box::new(json!({})));
    next["viewRefs"] = ref_recovery(owner, repo);
    failure.next = Some(next);
    failure
}

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
    let confidence = if pinned.is_some() {
        Confidence::High
    } else {
        Confidence::Low
    };
    failure.hints.clear();
    failure.next = Some(Box::new(
        json!({ "viewTree": tree_recovery(&at, None, confidence) }),
    ));
    failure
}

/// The one warning a read states for a renamed repository.
pub(crate) fn renamed_warning(owner: &str, repo: &str, to: &(String, String)) -> String {
    format!(
        "Repository {owner}/{repo} was renamed to {}/{}; leads use the canonical name.",
        to.0, to.1
    )
}

/// Keywords are ANDed literal terms: a bare GitHub boolean operator (`OR`,
/// `NOT`, `AND`, as GitHub spells them) standing as a word of a keyword
/// would be searched as a word (quoted into a phrase) and silently empty
/// the search, so it is refused, naming the operator and the one way to ask
/// for alternatives.
pub(crate) fn reject_boolean_keywords<S: AsRef<str>>(keywords: &[S]) -> Result<(), ProviderError> {
    let operator = keywords
        .iter()
        .flat_map(|keyword| keyword.as_ref().split_whitespace())
        .find_map(|word| ["OR", "NOT", "AND"].into_iter().find(|op| word == *op));
    match operator {
        Some(operator) => Err(ProviderError::new(
            ProviderErrorKind::Validation,
            format!(
                "keywords are ANDed literal terms; {operator} is no operator here: run one search per alternative"
            ),
        )),
        None => Ok(()),
    }
}

/// Results GitHub search reaches for one query, however many match.
pub(crate) const SEARCH_RESULT_CAP: usize = 1000;

/// Where a GitHub search endpoint draws its 1,000-result line through the
/// pages of one query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchWindow {
    /// A page is served while its first result lies inside the cap, so the
    /// last page may run past it: repository, issue and commit search.
    PageStart,
    /// A page is served only while its last result lies inside the cap
    /// (`page * per <= 1000`): code search. When `per` does not divide
    /// 1,000, the results past the last whole page are unreachable.
    PageEnd,
}

impl SearchWindow {
    /// The last page GitHub serves at `per` rows a page.
    pub(crate) fn last_page(self, per: usize) -> usize {
        let per = per.max(1);
        match self {
            Self::PageStart => SEARCH_RESULT_CAP.div_ceil(per),
            Self::PageEnd => (SEARCH_RESULT_CAP / per).max(1),
        }
    }

    /// What `matched` results reach through this window at `per` a page.
    pub(crate) fn reach(self, matched: usize, per: usize) -> SearchReach {
        let per = per.max(1);
        let total = matched.min(self.last_page(per).saturating_mul(per));
        SearchReach {
            total,
            pages: total.div_ceil(per),
            capped: matched > total,
        }
    }

    /// A page past the last one GitHub serves cannot be read.
    pub(crate) fn reject(self, page: usize, per: usize) -> Result<(), ProviderError> {
        let last = self.last_page(per);
        if page > last {
            Err(ProviderError::new(
                ProviderErrorKind::Validation,
                format!(
                    "Page {page} is past GitHub's search window: at {per} per page, page {last} is the last, reaching the first {} results.",
                    last.saturating_mul(per.max(1))
                ),
            )
            .with_reason(ProviderErrorReason::SearchWindowExceeded))
        } else {
            Ok(())
        }
    }
}

/// What one search's match count reaches through GitHub's 1,000-result
/// window at `per` rows a page.
#[derive(Clone, Copy)]
pub(crate) struct SearchReach {
    /// Reachable results.
    pub(crate) total: usize,
    pub(crate) pages: usize,
    /// More results matched than the window reaches.
    pub(crate) capped: bool,
}

impl SearchReach {
    /// Whether page `current` lies past the last page of a non-empty search.
    pub(crate) fn past_last(&self, current: usize) -> bool {
        self.total > 0 && current > self.pages
    }
}

/// A search failure's one repair lead, replacing its static hint: a page
/// past the search window leads to the last page a
/// [`SearchWindow::PageStart`] search serves (`repair`; a code search
/// re-points it at its own last page); a
/// repository that did not resolve leads to a search for its name
/// (`findRepository`), which names the owner that holds it. Any other
/// failure is returned unchanged.
pub(crate) fn with_search_repair(
    mut failure: GhFailure,
    tool: ToolId,
    query: &impl serde::Serialize,
    per: usize,
    repo: Option<&str>,
) -> GhFailure {
    let (name, lead) = if failure.error.reason == Some(ProviderErrorReason::SearchWindowExceeded) {
        (
            "repair",
            page_replay(tool, query, SearchWindow::PageStart.last_page(per)),
        )
    } else if let Some(repo) = repo.filter(|_| repository_not_found(&failure.error)) {
        ("findRepository", repository_search(repo))
    } else {
        return failure;
    };
    failure.hints.clear();
    let mut next = failure.next.take().unwrap_or_else(|| Box::new(json!({})));
    next[name] = lead;
    failure.next = Some(next);
    failure
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
    let Some(next_page) = crate::response::paging::next_cursor(value, tool, None, "page", page + 1)
    else {
        return;
    };
    value["next"] = json!({"nextPage": page_replay(tool, query, next_page)});
}

/// The same query at `page`: an exact replay.
fn page_replay(tool: ToolId, query: &impl serde::Serialize, page: usize) -> Value {
    let mut row = query_row(query, &[]);
    row["page"] = json!(page);
    Continuation::new(tool, row)
        .confidence(Confidence::Exact)
        .build()
}

/// Keys no broadened variant copies: where the walk stands, the output
/// options and the brief. A variant is a fresh search from its first page.
const BROADEN_STRIP: &[&str] = &["page", "pageSize", "extras", "mainGoal", "reasoning"];

/// What one empty search can drop to broaden: the tool's own keys a
/// variant never copies (beside [`BROADEN_STRIP`]), and its narrowing
/// filters, each group dropped whole (`since` and `until` are one window).
pub(crate) struct Broaden<'a> {
    pub(crate) strip: &'a [&'a str],
    pub(crate) groups: &'a [&'a [&'a str]],
}

/// An empty search's broader variants as one low-confidence batch lead, so
/// the variant that returns rows names what emptied the search: `row` (the
/// search as a wire row) once per filter group it sets, then once per
/// dropped keyword, the last first (the first usually names the subject); a
/// lone keyword drops whole. A variant the tool's contract rejects (nothing
/// left to search by) is skipped, and at most `limit` are kept.
pub(crate) fn broaden(
    tool: ToolId,
    row: &Value,
    drop: &Broaden<'_>,
    limit: usize,
) -> Option<Value> {
    let mut base = row.clone();
    if let Some(object) = base.as_object_mut() {
        for key in BROADEN_STRIP.iter().chain(drop.strip) {
            object.remove(*key);
        }
    }
    let without = |keys: &[&str]| {
        let mut variant = base.clone();
        if let Some(object) = variant.as_object_mut() {
            for key in keys {
                object.remove(*key);
            }
        }
        variant
    };
    let mut variants = drop
        .groups
        .iter()
        .filter(|group| group.iter().any(|key| base.get(*key).is_some()))
        .map(|group| without(group))
        .collect::<Vec<_>>();
    let keywords = base["keywords"].as_array().map_or(&[][..], Vec::as_slice);
    match keywords {
        [] => {}
        [_] => variants.push(without(&["keywords"])),
        many => variants.extend((0..many.len()).rev().map(|dropped| {
            let mut variant = base.clone();
            variant["keywords"] = many
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != dropped)
                .map(|(_, keyword)| keyword.clone())
                .collect();
            variant
        })),
    }
    let variants = variants
        .into_iter()
        .filter(|variant| {
            crate::contracts::validate(tool.as_str(), json!({ "queries": [variant] })).is_ok()
        })
        .take(limit)
        .collect::<Vec<_>>();
    (!variants.is_empty()).then(|| {
        Continuation::input(tool, json!({ "queries": variants }))
            .confidence(Confidence::Low)
            .build()
    })
}

/// What GitHub said about one search page's completeness.
#[derive(Clone, Copy)]
pub(crate) struct SearchCompleteness<'a> {
    pub(crate) tool: ToolId,
    /// GitHub's own match count (past the cap, more than `reachable`).
    pub(crate) matched: usize,
    /// The results the search window reaches (`totalItems`).
    pub(crate) reachable: usize,
    /// GitHub's incomplete-results flag.
    pub(crate) incomplete: bool,
    pub(crate) page: usize,
    pub(crate) has_more: bool,
    /// What one result is (`code`, `repository`), for the count warning.
    pub(crate) subject: &'a str,
}

/// Provider-side incompleteness of a search page: the 1,000-result cap and
/// GitHub's own incomplete-results flag (with a `retry` of the same page).
/// Past the cap the first page states GitHub's match count, since the
/// reachable `totalItems` is not the count. Only the last reachable page of
/// a capped search ends coverage (`terminalLimit`: no executable
/// continuation remains).
pub(crate) fn apply_partial(
    value: &mut Value,
    query: &impl serde::Serialize,
    facts: &SearchCompleteness<'_>,
) {
    use crate::response::paging::{mark_partial, terminal_limit};
    let SearchCompleteness {
        tool,
        matched,
        reachable,
        incomplete,
        page,
        has_more,
        subject,
    } = *facts;
    if matched > reachable {
        // The match count is this call's fact; that search stops at its
        // window is the cap reason (`providerResultCap`), not a per-call
        // note.
        let count = (page == 1).then(|| {
            format!(
                "GitHub matched {matched} {subject} results; the first {reachable} are reachable."
            )
        });
        let warnings = Vec::from_iter(count);
        if has_more {
            mark_partial(value, "providerResultCap", &warnings);
        } else {
            terminal_limit(value, "providerResultCap", &warnings);
        }
    }
    if incomplete {
        mark_partial(value, "providerIncompleteResults", &[]);
        value["next"]["retry"] = page_replay(tool, query, page);
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

/// A place in a repository: its coordinates, a path (`""` for the root)
/// and a ref (the default branch when `None`). Recoveries list under it,
/// and every cross-tool lead to a repository starts at one.
pub(crate) struct RepoPath<'a> {
    pub(crate) owner: &'a str,
    pub(crate) repo: &'a str,
    pub(crate) path: &'a str,
    pub(crate) reference: Option<&'a str>,
}

impl<'a> RepoPath<'a> {
    /// The repository's root on its default branch.
    pub(crate) fn root(owner: &'a str, repo: &'a str) -> Self {
        Self {
            owner,
            repo,
            path: "",
            reference: None,
        }
    }

    /// The query row of this place: `owner`, `repo`, then `path` and `ref`
    /// when set.
    pub(crate) fn row(&self) -> Value {
        let mut row = json!({"owner": self.owner, "repo": self.repo});
        if !self.path.is_empty() {
            row["path"] = json!(self.path);
        }
        if let Some(reference) = self.reference {
            row["ref"] = json!(reference);
        }
        row
    }

    /// A lead to `tool` (ghStructure, ghCloneRepo) that starts here.
    pub(crate) fn lead(&self, tool: ToolId) -> Continuation {
        Continuation::new(tool, self.row())
    }
}

/// Walk up to the nearest directory that exists, then back down matching each
/// remaining segment case-insensitively, within [`PATH_RECOVERY_LISTINGS`].
pub(crate) async fn locate_path(
    provider: &GitHubProvider,
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
    let normal = repo_path(path);
    std::path::Path::new(&normal)
        .parent()
        .map(|path| path.to_string_lossy().into_owned())
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| ".".into())
}

/// `path`'s segments with empty, `.` and `..` ones resolved lexically, and
/// whether a `..` climbed above the repository root (and was dropped).
fn resolved_segments(path: &str) -> (Vec<&str>, bool) {
    let mut segments: Vec<&str> = Vec::new();
    let mut escaped = false;
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => escaped |= segments.pop().is_none(),
            segment => segments.push(segment),
        }
    }
    (segments, escaped)
}

/// A repository path with its `.` and `..` segments resolved lexically and
/// clamped at the repository root (a tree has nothing above it); `.` for
/// the root. A recovery lead never repeats a traversal the caller sent.
pub(crate) fn repo_path(path: &str) -> String {
    let (segments, _) = resolved_segments(path);
    if segments.is_empty() {
        ".".into()
    } else {
        segments.join("/")
    }
}

/// A requested repository path (a listed directory, a read file) as the
/// repository names it: `.`, `..`, repeated and edge slashes resolved
/// lexically, `""` for the root. A `..` that climbs above the root names
/// nothing a repository holds: invalid input.
pub(crate) fn listing_path(path: &str) -> Result<String, ProviderError> {
    match resolved_segments(path) {
        (_, true) => Err(ProviderError::new(
            ProviderErrorKind::Validation,
            format!("Path \"{path}\" climbs above the repository root."),
        )),
        (segments, false) => Ok(segments.join("/")),
    }
}

/// A ghStructure listing of `path` at the caller's ref (and depth, when the
/// caller set one): a fresh query from page 1, not a page of the failed call.
pub(crate) fn tree_recovery(
    at: &RepoPath<'_>,
    max_depth: Option<i64>,
    confidence: Confidence,
) -> Value {
    let mut tree_query = json!({"owner": at.owner, "repo": at.repo, "path": repo_path(at.path)});
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

/// `fields` row pick (gh `--json a,b`): the row's `id` keys, then the
/// picked keys in request order, so a picked row stays readable (its
/// number, sha, owner+repo or path). No pick, or a row that is not an
/// object, passes through whole; a key the row lacks stays absent.
pub(crate) fn pick_fields(row: Value, id: &[&str], fields: &[String]) -> Value {
    let Value::Object(map) = row else {
        return row;
    };
    if fields.is_empty() {
        return Value::Object(map);
    }
    let mut picked = serde_json::Map::new();
    for key in id.iter().copied().chain(fields.iter().map(String::as_str)) {
        if !picked.contains_key(key)
            && let Some(value) = map.get(key)
        {
            picked.insert(key.to_owned(), value.clone());
        }
    }
    Value::Object(picked)
}

/// A generated `fields` list as plain key names.
pub(crate) fn field_names<T: ToString>(fields: &[T]) -> Vec<String> {
    fields.iter().map(ToString::to_string).collect()
}

/// Makes the row's `next` an object (an empty one when absent), so leads
/// can be added by name.
pub(crate) fn ensure_next(value: &mut Value) {
    if !value.get("next").is_some_and(Value::is_object) {
        value["next"] = json!({});
    }
}

/// Puts one per-call warning first in a row's `warnings` (a rename the
/// rest of the row's warnings follow from).
pub(crate) fn push_warning_front(value: &mut Value, warning: String) {
    match value.get_mut("warnings").and_then(Value::as_array_mut) {
        Some(warnings) => warnings.insert(0, json!(warning)),
        None => value["warnings"] = json!([warning]),
    }
}

/// The one `fields` path of the search tools: picks `value[rows]` (each row
/// keeps its `id` keys) and names picked keys no row carries.
pub(crate) fn pick_rows(value: &mut Value, rows: &str, id: &[&str], fields: &[String]) {
    if fields.is_empty() {
        return;
    }
    if let Some(listed) = value.get_mut(rows).and_then(Value::as_array_mut) {
        for row in listed.iter_mut() {
            *row = pick_fields(std::mem::take(row), id, fields);
        }
    }
    warn_absent_fields(value, rows, fields);
}

/// Names the picked `fields` no row of `value[rows]` carries: a typo, or a
/// value these rows lack. The call still answers; the caller is told
/// instead of reading the missing key as an empty value.
fn warn_absent_fields(value: &mut Value, rows: &str, fields: &[String]) {
    let Some(listed) = value.get(rows).and_then(Value::as_array) else {
        return;
    };
    if listed.is_empty() {
        return;
    }
    let absent = fields
        .iter()
        .map(String::as_str)
        .filter(|key| !listed.iter().any(|row| row.get(*key).is_some()))
        .collect::<Vec<_>>();
    if !absent.is_empty() {
        push_warnings(value, [format!("No row has {}.", absent.join(", "))]);
    }
}
