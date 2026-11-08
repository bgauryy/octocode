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
#[cfg(test)]
use plan::execute_clone;
use plan::{execute_prepared, prepare_clone};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
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

/// Everything one clone reads, owned, so [`run`] does the clone's blocking
/// setup and git work on the blocking pool: the runtime thread meanwhile
/// sends the pin request and serves other requests. [`Self::context`] is the
/// borrowed view that setup and git work take.
pub struct CloneSetup {
    pub config: CloneConfig,
    pub endpoint: GitHubEndpoint,
    /// Network git admission: the API transport's budget, waits on the row.
    pub git_limiter: octocode_github::GitLimiter,
    pub credential: Option<ResolvedCredential>,
    pub cancellation: Arc<dyn CancellationCheck + Send>,
    pub deadline: Instant,
    pub path_policy: PathPolicy,
    pub git: Arc<dyn GitRunner>,
}

impl CloneSetup {
    pub fn context(&self) -> CloneContext<'_> {
        CloneContext {
            config: &self.config,
            endpoint: &self.endpoint,
            git_limiter: self.git_limiter.clone(),
            credential: self.credential.as_ref(),
            cancellation: self.cancellation.as_ref(),
            deadline: self.deadline,
            path_policy: &self.path_policy,
            git: self.git.as_ref(),
        }
    }

    /// `work` on the context, on the blocking pool, keeping the call's notices.
    async fn off_runtime<T: Send + 'static>(
        self: &Arc<Self>,
        work: impl FnOnce(&CloneContext<'_>) -> Result<T, CloneError> + Send + 'static,
    ) -> Result<T, CloneError> {
        let setup = Arc::clone(self);
        tokio::task::spawn_blocking(octocode_io::notice::carried(move || work(&setup.context())))
            .await
            .map_err(|error| {
                CloneError::new(
                    ErrorCode::ExecutionFailed,
                    format!("Clone worker failed: {error}"),
                )
            })?
    }
}

pub struct CloneContext<'a> {
    pub config: &'a CloneConfig,
    pub endpoint: &'a GitHubEndpoint,
    /// Network git admission: the API transport's budget, waits on the row.
    pub git_limiter: octocode_github::GitLimiter,
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
    /// Credential-named files (`.env`, keys, ...) in scope that the checkout
    /// leaves out, as ghStructure materialize does; absent when there are none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<Vec<String>>,
    /// History depth when more than the shallow single commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_depth: Option<u64>,
    /// When the checkout of this commit was cloned.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloned_at: Option<String>,
    /// The checkout was already on disk, verified unchanged, and nothing was
    /// fetched: `clonedAt` is its first clone. Absent on a fresh clone.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reused: bool,
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
    setup: &Arc<CloneSetup>,
) -> Result<CloneResult, CloneFailure> {
    require_persistent_storage(setup.config.persistent)?;
    let provider = GitHubProvider {
        transport: provider.transport.clone(),
    };
    let provider = &provider;
    let request = request.map_err(CloneFailure::Provider)?;
    validate_query(query).map_err(CloneFailure::Clone)?;
    // Git and the clones root need no commit: they are set up on the
    // blocking pool while the runtime thread sends the pin and the API
    // answers it. The pin's failure comes first, as it did when the setup
    // waited for it.
    let (pin, prepared) = futures_util::future::join(
        pin(provider, query, request),
        setup.off_runtime(prepare_clone),
    )
    .await;
    let pin = pin?;
    prepared.map_err(CloneFailure::from)?;
    let decode = |error: octocode_contract_types::error::ConversionError| {
        CloneFailure::Clone(CloneError::new(ErrorCode::InvalidInput, error.to_string()))
    };
    let pinned = GhCloneRepoQuery {
        owner: pin.owner.parse().map_err(decode)?,
        repo: pin.repo.parse().map_err(decode)?,
        ref_: Some(pin.commit.parse().map_err(decode)?),
        ..query.clone()
    };
    // The clone itself (lock, fetch, checkout, verification) blocks for as
    // long as git runs: off the runtime thread, so other requests go on.
    let mut result = setup
        .off_runtime(move |context| execute_prepared(&pinned, context))
        .await
        .map_err(CloneFailure::from)?;
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
    let mut digest = octocode_engine::digest::HashWriter::default();
    digest.update(value.as_bytes());
    hex::encode(digest.finalize())[..characters].to_owned()
}

#[cfg(test)]
mod tests;
