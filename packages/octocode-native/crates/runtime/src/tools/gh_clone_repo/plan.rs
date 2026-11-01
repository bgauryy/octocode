//! One fresh clone: resolve its destination, hold its lock and publish a verified checkout.
use super::checkout::fresh_clone;
use super::validate::{repository_url, validate_query, web_root};
use super::{
    CloneContext, CloneError, CloneLocation, CloneResult, GhCloneRepoQuery, Sparse, check_control,
    history_depth, policy_denied, storage,
};
use crate::policy::PolicyErrorCode;
use octocode_contract_types::ErrorCode;
use std::path::{Path, PathBuf};

/// One clone request's resolved checkout identity and destination.
pub(super) struct Plan<'a> {
    pub(super) query: &'a GhCloneRepoQuery,
    pub(super) context: &'a CloneContext<'a>,
    pub(super) repository_url: String,
    /// The web root checkouts are cloned from ([`web_root`]): it keys the
    /// destination beside the commit, not the URL, whose owner/repo casing
    /// would split one repository into several checkouts.
    host: String,
    pub(super) sparse: Option<Sparse>,
    sparse_key: Option<String>,
    pub(super) depth: u64,
}

impl Plan<'_> {
    /// The destination of `commit`, once the path policy admits it.
    fn target(&self, commit: &str) -> Result<PathBuf, CloneError> {
        let clone_dir = storage::clone_dir(
            &self.context.config.home,
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

    pub(super) fn result(&self, commit: &str, clone_dir: &Path, checkout: Checkout) -> CloneResult {
        let bytes = checkout.size;
        let paths = self.sparse.as_ref().map(Sparse::paths);
        CloneResult {
            bytes,
            calls: Some(explore_next(clone_dir, paths)),
            warnings: Vec::new(),
            location: CloneLocation {
                kind: if paths.is_some() { "tree" } else { "repo" },
                local_path: clone_dir.to_string_lossy().into_owned(),
                commit_sha: checkout.commit_sha,
                resolved_ref: commit.to_owned(),
                requested_paths: paths.map(<[String]>::to_vec),
                symlinks: (!checkout.symlinks.is_empty()).then_some(checkout.symlinks),
                history_depth: (self.depth > 1).then_some(self.depth),
                cloned_at: Some(crate::civil_date::iso8601_millis(
                    crate::civil_date::unix_millis(),
                )),
            },
        }
    }
}

pub(super) fn execute_clone(
    query: &GhCloneRepoQuery,
    context: &CloneContext<'_>,
) -> Result<CloneResult, CloneError> {
    check_control(context)?;
    super::require_persistent_storage(context.config.persistent)?;
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
    let _lock = storage::CloneLock::acquire(&clone_dir, context)?;
    fresh_clone(&plan, reference, &clone_dir)
}

/// The clones root under the home passes the path policy, then exists.
/// The runtime allows only the checkout trees under the Octocode home, never
/// the home itself, so the clones root is what must be authorized.
fn authorize_home(context: &CloneContext<'_>) -> Result<(), CloneError> {
    let clones_root = storage::clones_root(&context.config.home);
    // Existing roots and roots below an allowed workspace must pass the
    // ordinary policy check. The one exception is an explicitly configured
    // root that does not exist yet and holds the clones root: its parent lies
    // outside that root until creation. Do not create a output directory for
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
            ErrorCode::FileAccessFailed,
            format!("Could not initialize the configured clone output directory: {error}"),
        )
    })?;
    Ok(())
}

/// Facts read from a newly verified checkout.
pub(super) struct Checkout {
    pub(super) commit_sha: String,
    pub(super) size: u64,
    pub(super) symlinks: Vec<String>,
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
        query.insert("operation".into(), serde_json::json!("tree"));
        ToolId::StructureSearch
    };
    serde_json::json!({
        "exploreClone": crate::response::result::Continuation::related(tool, serde_json::Value::Object(query))
            .build()
    })
}
