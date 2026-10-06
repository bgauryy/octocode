//! Durable, shallow GitHub checkout materialization through the system Git baseline.
mod cache;
mod git;
mod process;

use crate::policy::{PolicyError, PolicyErrorCode, path::PathPolicy};
use crate::providers::github::{
    ConditionalCache, CredentialResolver, GitHubEndpoint, GitHubProvider, ProviderError,
    ProviderErrorKind, ProviderErrorReason, RequestContext, ResolvedCredential,
};
use crate::tools::cancel::CancellationCheck;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use process::{GitOutput, GitRunControl, GitRunRequest, GitRunner, SystemGit};

pub use crate::contracts::tool_types::{GhCloneRepoQuery, GhCloneRepoQueryPath};

/// The requested sparse checkout: repo-relative paths in request order,
/// duplicates removed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Sparse {
    paths: Vec<String>,
}

impl Sparse {
    fn of(query: &GhCloneRepoQuery) -> Option<Self> {
        let mut paths = Vec::<String>::new();
        match query.path.as_ref()? {
            GhCloneRepoQueryPath::String(path) => paths.push(path.clone()),
            GhCloneRepoQueryPath::Array(values) => {
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
    query.history_depth.map_or(1, std::num::NonZeroU64::get)
}

#[derive(Clone, Debug)]
pub struct CloneConfig {
    pub cache_home: PathBuf,
    pub persistent: bool,
    pub cache_ttl: Duration,
    pub max_cache_size_bytes: u64,
    pub max_clone_count: usize,
    pub lock_wait: Duration,
    /// `network.timeout`: git's network steps are sized from it.
    pub network_timeout: Duration,
}

impl CloneConfig {
    /// A persistent clone cache under `cache_home` with the config
    /// contract's `cloneCache.*` and `network.timeout` defaults.
    pub fn persistent(cache_home: impl Into<PathBuf>) -> Self {
        let defaults: serde_json::Value =
            serde_json::from_str(crate::config::DEFAULT_RESOLVED_CONFIG_JSON).unwrap_or_default();
        let limits = serde_json::from_value(defaults["cloneCache"].clone()).unwrap_or_default();
        let timeout = defaults["network"]["timeout"].as_f64().unwrap_or_default();
        Self {
            cache_home: cache_home.into(),
            persistent: true,
            cache_ttl: Duration::ZERO,
            max_cache_size_bytes: 0,
            max_clone_count: 0,
            lock_wait: Duration::from_secs(5 * 60),
            network_timeout: Duration::ZERO,
        }
        .with_limits(&limits)
        .with_network_timeout(Duration::from_secs_f64(timeout.max(0.0) / 1000.0))
    }

    /// Apply the resolved `network.timeout`.
    #[must_use]
    pub fn with_network_timeout(mut self, timeout: Duration) -> Self {
        self.network_timeout = timeout;
        self
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
    /// Parent/provider-resolved default branch. Required when query.ref_ is absent.
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

/// Where the checkout lives and what it holds. `verified` is stated only
/// when false; owner/repo are the caller's own input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneLocation {
    pub kind: &'static str,
    pub local_path: String,
    pub cached: bool,
    pub commit_sha: String,
    #[serde(skip_serializing_if = "is_true")]
    pub verified: bool,
    pub resolved_ref: String,
    /// Sparse checkouts name every requested path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_paths: Option<Vec<String>>,
    /// History depth when more than the shallow single commit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history_depth: Option<u64>,
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
            "notFound",
            format!("Repository not found: {owner}/{repo}"),
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

/// Run one ghCloneRepo row. The happy path runs on git alone; only after
/// git failed does one `commits/{ref}` call name a missing ref or
/// repository.
pub async fn run<R: CredentialResolver, C: ConditionalCache>(
    provider: &GitHubProvider<R, C>,
    query: &GhCloneRepoQuery,
    request: Result<&RequestContext, ProviderError>,
    context: &CloneContext<'_>,
) -> Result<CloneResult, CloneFailure> {
    let request = request.map_err(CloneFailure::Provider)?;
    let git = match execute_clone(query, context) {
        Ok(result) => return Ok(result),
        Err(error) => error,
    };
    if git.code != GIT_FAILED {
        return Err(CloneFailure::Clone(git));
    }
    let reference = query.ref_.as_deref().unwrap_or("HEAD");
    let answer = provider
        .transport
        .commit_sha(&query.owner, &query.repo, reference, request)
        .await;
    Err(explain_git_failure(query, git, answer))
}

const GIT_FAILED: &str = "gitFailed";

/// After git failed, the `commits/{ref}` answer explains why: a missing ref
/// or repository is not-found input, a credential the API rejects keeps its
/// provider error. When the ref resolves, or the API itself failed (rate
/// limit, outage, timeout), the answer says nothing about the request, so
/// the git error stands and an API failure is a hint.
pub(crate) fn explain_git_failure(
    query: &GhCloneRepoQuery,
    mut git: CloneError,
    answer: Result<String, ProviderError>,
) -> CloneFailure {
    let Err(api) = answer else {
        return CloneFailure::Clone(git);
    };
    match (api.kind, api.reason) {
        (_, Some(ProviderErrorReason::RefNotFound)) => {
            let reference = query.ref_.as_deref().unwrap_or("HEAD");
            CloneFailure::Clone(CloneError {
                hints: vec![
                    "Verify the branch, tag, or SHA (ghStructure lists a ref), or omit ref for the default branch.".into(),
                ],
                ..CloneError::new(
                    "notFound",
                    format!(
                        "Branch, tag, or SHA not found for {}/{}: \"{reference}\"",
                        query.owner, query.repo
                    ),
                )
            })
        }
        (ProviderErrorKind::NotFound, _) => CloneFailure::Clone(repository_not_found(query)),
        (ProviderErrorKind::Authentication | ProviderErrorKind::Permission, _) => {
            CloneFailure::Provider(api)
        }
        _ => {
            git.hints.push(format!(
                "The GitHub API ref check also failed: {}",
                api.message
            ));
            CloneFailure::Clone(git)
        }
    }
}

/// A path the policy denies, under the policy's own flat code (as the local
/// tools report it).
fn policy_denied(error: PolicyError) -> CloneError {
    let code = serde_json::to_value(error.code)
        .ok()
        .and_then(|code| code.as_str().map(str::to_owned))
        .unwrap_or_else(|| "permissionDenied".into());
    CloneError::new(code, error.message)
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
    let sparse = Sparse::of(query);
    let sparse_key = sparse.as_ref().map(Sparse::key);
    let depth = history_depth(query);
    let force_refresh = query.force_refresh.unwrap_or(false);
    let repository_url = repository_url(context.endpoint, &query.owner, &query.repo)?;
    authorize_home(context)?;
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
            .map_err(policy_denied)?;
        Ok(clone_dir)
    };
    // The default branch comes from the caller, else from the alias a
    // default-branch clone recorded: a hit needs no GitHub API call.
    let known_branch = query
        .ref_
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
        if query.ref_.is_some() || context.resolved_default_branch.is_some() {
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

/// The clone cache home passes the path policy, then exists.
fn authorize_home(context: &CloneContext<'_>) -> Result<(), CloneError> {
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
        Err(error) => return Err(policy_denied(error)),
    }
    // The configured Octocode home is an explicit cache root. Create it
    // before validating a clone target below it: otherwise a fresh home has
    // only an existing parent outside the allowed roots to canonicalize.
    std::fs::create_dir_all(&context.config.cache_home).map_err(|error| {
        CloneError::new(
            "cacheUnavailable",
            format!("Could not initialize the configured clone cache home: {error}"),
        )
    })?;
    Ok(())
}

/// A served cache hit: commit and age of a clean checkout that still matches
/// the requested identity.
struct CacheHit {
    commit_sha: String,
    verified: bool,
    age: Option<cache::CacheAge>,
    /// Checked-out bytes recorded at publication (older entries walk).
    size: Option<u64>,
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
        .map_err(policy_denied)?;
    Ok(Some(CacheHit {
        commit_sha,
        verified: meta.verified,
        age: cache::CacheAge::of(&meta, context.config.cache_ttl),
        size: meta.checkout_bytes,
    }))
}

/// Check out into a stage, verify it, and publish it. `branch` is `None` for
/// a default-branch clone: git resolves the remote HEAD and the branch it
/// checked out names the cache entry (and the default-branch alias).
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
    // Only a fresh clone probes git: a cache hit's failing `rev-parse`
    // already falls through to here.
    context.git.assert_available(&control(context))?;
    cache::cleanup_stale_artifacts(&context.config.cache_home);
    cache::evict(context, None);
    let stage_key = match locked_dir {
        Some(dir) => dir.to_path_buf(),
        None => target("HEAD")?,
    };
    let stage = cache::stage_dir(&context.config.cache_home, &stage_key)?;
    let sparse_paths = sparse.map(Sparse::paths);
    let checkout = checkout_stage(
        query,
        context,
        repository_url,
        sparse_paths,
        depth,
        branch,
        &stage,
    );
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
        let meta = write_stage_meta(
            query,
            sparse,
            depth,
            (&resolved, &commit_sha),
            &stage,
            context.config.cache_ttl,
        )?;
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
        .map_err(policy_denied)?;
    let result = result(
        sparse,
        depth,
        &resolved,
        &clone_dir,
        CacheHit {
            commit_sha,
            verified: true,
            age: cache::CacheAge::of(&meta, context.config.cache_ttl),
            size: meta.checkout_bytes,
        },
    )
    .map(|mut result| {
        result.location.cached = false;
        result
    })?;
    cache::evict(context, Some(&clone_dir));
    Ok(result)
}

/// Check out into `stage` and verify it: every sparse path exists, and a
/// requested commit is the checked-out HEAD. Returns the resolved branch
/// and commit.
fn checkout_stage(
    query: &GhCloneRepoQuery,
    context: &CloneContext<'_>,
    repository_url: &str,
    sparse_paths: Option<&[String]>,
    depth: u64,
    branch: Option<&str>,
    stage: &Path,
) -> Result<(String, String), CloneError> {
    git::checkout(context, repository_url, branch, sparse_paths, depth, stage)?;
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
        None => git::current_branch(context, stage)?,
    };
    let commit_sha = git::read_head(context, stage)?;
    if is_commit(&resolved) && commit_sha != resolved.to_ascii_lowercase() {
        return Err(CloneError::new(
            "commitMismatch",
            format!("Checkout HEAD {commit_sha} does not match requested commit {resolved}."),
        ));
    }
    Ok((resolved, commit_sha))
}

/// Record the checkout's identity, commit and size in its stage. Repository
/// evidence is never overwritten by this bookkeeping.
fn write_stage_meta(
    query: &GhCloneRepoQuery,
    sparse: Option<&Sparse>,
    depth: u64,
    (resolved, commit_sha): (&str, &str),
    stage: &Path,
    ttl: Duration,
) -> Result<cache::CacheMeta, CloneError> {
    let mut meta = cache::CacheMeta::new(
        &cache::Identity {
            owner: &query.owner,
            repo: &query.repo,
            branch: resolved,
            sparse_key: sparse.map(Sparse::key).as_deref(),
            depth,
        },
        commit_sha,
        ttl,
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
                "cacheUnavailable",
                format!("Could not inspect clone metadata destination: {error}"),
            ));
        }
    }
    // The size of a clean checkout cannot change: record it once.
    meta.checkout_bytes = Some(cache::checked_out_size(stage));
    cache::write_meta(stage, &meta)?;
    Ok(meta)
}

fn dirty_checkout(path: &Path) -> CloneError {
    CloneError {
        hints: vec![
            "Preserve your changes outside this managed checkout, then restore it to a clean state before retrying. forceRefresh does not discard local files.".into(),
        ],
        ..CloneError::new(
            "checkoutDirty",
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
        hints: vec!["Verify the path with ghStructure, or omit path for a full clone.".into()],
        ..CloneError::new(
            "pathNotFound",
            format!(
                "Path does not exist: {paths} in {}/{}{at}; nothing was checked out for it.",
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
    let total_size = hit
        .size
        .unwrap_or_else(|| cache::checked_out_size(clone_dir));
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
            resolved_ref: branch.to_owned(),
            requested_paths: paths.map(<[String]>::to_vec),
            history_depth: (depth > 1).then_some(depth),
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
        ToolId::StructureSearch
    };
    serde_json::json!({
        "exploreClone": crate::tools::result::Continuation::new(tool, serde_json::Value::Object(query))
            .confidence("exact")
            .build()
    })
}

fn validate_query(query: &GhCloneRepoQuery) -> Result<(), CloneError> {
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
                "invalidInput",
                format!("{name} must be a non-empty GitHub path segment"),
            ));
        }
    }
    if query
        .ref_
        .as_deref()
        .is_some_and(|branch| branch.trim().is_empty())
    {
        return Err(CloneError::new("invalidInput", "ref must not be empty"));
    }
    let sparse = Sparse::of(query);
    if sparse
        .as_ref()
        .is_some_and(|sparse| sparse.paths.is_empty())
    {
        return Err(CloneError::new(
            "invalidInput",
            "path must name at least one path",
        ));
    }
    for path in sparse.as_ref().map_or(&[][..], Sparse::paths) {
        if !crate::tools::gh_shared::is_repo_relative(path) {
            return Err(CloneError::new(
                "invalidInput",
                "path must be a non-empty repo-relative path without traversal",
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
        .map_err(|error| CloneError::new("configuration", error.message))?;
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
        "configuration",
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
            "timeout",
            "Clone execution exceeded its deadline",
        ));
    }
    context
        .cancellation
        .check()
        .map_err(|_| CloneError::new("cancelled", "Clone execution was cancelled"))
}

fn hash(value: &str, characters: usize) -> String {
    let mut digest = Sha256::new();
    digest.update(value.as_bytes());
    hex::encode(digest.finalize())[..characters].to_owned()
}

/// This tool's output facts for the shared response stages.
pub(crate) struct Output;
impl crate::tools::output::ToolOutput for Output {
    fn fallback_hint(&self, _query: &serde_json::Value) -> &'static str {
        "Verify owner/repo/ref and path."
    }
    fn evidence_kind(&self, _query: &serde_json::Value, _data: &serde_json::Value) -> &'static str {
        "provider"
    }
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
