//! Durable, shallow GitHub checkout materialization through the system Git baseline.
pub(crate) mod cache;
mod git;
mod process;
mod recovery;

use crate::execution::cancel::CancellationCheck;
use crate::policy::{PolicyError, PolicyErrorCode, path::PathPolicy};
use crate::response::result::Confidence;
use octocode_github::{
    GitHubEndpoint, GitHubProvider, ProviderError, ProviderErrorKind, ProviderErrorReason,
    RequestContext, ResolvedCredential,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use process::{GitOutput, GitRunControl, GitRunRequest, GitRunner, SystemGit};

pub use octocode_contract_types::{GhCloneRepoQuery, GhCloneRepoQueryPath};

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
            GhCloneRepoQueryPath::String(path) => paths.push(path.to_string()),
            GhCloneRepoQueryPath::Array(values) => {
                for path in values {
                    if !paths.iter().any(|known| known == path.as_str()) {
                        paths.push(path.to_string());
                    }
                }
            }
        }
        Some(Self { paths })
    }

    pub(crate) fn paths(&self) -> &[String] {
        &self.paths
    }

    /// Cache identity: the sorted, newline-joined path set, so request
    /// order does not split the cache.
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
    pub cache_home: PathBuf,
    pub persistent: bool,
    pub cache_ttl: Duration,
    pub max_cache_size_bytes: u64,
    pub max_clone_count: usize,
    pub lock_wait: Duration,
    /// The runtime's network timeout: git's network steps are sized from it.
    pub network_timeout: Duration,
}

impl CloneConfig {
    /// A persistent clone cache under `cache_home` with the runtime's
    /// constant clone limits and network timeout.
    pub fn persistent(cache_home: impl Into<PathBuf>) -> Self {
        let defaults = crate::execution::RuntimeLimits::DEFAULT;
        Self {
            cache_home: cache_home.into(),
            persistent: true,
            cache_ttl: Duration::ZERO,
            max_cache_size_bytes: 0,
            max_clone_count: 0,
            lock_wait: Duration::from_secs(5 * 60),
            network_timeout: Duration::ZERO,
        }
        .with_limits(&defaults.clone)
        .with_network_timeout(defaults.network_timeout)
    }

    /// Apply the runtime's network timeout.
    #[must_use]
    pub fn with_network_timeout(mut self, timeout: Duration) -> Self {
        self.network_timeout = timeout;
        self
    }

    /// Apply the runtime's clone cache limits.
    #[must_use]
    pub fn with_limits(mut self, limits: &crate::execution::CloneLimits) -> Self {
        self.cache_ttl = limits.ttl;
        self.max_cache_size_bytes = limits.max_bytes;
        self.max_clone_count = limits.max_clones;
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
    pub cached: bool,
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
    /// When the checkout stops being reused (`forceRefresh` re-clones now).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneResult {
    pub bytes: u64,
    pub location: CloneLocation,
    /// Continuations into the local tools on the checkout.
    pub next: serde_json::Value,
    /// Per-call facts the checkout follows from (a repository rename).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hints: Vec<String>,
    /// Runnable recovery calls (`findRepository`, `viewRefs`) beside the hints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<Box<serde_json::Value>>,
}

impl CloneError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hints: Vec::new(),
            next: None,
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
        hints: vec![crate::tools::gh_shared::REPOSITORY_ACCESS_HINT.into()],
        next: Some(Box::new(serde_json::json!({
            "findRepository": crate::tools::gh_shared::repository_search(repo.as_str())
        }))),
        ..CloneError::new("notFound", format!("Repository not found: {owner}/{repo}"))
    }
}

/// Why a ghCloneRepo row failed: the clone's own error, or a GitHub
/// credential the API rejected.
#[derive(Debug)]
pub enum CloneFailure {
    Clone(CloneError),
    Provider(ProviderError),
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
    let request = request.map_err(CloneFailure::Provider)?;
    validate_query(query).map_err(CloneFailure::Clone)?;
    let pin = pin(provider, query, request).await?;
    let decode = |error: octocode_contract_types::error::ConversionError| {
        CloneFailure::Clone(CloneError::new("invalidInput", error.to_string()))
    };
    let pinned = GhCloneRepoQuery {
        owner: pin.owner.parse().map_err(decode)?,
        repo: pin.repo.parse().map_err(decode)?,
        ref_: Some(pin.commit.parse().map_err(decode)?),
        ..query.clone()
    };
    let mut result = execute_clone(&pinned, context).map_err(CloneFailure::Clone)?;
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

/// The commit one clone checks out, under the repository's canonical name,
/// and the ref name it was asked for (the default branch's name when none).
struct Pin {
    owner: String,
    repo: String,
    commit: String,
    label: String,
    /// `owner`/`repo` is the canonical name of a renamed repository.
    renamed: bool,
}

/// Resolve the clone's commit with the caller's credential. The clone cache
/// is shared by every credential of the runtime and its keys name none (a
/// refreshed token must not re-clone), so this read is also the access check
/// a cached checkout is served behind. Any failed answer refuses the row, a
/// rate limit or outage included: one caller's spent quota never unlocks
/// another caller's checkout. The reads follow GitHub's rename redirect, so
/// the canonical name costs no further request when nothing was renamed.
async fn pin(
    provider: &GitHubProvider,
    query: &GhCloneRepoQuery,
    request: &RequestContext,
) -> Result<Pin, CloneFailure> {
    let (owner, repo) = (query.owner.as_str(), query.repo.as_str());
    let transport = &provider.transport;
    // Without a ref, the default branch's name and its head commit load
    // together: `HEAD` is that branch's head, so both answers name one commit.
    let (label, commit) = match query.ref_.as_deref() {
        Some(reference) => (
            reference.to_owned(),
            transport.commit_sha(owner, repo, reference, request).await,
        ),
        None => {
            let (metadata, head) = tokio::join!(
                transport.repository_metadata(owner, repo, request),
                transport.commit_sha(owner, repo, "HEAD", request),
            );
            match metadata {
                Ok(metadata) => (metadata.default_branch, head),
                Err(api) => return Err(refused(provider, query, "HEAD", api, request).await),
            }
        }
    };
    let commit = match commit {
        Ok(commit) => commit,
        Err(api) => return Err(refused(provider, query, &label, api, request).await),
    };
    let canonical = crate::tools::gh_shared::canonical_repo(
        provider,
        owner,
        repo,
        request,
        crate::tools::gh_shared::RenameCheck::Redirected,
    )
    .await
    .map_err(CloneFailure::Provider)?;
    let renamed = canonical.is_some();
    let (owner, repo) = canonical.unwrap_or_else(|| (owner.to_owned(), repo.to_owned()));
    Ok(Pin {
        owner,
        repo,
        commit,
        label,
        renamed,
    })
}

/// The failure of a clone whose commit lookup `api` refused ([`api_refusal`]);
/// a missing ref also names the repository's default branch and offers the
/// same clone at it.
async fn refused(
    provider: &GitHubProvider,
    query: &GhCloneRepoQuery,
    reference: &str,
    api: ProviderError,
    request: &RequestContext,
) -> CloneFailure {
    let missing_ref = api.reason == Some(ProviderErrorReason::RefNotFound);
    let mut failure = api_refusal(query, reference, api).unwrap_or_else(CloneFailure::Provider);
    if missing_ref
        && let CloneFailure::Clone(error) = &mut failure
        && let Some(retry) = crate::tools::gh_shared::default_branch_retry(
            provider,
            crate::id::ToolId::GhCloneRepo,
            crate::response::result::query_row(query, &[]),
            &["ref"],
            request,
        )
        .await
    {
        error.message = retry.message(&error.message);
        let (name, lead) = retry.lead();
        error
            .next
            .get_or_insert_with(|| Box::new(serde_json::json!({})))[name] = lead;
    }
    failure
}

/// What a failed `commits/{reference}` answer says about the request: a
/// missing ref or repository is not-found input, a credential the API
/// rejects keeps its provider error. Any other failure (rate limit, outage,
/// timeout) says nothing about the request and is returned unclassified.
fn api_refusal(
    query: &GhCloneRepoQuery,
    reference: &str,
    api: ProviderError,
) -> Result<CloneFailure, ProviderError> {
    match (api.kind, api.reason) {
        (_, Some(ProviderErrorReason::RefNotFound)) => {
            Ok(CloneFailure::Clone(CloneError {
                hints: vec![
                    "Verify the branch, tag, or SHA (ghStructure lists a ref), or omit ref for the default branch.".into(),
                ],
                next: Some(Box::new(serde_json::json!({
                    "viewRefs": crate::tools::gh_shared::ref_recovery(&query.owner, &query.repo)
                }))),
                ..CloneError::new(
                    "notFound",
                    format!(
                        "Branch, tag, or SHA not found for {}/{}: \"{reference}\"",
                        query.owner, query.repo
                    ),
                )
            }))
        }
        (ProviderErrorKind::NotFound, _) => Ok(CloneFailure::Clone(repository_not_found(query))),
        (ProviderErrorKind::Authentication | ProviderErrorKind::Permission, _) => {
            Ok(CloneFailure::Provider(api))
        }
        _ => Err(api),
    }
}

/// A path the policy denies, under the flat code the local tools share
/// ([`PolicyError::local_error_code`]), so it gets their recovery hints.
fn policy_denied(error: PolicyError) -> CloneError {
    CloneError::new(
        error.local_error_code("pathValidationFailed"),
        error.message,
    )
}

/// One clone request's resolved inputs: what its cache entries are named
/// by, checked out from, and recorded as.
struct Plan<'a> {
    query: &'a GhCloneRepoQuery,
    context: &'a CloneContext<'a>,
    repository_url: String,
    /// The web root checkouts are cloned from ([`web_root`]): it keys the
    /// cache entry beside the commit, not the URL, whose owner/repo casing
    /// would split one repository into several checkouts.
    host: String,
    sparse: Option<Sparse>,
    sparse_key: Option<String>,
    depth: u64,
}

impl Plan<'_> {
    /// The cache entry of `commit`, once the path policy admits it.
    fn target(&self, commit: &str) -> Result<PathBuf, CloneError> {
        let clone_dir = cache::clone_dir(
            &self.context.config.cache_home,
            &self.query.owner,
            &self.query.repo,
            commit,
            self.sparse_key.as_deref(),
            self.depth,
            &self.host,
        );
        self.context
            .path_policy
            .validate_output(&clone_dir)
            .map_err(policy_denied)?;
        Ok(clone_dir)
    }

    /// The identity a checkout of `commit` records and a hit must match.
    fn identity<'b>(&'b self, commit: &'b str) -> cache::Identity<'b> {
        cache::Identity {
            owner: &self.query.owner,
            repo: &self.query.repo,
            commit,
            sparse_key: self.sparse_key.as_deref(),
            depth: self.depth,
        }
    }

    fn force_refresh(&self) -> bool {
        self.query.force_refresh.unwrap_or(false)
    }

    /// The row of a checkout of `commit` at `clone_dir`: a served cache hit
    /// (`cached`); a fresh checkout clears the flag. `run` names the ref
    /// that was asked for.
    fn result(&self, commit: &str, clone_dir: &Path, hit: CacheHit) -> CloneResult {
        let bytes = hit.size;
        let (cloned_at, expires_at) = hit.age.map_or((None, None), |age| {
            (Some(age.cloned_at), Some(age.expires_at))
        });
        let paths = self.sparse.as_ref().map(Sparse::paths);
        CloneResult {
            bytes,
            next: explore_next(clone_dir, paths),
            warnings: Vec::new(),
            location: CloneLocation {
                kind: if paths.is_some() { "tree" } else { "repo" },
                local_path: clone_dir.to_string_lossy().into_owned(),
                cached: true,
                commit_sha: hit.commit_sha,
                resolved_ref: commit.to_owned(),
                requested_paths: paths.map(<[String]>::to_vec),
                symlinks: (!hit.symlinks.is_empty()).then_some(hit.symlinks),
                history_depth: (self.depth > 1).then_some(self.depth),
                cloned_at,
                expires_at,
            },
        }
    }
}

fn execute_clone(
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
    let root = web_root(context.endpoint)?;
    let plan = Plan {
        query,
        context,
        repository_url: repository_url(&root, &query.owner, &query.repo)?,
        host: root.into(),
        sparse_key: sparse.as_ref().map(Sparse::key),
        sparse,
        depth: history_depth(query),
    };
    authorize_home(context)?;
    // `run` pins every row to a commit before it gets here.
    let Some(reference) = query
        .ref_
        .as_deref()
        .filter(|reference| octocode_github::is_full_sha(reference))
    else {
        return Err(CloneError::new(
            "invalidInput",
            "ref must name the full commit SHA to check out",
        ));
    };
    let clone_dir = plan.target(reference)?;
    let _lock = cache::CloneLock::acquire(&clone_dir, context)?;
    if !plan.force_refresh()
        && let Some(hit) = cache_hit(context, &clone_dir, &plan.identity(reference))?
    {
        return Ok(plan.result(reference, &clone_dir, hit));
    }
    fresh_clone(&plan, reference, &clone_dir)
}

/// The clones root under the cache home passes the path policy, then exists.
/// The runtime allows only the checkout trees under the Octocode home, never
/// the home itself, so the clones root is what must be authorized.
fn authorize_home(context: &CloneContext<'_>) -> Result<(), CloneError> {
    let clones_root = cache::clones_root(&context.config.cache_home);
    // Existing roots and roots below an allowed workspace must pass the
    // ordinary policy check. The one exception is an explicitly configured
    // root that does not exist yet and holds the clones root: its parent lies
    // outside that root until creation. Do not create a cache directory for
    // any other denied path.
    match context.path_policy.validate_output(&clones_root) {
        Ok(_) => {}
        Err(error)
            if error.code == PolicyErrorCode::OutsideAllowedRoots
                && context
                    .path_policy
                    .allowed_roots()
                    .iter()
                    .any(|root| !root.exists() && clones_root.starts_with(root)) => {}
        Err(error) => return Err(policy_denied(error)),
    }
    // Create the clones root before validating a clone target below it:
    // otherwise a fresh root has only an existing parent outside the allowed
    // roots to canonicalize.
    std::fs::create_dir_all(&clones_root).map_err(|error| {
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
    age: Option<cache::CacheAge>,
    /// Checked-out bytes recorded at publication.
    size: u64,
    /// Repository symlinks the checkout holds as plain files.
    symlinks: Vec<String>,
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
    if commit_sha != identity.commit.to_ascii_lowercase() {
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
        age: cache::CacheAge::of(&meta, context.config.cache_ttl),
        size: meta.checkout_bytes,
        symlinks: meta.symlinks,
    }))
}

/// Check out `reference` into a stage, verify it, and publish it as the
/// cache entry `clone_dir`, whose lock the caller holds.
fn fresh_clone(
    plan: &Plan<'_>,
    reference: &str,
    clone_dir: &Path,
) -> Result<CloneResult, CloneError> {
    let context = plan.context;
    // Only a fresh clone probes git: a cache hit's failing `rev-parse`
    // already falls through to here.
    context.git.assert_available(&control(context))?;
    cache::cleanup_stale_artifacts(&context.config.cache_home);
    cache::evict(context, None);
    let stage = cache::stage_dir(&context.config.cache_home, clone_dir)?;
    let published = checkout_stage(plan, reference, &stage)
        .and_then(|commit_sha| publish_stage(plan, &stage, reference, &commit_sha, clone_dir));
    let meta = published.inspect_err(|_| discard_stage(context, &stage))?;
    context
        .path_policy
        .validate(clone_dir)
        .map_err(policy_denied)?;
    let mut result = plan.result(
        reference,
        clone_dir,
        CacheHit {
            commit_sha: meta.commit_sha.clone(),
            age: cache::CacheAge::of(&meta, context.config.cache_ttl),
            size: meta.checkout_bytes,
            symlinks: meta.symlinks,
        },
    );
    result.location.cached = false;
    cache::evict(context, Some(clone_dir));
    Ok(result)
}

/// Publish a verified stage as the cache entry `clone_dir` of `reference`.
fn publish_stage(
    plan: &Plan<'_>,
    stage: &Path,
    reference: &str,
    commit_sha: &str,
    clone_dir: &Path,
) -> Result<cache::CacheMeta, CloneError> {
    let context = plan.context;
    let meta = write_stage_meta(plan, reference, commit_sha, stage)?;
    // The target lock also covers this final check. forceRefresh bypasses
    // cache_hit, and user writes may arrive while the stage is fetched.
    if clone_dir.exists() && !git::checkout_status(context, clone_dir)?.safe_to_replace {
        return Err(dirty_checkout(clone_dir));
    }
    cache::promote(&context.config.cache_home, stage, clone_dir)?;
    Ok(meta)
}

/// Check out the commit `reference` into `stage` and verify it: every
/// sparse path exists, and the commit is the checked-out HEAD. Returns the
/// checked-out commit.
fn checkout_stage(plan: &Plan<'_>, reference: &str, stage: &Path) -> Result<String, CloneError> {
    let context = plan.context;
    let sparse_paths = plan.sparse.as_ref().map(Sparse::paths);
    git::checkout(
        context,
        &plan.repository_url,
        reference,
        sparse_paths,
        plan.depth,
        stage,
    )?;
    if let Some(paths) = sparse_paths {
        let missing = paths
            .iter()
            .filter(|path| !stage.join(path).exists())
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(sparse_not_found(plan, reference, &missing, stage));
        }
    }
    let commit_sha = git::read_head(context, stage)?;
    if commit_sha != reference.to_ascii_lowercase() {
        return Err(CloneError::new(
            "commitMismatch",
            format!("Checkout HEAD {commit_sha} does not match requested commit {reference}."),
        ));
    }
    Ok(commit_sha)
}

/// Record the checkout's identity, commit, size and symlinks in its stage.
/// Repository evidence is never overwritten by this bookkeeping.
fn write_stage_meta(
    plan: &Plan<'_>,
    resolved: &str,
    commit_sha: &str,
    stage: &Path,
) -> Result<cache::CacheMeta, CloneError> {
    let identity = plan.identity(resolved);
    // Repository evidence must never be overwritten by our bookkeeping.
    // This also rejects a directory or symlink at the reserved filename.
    match std::fs::symlink_metadata(stage.join(cache::META_FILE)) {
        Ok(_) => {
            return Err(cache::metadata_conflict(
                identity.owner,
                identity.repo,
                commit_sha,
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CloneError::new(
                "cacheUnavailable",
                format!("Could not inspect clone metadata destination: {error}"),
            ));
        }
    }
    // The size of a clean checkout cannot change: record it once, the
    // checked-out files for the row and the whole entry for eviction.
    let checkout = cache::checked_out_size(stage);
    let meta = cache::CacheMeta::new(
        &identity,
        commit_sha,
        plan.context.config.cache_ttl,
        cache::CheckoutFacts {
            size_bytes: checkout.saturating_add(cache::git_dir_size(stage)),
            checkout_bytes: checkout,
            symlinks: git::symlinks(plan.context, stage)?,
        },
    );
    cache::write_meta(stage, &meta)?;
    Ok(meta)
}

fn dirty_checkout(path: &Path) -> CloneError {
    CloneError {
        hints: vec![
            "Move your edits out of this managed checkout, restore it to clean, then retry; forceRefresh never discards local files.".into(),
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
/// input, named per path. The checkout holds the commit's trees, so the
/// recovery is a runnable lead ([`recovery::sparse_next`]); only when the
/// tree cannot be read is it a hint.
fn sparse_not_found(
    plan: &Plan<'_>,
    reference: &str,
    missing: &[&str],
    stage: &Path,
) -> CloneError {
    let query = plan.query;
    let next = recovery::sparse_next(plan.context, stage, query, missing);
    let hints = if next.is_some() {
        Vec::new()
    } else {
        vec!["Verify the path with ghStructure, or omit path for a full clone.".into()]
    };
    let paths = missing
        .iter()
        .map(|path| format!("\"{path}\""))
        .collect::<Vec<_>>()
        .join(", ");
    CloneError {
        hints,
        next: next.map(Box::new),
        ..CloneError::new(
            "pathNotFound",
            format!(
                "Path does not exist: {paths} in {}/{}@{reference}; nothing was checked out for it.",
                query.owner, query.repo
            ),
        )
    }
}

/// `next.exploreClone`: list the checkout (the sparse subtree when one was
/// requested) with structureSearch, the local entry into localSearch,
/// astSearch, and lspSearch on it. A single checked-out file is read with
/// localFetch instead: structureSearch lists directories only.
fn explore_next(clone_dir: &Path, sparse_paths: Option<&[String]>) -> serde_json::Value {
    use crate::id::ToolId;
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
        "exploreClone": crate::response::result::Continuation::new(tool, serde_json::Value::Object(query))
            .confidence(Confidence::Exact)
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
        if !is_sparse_path(path) {
            return Err(CloneError::new(
                "invalidInput",
                "path must be a non-empty repo-relative path without traversal",
            ));
        }
    }
    Ok(())
}

/// A sparse checkout path that stays inside the repository: not blank, no
/// backslash, no root or drive prefix (`/x`, `C:x`), and no `.` or `..`
/// segment (an interior `a/./b` included, which `Path::components`
/// would silently normalize away). A trailing `/` names a directory.
fn is_sparse_path(path: &str) -> bool {
    let drive = path.as_bytes().get(1) == Some(&b':')
        && path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic);
    !path.trim().is_empty()
        && !path.contains('\\')
        && !path.starts_with('/')
        && !drive
        && path
            .strip_suffix('/')
            .unwrap_or(path)
            .split('/')
            .all(|segment| !matches!(segment, "." | ".."))
}

/// The git web root of `endpoint` that repositories are cloned from:
/// `https://github.com` for the public API, the GitHub Enterprise host (and
/// any path prefix before `/api/v3`) otherwise.
fn web_root(endpoint: &GitHubEndpoint) -> Result<url::Url, CloneError> {
    let base = endpoint
        .rest(&[])
        .map_err(|error| CloneError::new("configuration", error.message))?;
    let secure = base.scheme() == "https"
        || (base.scheme() == "http" && octocode_github::is_loopback(&base));
    if !secure
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
    {
        return Err(unsupported_endpoint());
    }
    let path = base.path().trim_end_matches('/');
    if base.host_str() == Some("api.github.com") && path.is_empty() {
        return url::Url::parse("https://github.com").map_err(|_| unsupported_endpoint());
    }
    let Some(prefix) = path.strip_suffix("/api/v3") else {
        return Err(unsupported_endpoint());
    };
    let mut url = base.clone();
    url.set_path(prefix);
    Ok(url)
}

/// The git URL of `owner/repo` under `root` ([`web_root`]).
fn repository_url(root: &url::Url, owner: &str, repo: &str) -> Result<String, CloneError> {
    let mut url = root.clone();
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
        "ghCloneRepo requires an HTTPS GitHub API endpoint (plain http only on loopback): https://api.github.com or a GitHub Enterprise endpoint ending in /api/v3, without credentials or query parameters. Use ghGetFileContent for other API proxies.",
    )
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
impl crate::response::output::ToolOutput for Output {
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

    /// A persistent cache runs under the runtime's constant limits; a test
    /// runtime's limits replace them field for field.
    #[test]
    fn persistent_defaults_are_the_runtime_constants_and_limits_apply() {
        let base = CloneConfig::persistent("/h");
        assert_eq!(base.cache_ttl, Duration::from_secs(24 * 60 * 60));
        assert_eq!(base.max_cache_size_bytes, 2 * 1024 * 1024 * 1024);
        assert_eq!(base.max_clone_count, 50);
        assert_eq!(base.network_timeout, Duration::from_secs(30));
        let config = CloneConfig::persistent("/h").with_limits(&crate::execution::CloneLimits {
            ttl: Duration::from_secs(120),
            max_bytes: 1024 * 1024,
            max_clones: 7,
        });
        assert_eq!(config.cache_ttl, Duration::from_secs(120));
        assert_eq!(config.max_clone_count, 7);
        assert_eq!(config.max_cache_size_bytes, 1024 * 1024);
    }
}
