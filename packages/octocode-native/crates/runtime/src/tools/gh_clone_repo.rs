//! Durable, shallow GitHub checkout materialization through the system Git baseline.
mod checkout;
mod git;
mod pin;
mod plan;
mod recovery;
pub(crate) mod storage;
mod validate;

use crate::policy::PolicyError;
use octocode_contract_types::{ErrorCode, GhCloneRepoQuery};
use octocode_github::{GitHubProvider, ProviderError, RequestContext};
use pin::pin;
#[cfg(test)]
use plan::execute_clone;
use plan::{execute_prepared, prepare_clone};
use serde::Serialize;
use std::sync::Arc;
use std::time::Instant;
use validate::validate_query;

use crate::tools::gh_git::{CloneContext, CloneError, CloneSetup, GitRunRequest, control};

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

/// Where the checkout lives and what it holds; owner/repo are the caller's
/// own input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneLocation {
    pub kind: &'static str,
    /// The checkout directory the local tools take.
    pub path: String,
    pub commit_sha: String,
    /// The branch or tag resolved to `commitSha`, else that commit.
    #[serde(rename = "ref")]
    pub ref_: String,
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

/// The repository metadata lookup returned 404: name the repository instead
/// of the internal-sounding `clone.defaultBranchUnavailable` that cloning
/// without metadata would report.
pub fn repository_not_found(query: &GhCloneRepoQuery) -> CloneError {
    let (owner, repo) = (query.owner.as_str(), query.repo.as_str());
    CloneError {
        hints: vec![crate::tools::gh_shared::REPOSITORY_SEARCH_HINT.to_owned()],
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
    result.location.ref_ = pin.label;
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
