//! Durable, shallow GitHub checkout materialization through the system Git baseline.
mod cache;
mod git;
mod process;

use crate::policy::path::PathPolicy;
use crate::providers::github::{GitHubEndpoint, ResolvedCredential};
use crate::tools::local_fetch::CancellationCheck;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

pub use process::{GitOutput, GitRunControl, GitRunRequest, GitRunner, SystemGit};

const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const DEFAULT_MAX_CACHE_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const DEFAULT_MAX_CLONES: usize = 50;

fn env_duration(key: &str, fallback: Duration) -> Duration {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis)
        .filter(|value| !value.is_zero())
        .unwrap_or(fallback)
}
fn env_u64(key: &str, fallback: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}
fn env_usize(key: &str, fallback: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GhCloneRepoQuery {
    pub owner: String,
    pub repo: String,
    pub branch: Option<String>,
    pub sparse_path: Option<String>,
    #[serde(default)]
    pub force_refresh: bool,
}

#[derive(Clone, Debug)]
pub struct CloneConfig {
    pub cache_home: PathBuf,
    pub persistent: bool,
    pub cache_ttl: Duration,
    pub max_cache_size_bytes: u64,
    pub max_clone_count: usize,
    pub lock_wait: Duration,
}

impl CloneConfig {
    pub fn persistent(cache_home: impl Into<PathBuf>) -> Self {
        Self {
            cache_home: cache_home.into(),
            persistent: true,
            cache_ttl: env_duration("OCTOCODE_CACHE_TTL_MS", DEFAULT_CACHE_TTL),
            max_cache_size_bytes: env_u64("OCTOCODE_MAX_CACHE_SIZE", DEFAULT_MAX_CACHE_SIZE),
            max_clone_count: env_usize("OCTOCODE_MAX_CLONES", DEFAULT_MAX_CLONES),
            lock_wait: Duration::from_secs(5 * 60),
        }
    }
}

pub struct CloneContext<'a> {
    pub config: &'a CloneConfig,
    pub endpoint: &'a GitHubEndpoint,
    pub credential: Option<&'a ResolvedCredential>,
    /// Parent/provider-resolved default branch. Required when query.branch is absent.
    pub resolved_default_branch: Option<&'a str>,
    pub cancellation: &'a dyn CancellationCheck,
    pub deadline: Instant,
    pub path_policy: &'a PathPolicy,
    pub git: &'a dyn GitRunner,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneLocation {
    pub kind: &'static str,
    pub local_path: String,
    pub source: &'static str,
    pub cached: bool,
    pub commit_sha: String,
    pub verified: bool,
    pub complete: bool,
    pub resolved_branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneResult {
    pub owner: String,
    pub repo: String,
    pub total_size: u64,
    pub location: CloneLocation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneError {
    pub code: String,
    pub message: String,
}

impl CloneError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

pub fn execute_clone(
    query: &GhCloneRepoQuery,
    context: &CloneContext<'_>,
) -> Result<CloneResult, CloneError> {
    check_control(context)?;
    if !context.config.persistent {
        return Err(CloneError::new(
            "persistentStorageDisabled",
            "Clone requires persistent local storage. Set storage.mode=\"persistent\" or OCTOCODE_STORAGE_MODE=persistent to use ghCloneRepo.",
        ));
    }
    validate_query(query)?;
    let branch = query
        .branch
        .as_deref()
        .or(context.resolved_default_branch)
        .ok_or_else(|| {
            CloneError::new(
                "clone.defaultBranchUnavailable",
                "The repository default branch was not resolved before clone execution.",
            )
        })?
        .to_owned();
    if branch.trim().is_empty() {
        return Err(CloneError::new(
            "clone.input.invalid",
            "branch must not be empty",
        ));
    }
    let repository_url = repository_url(context.endpoint, &query.owner, &query.repo)?;
    context.git.assert_available(&control(context))?;
    let clone_dir = cache::clone_dir(
        &context.config.cache_home,
        &query.owner,
        &query.repo,
        &branch,
        query.sparse_path.as_deref(),
        &repository_url,
    );
    context
        .path_policy
        .validate_output(&clone_dir)
        .map_err(|error| CloneError::new("clone.policy.denied", error.message))?;
    let _lock = cache::CloneLock::acquire(&clone_dir, context)?;
    if !query.force_refresh
        && let Some(meta) = cache::valid_clone(&clone_dir, context.config.cache_ttl)
        && meta.source == "clone"
        && let Ok(commit_sha) = git::read_head(context, &clone_dir)
        && (!is_commit(&branch) || commit_sha == branch.to_ascii_lowercase())
    {
        context
            .path_policy
            .validate(&clone_dir)
            .map_err(|error| CloneError::new("clone.policy.denied", error.message))?;
        return result(query, branch, &clone_dir, commit_sha, true, false);
    }

    cache::cleanup_stale_artifacts(&context.config.cache_home);
    cache::evict(
        &context.config.cache_home,
        context.config.cache_ttl,
        context.config.max_cache_size_bytes,
        context.config.max_clone_count,
        None,
    );
    let stage = cache::stage_dir(&context.config.cache_home, &clone_dir)?;
    let checkout = (|| {
        git::checkout(
            context,
            &repository_url,
            &branch,
            query.sparse_path.as_deref(),
            &stage,
        )?;
        if let Some(sparse_path) = query.sparse_path.as_deref()
            && !stage.join(sparse_path).exists()
        {
            return Err(CloneError::new(
                "clone.sparsePath.notFound",
                format!(
                    "sparsePath \"{sparse_path}\" does not exist in {}/{}@{branch} — nothing was checked out for it. Verify the path with ghSearch operation:\"tree\", then retry with the correct sparsePath (or omit it for a full clone).",
                    query.owner, query.repo
                ),
            ));
        }
        let commit_sha = git::read_head(context, &stage)?;
        if is_commit(&branch) && commit_sha != branch.to_ascii_lowercase() {
            return Err(CloneError::new(
                "clone.commit.mismatch",
                format!("Checkout HEAD {commit_sha} does not match requested commit {branch}."),
            ));
        }
        let meta = cache::CacheMeta::new(
            &query.owner,
            &query.repo,
            &branch,
            query.sparse_path.as_deref(),
            &commit_sha,
            context.config.cache_ttl,
        );
        cache::write_meta(&stage, &meta)?;
        cache::promote(&stage, &clone_dir)?;
        Ok(commit_sha)
    })();
    let commit_sha = match checkout {
        Ok(value) => value,
        Err(error) => {
            cache::remove_dir(&stage);
            return Err(error);
        }
    };
    context
        .path_policy
        .validate(&clone_dir)
        .map_err(|error| CloneError::new("clone.policy.denied", error.message))?;
    let result = result(query, branch, &clone_dir, commit_sha, false, true)?;
    cache::evict(
        &context.config.cache_home,
        context.config.cache_ttl,
        context.config.max_cache_size_bytes,
        context.config.max_clone_count,
        Some(&clone_dir),
    );
    Ok(result)
}

fn result(
    query: &GhCloneRepoQuery,
    branch: String,
    clone_dir: &Path,
    commit_sha: String,
    cached: bool,
    verified: bool,
) -> Result<CloneResult, CloneError> {
    let local_path = clone_dir.to_string_lossy().into_owned();
    let total_size = cache::checked_out_size(clone_dir);
    Ok(CloneResult {
        owner: query.owner.clone(),
        repo: query.repo.clone(),
        total_size,
        location: CloneLocation {
            kind: if query.sparse_path.is_some() {
                "tree"
            } else {
                "repo"
            },
            local_path: local_path.clone(),
            source: "clone",
            cached,
            commit_sha,
            verified,
            complete: true,
            resolved_branch: branch,
            requested_path: query.sparse_path.clone(),
        },
    })
}

fn validate_query(query: &GhCloneRepoQuery) -> Result<(), CloneError> {
    for (name, value) in [("owner", &query.owner), ("repo", &query.repo)] {
        if value.trim().is_empty()
            || value.contains('/')
            || value.contains('\\')
            || value == "."
            || value == ".."
        {
            return Err(CloneError::new(
                "clone.input.invalid",
                format!("{name} must be a non-empty GitHub path segment"),
            ));
        }
    }
    if let Some(path) = query.sparse_path.as_deref() {
        let value = Path::new(path);
        if path.trim().is_empty()
            || path.contains('\\')
            || value.is_absolute()
            || value
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(CloneError::new(
                "clone.input.invalid",
                "sparsePath must be a non-empty repo-relative path without traversal",
            ));
        }
    }
    Ok(())
}

fn repository_url(
    endpoint: &GitHubEndpoint,
    owner: &str,
    repo: &str,
) -> Result<String, CloneError> {
    let base = endpoint
        .rest(&[])
        .map_err(|error| CloneError::new("clone.endpoint.unsupported", error.message))?;
    if base.scheme() != "https"
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(unsupported_endpoint());
    }
    let path = base.path().trim_end_matches('/');
    if base.host_str() == Some("api.github.com") && path.is_empty() {
        let mut url = url::Url::parse("https://github.com").map_err(|_| unsupported_endpoint())?;
        url.path_segments_mut()
            .map_err(|_| unsupported_endpoint())?
            .push(owner)
            .push(&format!("{repo}.git"));
        return Ok(url.into());
    }
    if !path.ends_with("/api/v3") {
        return Err(unsupported_endpoint());
    }
    let prefix = &path[..path.len() - "/api/v3".len()];
    let mut url = base.clone();
    url.set_path(prefix);
    url.path_segments_mut()
        .map_err(|_| unsupported_endpoint())?
        .pop_if_empty()
        .push(owner)
        .push(&format!("{repo}.git"));
    Ok(url.into())
}

fn unsupported_endpoint() -> CloneError {
    CloneError::new(
        "clone.endpoint.unsupported",
        "ghCloneRepo requires an HTTPS GitHub API endpoint: https://api.github.com or a GitHub Enterprise endpoint ending in /api/v3, without credentials or query parameters. Use ghGetFileContent for other API proxies.",
    )
}

fn is_commit(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn control<'a>(context: &'a CloneContext<'a>) -> GitRunControl<'a> {
    GitRunControl {
        cancellation: context.cancellation,
        deadline: context.deadline,
        cache_home: &context.config.cache_home,
    }
}

fn check_control(context: &CloneContext<'_>) -> Result<(), CloneError> {
    if Instant::now() >= context.deadline {
        return Err(CloneError::new(
            "clone.execution.timeout",
            "Clone execution exceeded its deadline",
        ));
    }
    context
        .cancellation
        .check()
        .map_err(|_| CloneError::new("clone.execution.cancelled", "Clone execution was cancelled"))
}

fn hash(value: &str, characters: usize) -> String {
    let mut digest = Sha256::new();
    digest.update(value.as_bytes());
    hex::encode(digest.finalize())[..characters].to_owned()
}

#[cfg(test)]
mod tests;
