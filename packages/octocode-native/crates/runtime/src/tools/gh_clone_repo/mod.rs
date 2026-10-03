//! Durable, shallow GitHub checkout materialization through the system Git baseline.
mod cache;
mod git;
mod process;

use crate::policy::{PolicyErrorCode, path::PathPolicy};
use crate::providers::github::{GitHubEndpoint, ResolvedCredential};
use crate::tools::cancel::CancellationCheck;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

pub use process::{GitOutput, GitRunControl, GitRunRequest, GitRunner, SystemGit};

const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const DEFAULT_MAX_CACHE_SIZE: u64 = 2 * 1024 * 1024 * 1024;
const DEFAULT_MAX_CLONES: usize = 50;

pub use crate::contracts::tool_types::{GhCloneRepoQuery, GhCloneRepoQuerySparsePath};

/// The requested sparse checkout: repo-relative paths in request order,
/// duplicates removed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Sparse {
    paths: Vec<String>,
}

impl Sparse {
    fn of(query: &GhCloneRepoQuery) -> Option<Self> {
        let mut paths = Vec::<String>::new();
        match query.sparse_path.as_ref()? {
            GhCloneRepoQuerySparsePath::String(path) => paths.push(path.clone()),
            GhCloneRepoQuerySparsePath::Array(values) => {
                for path in values {
                    if !paths.contains(path) {
                        paths.push(path.clone());
                    }
                }
            }
        }
        Some(Self { paths })
    }

    pub(crate) fn paths(&self) -> &[String] {
        &self.paths
    }

    /// Cache identity: one path keys as itself (the layout before multi-path
    /// sparse, so existing entries stay valid); several key as their sorted,
    /// newline-joined set, so request order does not split the cache.
    pub(crate) fn key(&self) -> String {
        if let [path] = self.paths.as_slice() {
            return path.clone();
        }
        let mut sorted = self.paths.clone();
        sorted.sort();
        sorted.join("\n")
    }
}

/// Commits of history to fetch; 1 is a shallow checkout.
fn history_depth(query: &GhCloneRepoQuery) -> u64 {
    query.depth.map_or(1, std::num::NonZeroU64::get)
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
            cache_ttl: DEFAULT_CACHE_TTL,
            max_cache_size_bytes: DEFAULT_MAX_CACHE_SIZE,
            max_clone_count: DEFAULT_MAX_CLONES,
            lock_wait: Duration::from_secs(5 * 60),
        }
    }

    /// Apply the resolved `cloneCache.*` settings. The config contract has
    /// already validated and clamped them (all minimums are positive).
    #[must_use]
    pub fn with_limits(mut self, limits: &crate::config::CloneCacheConfig) -> Self {
        self.cache_ttl = Duration::from_millis(limits.ttl as u64);
        self.max_cache_size_bytes = limits.max_size as u64;
        self.max_clone_count = limits.max_clones as usize;
        self
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

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_true(value: &bool) -> bool {
    *value
}

/// Where the checkout lives and what it holds. `verified`/`complete` are
/// stated only when false; owner/repo are the caller's own input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneLocation {
    pub kind: &'static str,
    pub local_path: String,
    pub cached: bool,
    pub commit_sha: String,
    #[serde(skip_serializing_if = "is_true")]
    pub verified: bool,
    #[serde(skip_serializing_if = "is_true")]
    pub complete: bool,
    pub resolved_branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_path: Option<String>,
    /// Multi-path sparse checkouts name every requested path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_paths: Option<Vec<String>>,
    /// History depth when more than the shallow single commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depth: Option<u64>,
    /// When the checkout was cloned; a cache hit's commit may lag the branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloned_at: Option<String>,
    /// When the checkout stops being reused (`forceRefresh` re-clones now).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneResult {
    pub total_size: u64,
    pub location: CloneLocation,
    /// Continuations into the local tools on the checkout.
    pub next: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
}

impl CloneError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hints: Vec::new(),
        }
    }
}

/// The repository metadata lookup returned 404: name the repository instead
/// of the internal-sounding `clone.defaultBranchUnavailable` that cloning
/// without metadata would report.
pub fn repository_not_found(query: &GhCloneRepoQuery) -> CloneError {
    let (owner, repo) = (&query.owner, &query.repo);
    CloneError {
        // The message names the repository; the hint stays within the
        // response stage's guidance cap so it is never cut mid-sentence.
        hints: vec![
            "The repository is missing, private, or hidden from this token; check owner/repo spelling and token access.".into(),
        ],
        ..CloneError::new(
            "clone.repositoryNotFound",
            format!("Repository not found: {owner}/{repo}"),
        )
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
    if let Some(branch) = query.branch.as_deref()
        && branch.trim().is_empty()
    {
        return Err(CloneError::new(
            "clone.input.invalid",
            "branch must not be empty",
        ));
    }
    let sparse = Sparse::of(query);
    let sparse_key = sparse.as_ref().map(Sparse::key);
    let depth = history_depth(query);
    let force_refresh = query.force_refresh.unwrap_or(false);
    let repository_url = repository_url(context.endpoint, &query.owner, &query.repo)?;
    // Existing homes and homes below an allowed workspace must pass the
    // ordinary policy check. The one exception is an explicitly configured
    // root that does not exist yet: its parent lies outside that root until
    // creation. Do not create a cache directory for any other denied path.
    match context
        .path_policy
        .validate_output(&context.config.cache_home)
    {
        Ok(_) => {}
        Err(error)
            if error.code == PolicyErrorCode::OutsideAllowedRoots
                && !context.config.cache_home.exists()
                && context
                    .path_policy
                    .allowed_roots()
                    .contains(&context.config.cache_home) => {}
        Err(error) => return Err(CloneError::new("clone.policy.denied", error.message)),
    }
    // The configured Octocode home is an explicit cache root. Create it
    // before validating a clone target below it: otherwise a fresh home has
    // only an existing parent outside the allowed roots to canonicalize.
    std::fs::create_dir_all(&context.config.cache_home).map_err(|error| {
        CloneError::new(
            "clone.cache.unavailable",
            format!("Could not initialize the configured clone cache home: {error}"),
        )
    })?;
    let target = |branch: &str| -> Result<PathBuf, CloneError> {
        let clone_dir = cache::clone_dir(
            &context.config.cache_home,
            &query.owner,
            &query.repo,
            branch,
            sparse_key.as_deref(),
            depth,
            &repository_url,
        );
        context
            .path_policy
            .validate_output(&clone_dir)
            .map_err(|error| CloneError::new("clone.policy.denied", error.message))?;
        Ok(clone_dir)
    };
    // The default branch comes from the caller, else from the alias a
    // default-branch clone recorded: a hit needs no GitHub API call.
    let known_branch = query
        .branch
        .clone()
        .or_else(|| context.resolved_default_branch.map(str::to_owned))
        .or_else(|| {
            (!force_refresh)
                .then(|| {
                    cache::default_branch_alias(
                        &context.config.cache_home,
                        &query.owner,
                        &query.repo,
                        &repository_url,
                        context.config.cache_ttl,
                    )
                })
                .flatten()
        });
    context.git.assert_available(&control(context))?;
    if let Some(branch) = known_branch.as_deref() {
        let clone_dir = target(branch)?;
        let _lock = cache::CloneLock::acquire(&clone_dir, context)?;
        let identity = cache::Identity {
            owner: &query.owner,
            repo: &query.repo,
            branch,
            sparse_key: sparse_key.as_deref(),
            depth,
        };
        if !force_refresh && let Some(hit) = cache_hit(context, &clone_dir, &identity)? {
            return result(sparse.as_ref(), depth, branch, &clone_dir, hit);
        }
        if query.branch.is_some() || context.resolved_default_branch.is_some() {
            return fresh_clone(
                query,
                context,
                &repository_url,
                sparse.as_ref(),
                depth,
                Some(branch),
                &target,
                Some(clone_dir.as_path()),
            );
        }
    }
    // Unknown (or stale-aliased) default branch: let git resolve the remote
    // HEAD. One lock serializes concurrent default-branch clones; "HEAD" is
    // never a branch name, so it cannot collide with a branch cache entry.
    let pending = target("HEAD")?;
    let _lock = cache::CloneLock::acquire(&pending, context)?;
    fresh_clone(
        query,
        context,
        &repository_url,
        sparse.as_ref(),
        depth,
        None,
        &target,
        None,
    )
}

/// A served cache hit: commit and age of a clean checkout that still matches
/// the requested identity.
struct CacheHit {
    commit_sha: String,
    verified: bool,
    age: Option<cache::CacheAge>,
}

fn cache_hit(
    context: &CloneContext<'_>,
    clone_dir: &Path,
    identity: &cache::Identity<'_>,
) -> Result<Option<CacheHit>, CloneError> {
    let Some(meta) = cache::valid_clone(clone_dir, context.config.cache_ttl) else {
        return Ok(None);
    };
    if meta.source != "clone" || !meta.matches(identity) {
        return Ok(None);
    }
    let Ok(commit_sha) = git::read_head(context, clone_dir) else {
        return Ok(None);
    };
    if is_commit(identity.branch) && commit_sha != identity.branch.to_ascii_lowercase() {
        return Ok(None);
    }
    // A modified cache no longer holds the fetched revision. Do not replace
    // the caller's bytes or report them as a verified cache hit.
    if !git::is_clean(context, clone_dir)? {
        return Err(dirty_checkout(clone_dir));
    }
    context
        .path_policy
        .validate(clone_dir)
        .map_err(|error| CloneError::new("clone.policy.denied", error.message))?;
    Ok(Some(CacheHit {
        commit_sha,
        verified: meta.verified,
        age: cache::CacheAge::of(&meta, context.config.cache_ttl),
    }))
}

/// Check out into a stage, verify it, and publish it. `branch` is `None` for
/// a default-branch clone: git resolves the remote HEAD and the branch it
/// checked out names the cache entry (and the default-branch alias).
#[allow(clippy::too_many_arguments)]
fn fresh_clone(
    query: &GhCloneRepoQuery,
    context: &CloneContext<'_>,
    repository_url: &str,
    sparse: Option<&Sparse>,
    depth: u64,
    branch: Option<&str>,
    target: &dyn Fn(&str) -> Result<PathBuf, CloneError>,
    locked_dir: Option<&Path>,
) -> Result<CloneResult, CloneError> {
    cache::cleanup_stale_artifacts(&context.config.cache_home);
    cache::evict(context, None);
    let stage_key = match locked_dir {
        Some(dir) => dir.to_path_buf(),
        None => target("HEAD")?,
    };
    let stage = cache::stage_dir(&context.config.cache_home, &stage_key)?;
    let sparse_paths = sparse.map(Sparse::paths);
    let checkout = (|| {
        git::checkout(context, repository_url, branch, sparse_paths, depth, &stage)?;
        if let Some(paths) = sparse_paths {
            let missing = paths
                .iter()
                .filter(|path| !stage.join(path).exists())
                .map(String::as_str)
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                return Err(sparse_not_found(query, branch, &missing));
            }
        }
        let resolved = match branch {
            Some(branch) => branch.to_owned(),
            None => git::current_branch(context, &stage)?,
        };
        let commit_sha = git::read_head(context, &stage)?;
        if is_commit(&resolved) && commit_sha != resolved.to_ascii_lowercase() {
            return Err(CloneError::new(
                "clone.commit.mismatch",
                format!("Checkout HEAD {commit_sha} does not match requested commit {resolved}."),
            ));
        }
        Ok((resolved, commit_sha))
    })();
    let (resolved, commit_sha) = match checkout {
        Ok(value) => value,
        Err(error) => {
            discard_stage(context, &stage);
            return Err(error);
        }
    };
    let publish = (|| {
        let clone_dir = match locked_dir {
            Some(dir) => dir.to_path_buf(),
            None => target(&resolved)?,
        };
        // A default-branch clone learns its entry only now; take that
        // entry's lock before replacing it.
        let _lock = match locked_dir {
            Some(_) => None,
            None => Some(cache::CloneLock::acquire(&clone_dir, context)?),
        };
        let meta = cache::CacheMeta::new(
            &cache::Identity {
                owner: &query.owner,
                repo: &query.repo,
                branch: &resolved,
                sparse_key: sparse.map(Sparse::key).as_deref(),
                depth,
            },
            &commit_sha,
            context.config.cache_ttl,
        );
        // Repository evidence must never be overwritten by our bookkeeping.
        // This also rejects a directory or symlink at the reserved filename.
        match std::fs::symlink_metadata(stage.join(cache::META_FILE)) {
            Ok(_) => {
                return Err(cache::metadata_conflict(&meta));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(CloneError::new(
                    "clone.cache.unavailable",
                    format!("Could not inspect clone metadata destination: {error}"),
                ));
            }
        }
        cache::write_meta(&stage, &meta)?;
        // The target lock also covers this final check. forceRefresh bypasses
        // cache_hit, and user writes may arrive while the stage is fetched.
        if clone_dir.exists() && !git::checkout_status(context, &clone_dir)?.safe_to_replace {
            return Err(dirty_checkout(&clone_dir));
        }
        cache::promote(&context.config.cache_home, &stage, &clone_dir)?;
        if branch.is_none() {
            cache::write_default_branch_alias(
                &context.config.cache_home,
                &query.owner,
                &query.repo,
                repository_url,
                &resolved,
            );
        }
        Ok::<_, CloneError>((clone_dir, meta))
    })();
    let (clone_dir, meta) = match publish {
        Ok(value) => value,
        Err(error) => {
            discard_stage(context, &stage);
            return Err(error);
        }
    };
    context
        .path_policy
        .validate(&clone_dir)
        .map_err(|error| CloneError::new("clone.policy.denied", error.message))?;
    let result = result(
        sparse,
        depth,
        &resolved,
        &clone_dir,
        CacheHit {
            commit_sha,
            verified: true,
            age: cache::CacheAge::of(&meta, context.config.cache_ttl),
        },
    )
    .map(|mut result| {
        result.location.cached = false;
        result
    })?;
    cache::evict(context, Some(&clone_dir));
    Ok(result)
}

fn dirty_checkout(path: &Path) -> CloneError {
    CloneError {
        hints: vec![
            "Preserve your changes outside this managed checkout, then restore it to a clean state before retrying. forceRefresh does not discard local files.".into(),
        ],
        ..CloneError::new(
            "clone.cache.dirty",
            format!("Clone checkout '{}' contains local files or changes; it was preserved without replacement.", path.display()),
        )
    }
}

fn discard_stage(context: &CloneContext<'_>, stage: &Path) {
    crate::cache::evictions::log_eviction(
        &context.config.cache_home,
        "failed-checkout-stage",
        stage,
        0,
    );
    cache::remove_dir(stage);
}

/// Requested sparse paths absent at the checked-out revision: a not-found
/// input, named per path, with the recovery as its one hint.
fn sparse_not_found(
    query: &GhCloneRepoQuery,
    branch: Option<&str>,
    missing: &[&str],
) -> CloneError {
    let at = branch.map_or_else(String::new, |branch| format!("@{branch}"));
    let paths = missing
        .iter()
        .map(|path| format!("\"{path}\""))
        .collect::<Vec<_>>()
        .join(", ");
    CloneError {
        hints: vec![
            "Verify the path with ghStructure, or omit sparsePath for a full clone.".into(),
        ],
        ..CloneError::new(
            "clone.sparsePath.notFound",
            format!(
                "sparsePath {paths} not found in {}/{}{at}; nothing was checked out for it.",
                query.owner, query.repo
            ),
        )
    }
}

/// One clone row: a fresh checkout or a served cache hit (`cached`).
fn result(
    sparse: Option<&Sparse>,
    depth: u64,
    branch: &str,
    clone_dir: &Path,
    hit: CacheHit,
) -> Result<CloneResult, CloneError> {
    let local_path = clone_dir.to_string_lossy().into_owned();
    let total_size = cache::checked_out_size(clone_dir);
    let (cloned_at, expires_at) = hit.age.map_or((None, None), |age| {
        (Some(age.cloned_at), Some(age.expires_at))
    });
    let paths = sparse.map(Sparse::paths);
    Ok(CloneResult {
        total_size,
        next: explore_next(clone_dir, paths),
        location: CloneLocation {
            kind: if sparse.is_some() { "tree" } else { "repo" },
            local_path,
            cached: true,
            commit_sha: hit.commit_sha,
            verified: hit.verified,
            complete: true,
            resolved_branch: branch.to_owned(),
            requested_path: paths.and_then(|paths| match paths {
                [path] => Some(path.clone()),
                _ => None,
            }),
            requested_paths: paths
                .filter(|paths| paths.len() > 1)
                .map(<[String]>::to_vec),
            depth: (depth > 1).then_some(depth),
            cloned_at,
            expires_at,
        },
    })
}

/// `next.exploreClone`: list the checkout (the sparse subtree when one was
/// requested) with structureSearch, the local entry into localSearch,
/// astSearch, and lspSearch on it. A single checked-out file is read with
/// localFetch instead: structureSearch lists directories only.
fn explore_next(clone_dir: &Path, sparse_paths: Option<&[String]>) -> serde_json::Value {
    use crate::tools::id::ToolId;
    // One sparse path explores that path; several explore the checkout root.
    let root = match sparse_paths {
        Some([path]) => clone_dir.join(path),
        _ => clone_dir.to_path_buf(),
    };
    let mut query = serde_json::Map::new();
    query.insert(
        "path".into(),
        serde_json::Value::String(root.to_string_lossy().into_owned()),
    );
    let tool = if root.is_file() {
        ToolId::LocalFetch
    } else {
        crate::contracts::stamp_schema_defaults(
            ToolId::StructureSearch,
            Some("tree"),
            &mut query,
            &["operation", "maxDepth", "page", "pageSize", "debug"],
        );
        ToolId::StructureSearch
    };
    serde_json::json!({"exploreClone": {
        "tool": tool.as_str(),
        "query": query,
        "confidence": "exact"
    }})
}

pub(crate) fn validate_query(query: &GhCloneRepoQuery) -> Result<(), CloneError> {
    for (name, value) in [
        ("owner", query.owner.as_str()),
        ("repo", query.repo.as_str()),
    ] {
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
    let sparse = Sparse::of(query);
    if sparse
        .as_ref()
        .is_some_and(|sparse| sparse.paths.is_empty())
    {
        return Err(CloneError::new(
            "clone.input.invalid",
            "sparsePath must name at least one path",
        ));
    }
    for path in sparse.as_ref().map_or(&[][..], Sparse::paths) {
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

#[cfg(test)]
mod limit_tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn contract_defaults_match_persistent_defaults_and_env_overrides_apply() {
        let defaults = crate::config::resolve_sections(&[], &BTreeMap::new()).expect("defaults");
        let base = CloneConfig::persistent("/h");
        let resolved = CloneConfig::persistent("/h").with_limits(&defaults.clone_cache);
        assert_eq!(resolved.cache_ttl, base.cache_ttl);
        assert_eq!(resolved.max_cache_size_bytes, base.max_cache_size_bytes);
        assert_eq!(resolved.max_clone_count, base.max_clone_count);

        let env = BTreeMap::from([
            ("OCTOCODE_CACHE_TTL_MS".to_owned(), "120000".to_owned()),
            ("OCTOCODE_MAX_CLONES".to_owned(), "7".to_owned()),
            // Below the contract minimum: clamped, not silently ignored.
            ("OCTOCODE_MAX_CACHE_SIZE".to_owned(), "5".to_owned()),
        ]);
        let overridden = crate::config::resolve_sections(&[], &env).expect("env");
        let config = CloneConfig::persistent("/h").with_limits(&overridden.clone_cache);
        assert_eq!(config.cache_ttl, Duration::from_secs(120));
        assert_eq!(config.max_clone_count, 7);
        assert_eq!(config.max_cache_size_bytes, 1024 * 1024);
    }
}
