//! The failure a GitHub tool states: its message, hint and recovery lead.
use super::RepoPath;
use octocode_github::{ProviderError, ProviderErrorKind, ProviderErrorReason};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::result::Continuation;
use serde_json::{Value, json};

/// The asked path exists under another letter case: the lead spells it.
pub const CASE_DIFFERS_HINT: &str = "Only the path's case differs.";

/// A SAML-protected organization refused the token: the row's `ssoUrl` is
/// where it is authorized.
pub(crate) const SSO_AUTHORIZATION_HINT: &str =
    "Authorize this token for the organization (SAML SSO) at the row's ssoUrl, then retry.";

/// A search qualifier named a login GitHub cannot resolve.
pub(crate) const USER_QUALIFIER_HINT: &str = "A login in a user qualifier (author, assignee, reviewed-by, …) does not resolve: fix its spelling or drop it.";

/// The one hint of an owner-wide request whose login does not exist.
pub const OWNER_SPELLING_HINT: &str = "Check the owner spelling: a user or organization login.";

/// A repository that did not resolve: GitHub answers a private repository
/// the token cannot see exactly like a missing one (the error says so); the
/// hint is the action.
const REPOSITORY_ACCESS_HINT: &str = "Check the owner/repo spelling and this token's access.";

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
    pub calls: Option<Box<Value>>,
    /// Fields the error row names besides the error (a file's identity).
    pub fields: Vec<(&'static str, Value)>,
}

impl GhFailure {
    pub fn new(error: ProviderError, message: impl Into<String>) -> Self {
        Self {
            error: Box::new(error),
            message: message.into(),
            hints: Vec::new(),
            calls: None,
            fields: Vec::new(),
        }
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hints = vec![hint.into()];
        self
    }
}

pub fn repository_not_found(error: &ProviderError) -> bool {
    error.reason == Some(ProviderErrorReason::RepositoryNotFound)
}

/// The message every GitHub tool states for a provider failure its own
/// not-found, ref and validation wording does not cover: authentication, an
/// unresolved repository, a missing resource, an outage, a network failure or
/// timeout, else the provider's own message.
pub fn provider_message(error: &ProviderError) -> String {
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
        ProviderErrorKind::Transport => match error.proxy.as_deref() {
            Some(proxy) => format!("Network connection failed through proxy {proxy}"),
            None => "Network connection failed".to_owned(),
        },
        ProviderErrorKind::Timeout => "Request timeout".to_owned(),
        _ => error.message.to_string(),
    }
}

/// A GitHub 422 as `summary: detail`, keeping GitHub's own detail (e.g.
/// `"abc" is not a numeric value`): it names the bad input.
pub fn validation_message(error: &ProviderError, summary: &str) -> String {
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
pub fn provider_hint(error: &ProviderError) -> Option<&'static str> {
    if error.sso_url.is_some() {
        Some(SSO_AUTHORIZATION_HINT)
    } else if error.kind == ProviderErrorKind::Authentication {
        Some(octocode_runtime_base::response::rows::GITHUB_AUTH_RECOVERY_HINT)
    } else if repository_not_found(error) {
        Some(REPOSITORY_ACCESS_HINT)
    } else if error.reason == Some(ProviderErrorReason::UserNotFound) {
        Some(USER_QUALIFIER_HINT)
    } else if error.reason == Some(ProviderErrorReason::FileAccess) {
        // A write under the home failed (a materialized tree): a read's own
        // hint names permissions and encoding, not a full or read-only home.
        Some(octocode_runtime_base::response::rows::WRITE_ACCESS_HINT)
    } else {
        None
    }
}

/// The failure of a search tool (ghSearchRepo, ghSearchCode, ghStructure).
/// A page past the search window takes its repair lead from
/// [`with_search_repair`](super::search::with_search_repair), not a hint.
pub fn search_failure(error: ProviderError) -> GhFailure {
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
    let hint = provider_hint(&error);
    let failure = GhFailure::new(error, message);
    match hint {
        Some(hint) => failure.hint(hint),
        None => failure,
    }
}

/// A ref that did not resolve: ghStructure `operation:"refs"` lists the
/// branches and tags it could name.
pub fn ref_recovery(owner: &str, repo: &str) -> Value {
    let mut refs = RepoPath::root(owner, repo).row();
    refs["operation"] = json!("refs");
    Continuation::related(ToolId::GhStructure, refs).build()
}

/// The page a recovery lead asks for: enough candidates to spot a
/// misspelled name or a nearby number at a fraction of a full page's cost
/// (a full page of an owner's repositories or a repository's pull requests
/// costs more than the failed call); the listing's own next page reads on,
/// so nothing is cut.
pub const RECOVERY_LIMIT: u32 = 10;

/// A repository that did not resolve: a search for its name across every
/// owner, which names the owner that holds it.
fn repository_search(repo: &str) -> Value {
    Continuation::related(
        ToolId::GhSearchRepo,
        json!({"operation":"search","keywords": [repo],"limit": RECOVERY_LIMIT}),
    )
    .build()
}

/// A repository that did not resolve under `owner`: the owner's own
/// repositories, beside the name search (a misspelled owner is not found
/// by the name; that listing says when no such login exists).
pub fn owner_listing(owner: &str) -> Value {
    Continuation::related(
        ToolId::GhSearchRepo,
        json!({"operation":"list","owner": owner,"limit": RECOVERY_LIMIT}),
    )
    .build()
}

/// The message of a repository that did not resolve on a read: GitHub
/// answers a private repository the token cannot see like a missing one.
pub fn repository_not_found_message(owner: &str, repo: &str) -> String {
    format!("Repository {owner}/{repo} not found, or private and not accessible to this token")
}

/// The leads of a repository that did not resolve, every GitHub tool's
/// one recovery for it: `findRepository`, the [`repository_search`] for its
/// name, and, when the `owner` is known, `findRepository2`, that
/// [`owner_listing`] (a misspelled repository is found there, a misspelled
/// owner by the name). [`REPOSITORY_SEARCH_HINT`] labels them.
pub fn repository_recovery(owner: Option<&str>, repo: &str) -> Value {
    let mut calls = json!({ "findRepository": repository_search(repo) });
    if let Some(owner) = owner {
        calls["findRepository2"] = owner_listing(owner);
    }
    calls
}

/// The failure of a read whose repository did not resolve: its message,
/// its [`repository_recovery`] leads and the [`REPOSITORY_SEARCH_HINT`]
/// that labels them.
pub fn repository_failure(error: ProviderError, owner: &str, repo: &str) -> GhFailure {
    let mut failure = GhFailure::new(error, repository_not_found_message(owner, repo))
        .hint(REPOSITORY_SEARCH_HINT.to_owned());
    failure.calls = Some(Box::new(repository_recovery(Some(owner), repo)));
    failure
}

/// The hint beside [`repository_recovery`]: the related[] calls say what
/// they list; the one fact they do not is that the name search's rows sit
/// under any owner, so each is a candidate whose owner the caller confirms.
pub const REPOSITORY_SEARCH_HINT: &str = "Confirm the owner before using a related[] repository.";

/// The error for an owner-wide search that found no such login. It names the
/// login; the caller adds the spelling hint only where one helps.
pub fn owner_not_found_message(owner: &str) -> String {
    format!("Owner \"{owner}\" not found: no GitHub user or organization has this login")
}

/// A missing ref's failure gets the runnable refs listing (the lead is the
/// fix: no hint beside it); any other failure is returned unchanged.
pub fn with_ref_recovery(mut failure: GhFailure, owner: &str, repo: &str) -> GhFailure {
    if failure.error.reason != Some(ProviderErrorReason::RefNotFound) {
        return failure;
    }
    let mut next = failure.calls.take().unwrap_or_else(|| Box::new(json!({})));
    next["viewRefs"] = ref_recovery(owner, repo);
    failure.calls = Some(next);
    failure
}

#[cfg(test)]
mod tests;
