//! System git for GitHub transfers, shared by ghCloneRepo and ghStructure
//! `materialize`: the runner and its process isolation ([`process`]), one
//! git step under the shared limiter ([`step`]), the endpoint's git URLs
//! ([`endpoint`]) and blob-only transfers ([`blobs`]).
//!
//! What a transfer is *for* stays with its tool: checkouts, stores and
//! pinning belong to ghCloneRepo, listings and the writer to ghStructure.
pub(crate) mod admission;
pub(crate) mod blobs;
pub(crate) mod endpoint;
pub(crate) mod process;
pub(crate) mod step;

use crate::execution::cancel::CancellationCheck;
use crate::policy::path::PathPolicy;
use octocode_contract_types::ErrorCode;
use octocode_github::{GitHubEndpoint, ProviderError, ResolvedCredential};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use process::{GitOutput, GitRunControl, GitRunRequest, GitRunner, SystemGit};

/// Root of the shared object stores under the Octocode home. Never a read
/// root: local tools see checkouts, not the object store behind them.
pub(crate) fn store_root(home: &Path) -> PathBuf {
    home.join("clone-store")
}

/// Where a checkout is staged under the Octocode home before it is published.
pub(crate) fn stage_root(home: &Path) -> PathBuf {
    home.join("tmp").join("clone-tmp")
}

#[derive(Clone, Debug)]
pub struct GitConfig {
    pub home: PathBuf,
    pub persistent: bool,
    /// The runtime's network timeout: git's network steps are sized from it.
    pub network_timeout: Duration,
}

impl GitConfig {
    /// Checkout output and working directories under the configured home.
    pub fn persistent(home: impl Into<PathBuf>) -> Self {
        let defaults = crate::execution::RuntimeLimits::DEFAULT;
        Self {
            home: home.into(),
            persistent: true,
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

/// Everything one git job reads, owned, so [`Self::off_runtime`] does its
/// blocking setup and git work on the blocking pool: the runtime thread
/// meanwhile sends requests and serves others. [`Self::context`] is the
/// borrowed view that setup and git work take.
pub struct GitSetup {
    pub config: GitConfig,
    pub endpoint: GitHubEndpoint,
    /// Network git admission: the API transport's budget, waits on the row.
    pub git_limiter: octocode_github::GitLimiter,
    pub credential: Option<ResolvedCredential>,
    pub cancellation: Arc<dyn CancellationCheck + Send>,
    pub deadline: Instant,
    pub path_policy: PathPolicy,
    pub git: Arc<dyn GitRunner>,
}

impl GitSetup {
    pub fn context(&self) -> GitContext<'_> {
        GitContext {
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
    pub(crate) async fn off_runtime<T: Send + 'static>(
        self: &Arc<Self>,
        work: impl FnOnce(&GitContext<'_>) -> Result<T, GitError> + Send + 'static,
    ) -> Result<T, GitError> {
        let setup = Arc::clone(self);
        tokio::task::spawn_blocking(octocode_io::notice::carried(move || work(&setup.context())))
            .await
            .map_err(|error| {
                GitError::new(
                    ErrorCode::ExecutionFailed,
                    format!("Clone worker failed: {error}"),
                )
            })?
    }
}

pub struct GitContext<'a> {
    pub config: &'a GitConfig,
    pub endpoint: &'a GitHubEndpoint,
    /// Network git admission: the API transport's budget, waits on the row.
    pub git_limiter: octocode_github::GitLimiter,
    pub credential: Option<&'a ResolvedCredential>,
    pub cancellation: &'a dyn CancellationCheck,
    pub deadline: Instant,
    pub path_policy: &'a PathPolicy,
    pub git: &'a dyn GitRunner,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    /// Runnable recovery calls (`findRepository`, `viewRefs`) beside the hints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calls: Option<Box<serde_json::Value>>,
    /// The provider failure a git step stopped on (the shared GitHub
    /// limiter's rate limit or open circuit): the row is that provider
    /// failure, with its rate-limit facts and retry.
    #[serde(skip)]
    pub(crate) upstream: Option<Box<ProviderError>>,
    /// The HTTP status GitHub answered the request this error classifies
    /// (a missing repository or ref): the row's `httpStatus`, as on every
    /// GitHub tool's row.
    #[serde(skip)]
    pub(crate) http_status: Option<u16>,
}

impl GitError {
    pub(crate) fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hints: Vec::new(),
            calls: None,
            upstream: None,
            http_status: None,
        }
    }

    /// This error, classifying a request GitHub answered with `status`.
    pub(crate) fn answered(mut self, status: Option<u16>) -> Self {
        self.http_status = status;
        self
    }

    /// A git step stopped by a provider failure: it reaches the row as the
    /// provider failure.
    pub(crate) fn provider(error: ProviderError) -> Self {
        let mut clone = Self::new(error.code(), error.message.to_string());
        clone.upstream = Some(Box::new(error));
        clone
    }
}

pub(crate) fn control<'a>(context: &'a GitContext<'a>) -> GitRunControl<'a> {
    GitRunControl {
        cancellation: context.cancellation,
        deadline: context.deadline,
        home: &context.config.home,
    }
}
