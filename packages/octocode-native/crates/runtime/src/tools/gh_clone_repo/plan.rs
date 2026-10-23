//! One clone request's resolved plan: its cache entry, a served cache hit, and the row it answers with.
use super::checkout::{dirty_checkout, fresh_clone};
use super::validate::{repository_url, validate_query, web_root};
use super::{
    CloneContext, CloneError, CloneLocation, CloneResult, GhCloneRepoQuery, Sparse, cache,
    check_control, git, history_depth, policy_denied,
};
use crate::policy::PolicyErrorCode;
use crate::response::result::Confidence;
use octocode_contract_types::ErrorCode;
use std::path::{Path, PathBuf};

/// One clone request's resolved inputs: what its cache entries are named
/// by, checked out from, and recorded as.
pub(super) struct Plan<'a> {
    pub(super) query: &'a GhCloneRepoQuery,
    pub(super) context: &'a CloneContext<'a>,
    pub(super) repository_url: String,
    /// The web root checkouts are cloned from ([`web_root`]): it keys the
    /// cache entry beside the commit, not the URL, whose owner/repo casing
    /// would split one repository into several checkouts.
    host: String,
    pub(super) sparse: Option<Sparse>,
    sparse_key: Option<String>,
    pub(super) depth: u64,
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
    pub(super) fn identity<'b>(&'b self, commit: &'b str) -> cache::Identity<'b> {
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
    pub(super) fn result(&self, commit: &str, clone_dir: &Path, hit: CacheHit) -> CloneResult {
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

pub(super) fn execute_clone(
    query: &GhCloneRepoQuery,
    context: &CloneContext<'_>,
) -> Result<CloneResult, CloneError> {
    check_control(context)?;
    if !context.config.persistent {
        return Err(CloneError::new(
            ErrorCode::PersistentStorageDisabled,
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
            ErrorCode::InvalidInput,
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
            ErrorCode::CacheUnavailable,
            format!("Could not initialize the configured clone cache home: {error}"),
        )
    })?;
    Ok(())
}

/// A served cache hit: commit and age of a clean checkout that still matches
/// the requested identity.
pub(super) struct CacheHit {
    pub(super) commit_sha: String,
    pub(super) age: Option<cache::CacheAge>,
    /// Checked-out bytes recorded at publication.
    pub(super) size: u64,
    /// Repository symlinks the checkout holds as plain files.
    pub(super) symlinks: Vec<String>,
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
