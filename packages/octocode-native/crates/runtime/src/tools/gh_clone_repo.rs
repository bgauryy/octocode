//! Durable, shallow GitHub checkout materialization through the system Git baseline.
mod checkout;
mod git;
mod pin;
mod plan;
mod process;
mod recovery;
pub(crate) mod storage;
mod validate;

use crate::execution::cancel::CancellationCheck;
use crate::policy::{PolicyError, path::PathPolicy};
use octocode_contract_types::{ErrorCode, GhCloneRepoQuery};
use octocode_github::{
    GitHubEndpoint, GitHubProvider, ProviderError, RequestContext, ResolvedCredential,
};
use pin::pin;
use plan::execute_clone;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use validate::validate_query;

pub use process::{GitOutput, GitRunControl, GitRunRequest, GitRunner, SystemGit};

/// Clone storage is an execution prerequisite, before provider or Git work.
pub(crate) fn require_persistent_storage(enabled: bool) -> Result<(), CloneError> {
    if enabled {
        Ok(())
    } else {
        Err(CloneError::new(
            ErrorCode::PersistentStorageDisabled,
            "Clone requires persistent local storage. Set storage.mode=\"persistent\" or OCTOCODE_STORAGE_MODE=persistent to use ghCloneRepo.",
        ))
    }
}

/// The requested sparse checkout: repo-relative paths in request order,
/// duplicates removed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Sparse {
    paths: Vec<String>,
}

impl Sparse {
    fn of(query: &GhCloneRepoQuery) -> Option<Self> {
        if query.paths.is_empty() {
            return None;
        }
        let mut paths = Vec::<String>::new();
        for path in &query.paths {
            if !paths.iter().any(|known| known == path.as_str()) {
                paths.push(path.to_string());
            }
        }
        Some(Self { paths })
    }

    pub(crate) fn paths(&self) -> &[String] {
        &self.paths
    }

    /// Destination identity: the sorted path set ignores request order.
    pub(crate) fn key(&self) -> String {
        let mut sorted = self.paths.clone();
        sorted.sort();
        sorted.join("\n")
    }
}

/// Commits of history to fetch; the contract's default of 1 is a shallow
/// checkout.
fn history_depth(query: &GhCloneRepoQuery) -> u64 {
    query.history_depth.get()
}

#[derive(Clone, Debug)]
pub struct CloneConfig {
    pub home: PathBuf,
    pub persistent: bool,
    pub lock_wait: Duration,
    /// The runtime's network timeout: git's network steps are sized from it.
    pub network_timeout: Duration,
}

impl CloneConfig {
    /// Checkout output and working directories under the configured home.
    pub fn persistent(home: impl Into<PathBuf>) -> Self {
        let defaults = crate::execution::RuntimeLimits::DEFAULT;
        Self {
            home: home.into(),
            persistent: true,
            lock_wait: Duration::from_secs(5 * 60),
            network_timeout: Duration::ZERO,
        }
        .with_network_timeout(defaults.network_timeout)
    }

    /// Apply the runtime's network timeout.
    #[must_use]
    pub fn with_network_timeout(mut self, timeout: Duration) -> Self {
        self.network_timeout = timeout;
        self
    }
}

pub struct CloneContext<'a> {
    pub config: &'a CloneConfig,
    pub endpoint: &'a GitHubEndpoint,
    pub credential: Option<&'a ResolvedCredential>,
    pub cancellation: &'a dyn CancellationCheck,
    pub deadline: Instant,
    pub path_policy: &'a PathPolicy,
    pub git: &'a dyn GitRunner,
}

/// Where the checkout lives and what it holds; owner/repo are the caller's
/// own input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneLocation {
    pub kind: &'static str,
    pub local_path: String,
    pub commit_sha: String,
    pub resolved_ref: String,
    /// Sparse checkouts name every requested path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_paths: Option<Vec<String>>,
    /// Repository symlinks in the checkout, each a plain file holding its
    /// link target; absent when there are none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlinks: Option<Vec<String>>,
    /// History depth when more than the shallow single commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_depth: Option<u64>,
    /// When the checkout of this commit was cloned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloned_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneResult {
    pub bytes: u64,
    pub location: CloneLocation,
    /// Continuations into the local tools on the checkout.
    #[serde(flatten, serialize_with = "crate::response::result::serialize_calls")]
    pub calls: Option<serde_json::Value>,
    /// Per-call facts the checkout follows from (a repository rename).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    /// Runnable recovery calls (`findRepository`, `viewRefs`) beside the hints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calls: Option<Box<serde_json::Value>>,
    /// The provider failure a git step stopped on (the shared GitHub
    /// limiter's rate limit or open circuit): the row is that provider
    /// failure, with its rate-limit facts and retry ([`CloneFailure::from`]).
    #[serde(skip)]
    upstream: Option<Box<ProviderError>>,
}

impl CloneError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hints: Vec::new(),
            calls: None,
            upstream: None,
        }
    }

    /// A git step stopped by a provider failure: it reaches the row as
    /// [`CloneFailure::Provider`].
    fn provider(error: ProviderError) -> Self {
        let mut clone = Self::new(error.kind.code(), error.message.to_string());
        clone.upstream = Some(Box::new(error));
        clone
    }
}

/// The repository metadata lookup returned 404: name the repository instead
/// of the internal-sounding `clone.defaultBranchUnavailable` that cloning
/// without metadata would report.
pub fn repository_not_found(query: &GhCloneRepoQuery) -> CloneError {
    let (owner, repo) = (query.owner.as_str(), query.repo.as_str());
    CloneError {
        hints: vec![crate::tools::gh_shared::repository_search_hint(
            Some(owner),
            repo,
        )],
        calls: Some(Box::new(crate::tools::gh_shared::repository_recovery(
            Some(owner),
            repo,
        ))),
        ..CloneError::new(
            ErrorCode::NotFound,
            crate::tools::gh_shared::repository_not_found_message(owner, repo),
        )
    }
}

/// Why a ghCloneRepo row failed: the clone's own error, or a GitHub
/// credential the API rejected.
#[derive(Debug)]
pub enum CloneFailure {
    Clone(CloneError),
    Provider(ProviderError),
}

impl From<CloneError> for CloneFailure {
    /// A clone error a provider failure caused is that provider failure.
    fn from(error: CloneError) -> Self {
        match error.upstream {
            Some(upstream) => Self::Provider(*upstream),
            None => Self::Clone(error),
        }
    }
}

/// Run one ghCloneRepo row. One API answer pins the checkout before git
/// runs: the commit the requested ref (else the default branch) names, read
/// with the caller's own credential ([`pin`]). The checkout is keyed by that
/// commit under the repository's canonical name, so a branch that moved is
/// checked out again, and one commit reached by branch, abbreviated or full
/// SHA, or an old repository name shares one checkout.
pub async fn run(
    provider: &GitHubProvider,
    query: &GhCloneRepoQuery,
    request: Result<&RequestContext, ProviderError>,
    context: &CloneContext<'_>,
) -> Result<CloneResult, CloneFailure> {
    require_persistent_storage(context.config.persistent)?;
    let provider = GitHubProvider {
        transport: provider.transport.uncached(),
    };
    let provider = &provider;
    let request = request.map_err(CloneFailure::Provider)?;
    validate_query(query).map_err(CloneFailure::Clone)?;
    let pin = pin(provider, query, request).await?;
    let decode = |error: octocode_contract_types::error::ConversionError| {
        CloneFailure::Clone(CloneError::new(ErrorCode::InvalidInput, error.to_string()))
    };
    let pinned = GhCloneRepoQuery {
        owner: pin.owner.parse().map_err(decode)?,
        repo: pin.repo.parse().map_err(decode)?,
        ref_: Some(pin.commit.parse().map_err(decode)?),
        ..query.clone()
    };
    let mut result = execute_clone(&pinned, context).map_err(CloneFailure::from)?;
    result.location.resolved_ref = pin.label;
    if pin.renamed {
        result
            .warnings
            .push(crate::tools::gh_shared::renamed_warning(
                query.owner.as_str(),
                query.repo.as_str(),
                &(pin.owner, pin.repo),
            ));
    }
    Ok(result)
}

/// A path the policy denies, under the flat code the local tools share
/// ([`PolicyError::shared_code`]), so it gets their recovery hints.
fn policy_denied(error: PolicyError) -> CloneError {
    CloneError::new(error.shared_code(), error.message)
}

fn control<'a>(context: &'a CloneContext<'a>) -> GitRunControl<'a> {
    GitRunControl {
        cancellation: context.cancellation,
        deadline: context.deadline,
        home: &context.config.home,
    }
}

fn check_control(context: &CloneContext<'_>) -> Result<(), CloneError> {
    if Instant::now() >= context.deadline {
        return Err(CloneError::new(
            ErrorCode::Timeout,
            "Clone execution exceeded its deadline",
        ));
    }
    context
        .cancellation
        .check()
        .map_err(|_| CloneError::new(ErrorCode::Cancelled, "Clone execution was cancelled"))
}

fn hash(value: &str, characters: usize) -> String {
    let mut digest = Sha256::new();
    digest.update(value.as_bytes());
    hex::encode(digest.finalize())[..characters].to_owned()
}

#[cfg(test)]
mod tests;
