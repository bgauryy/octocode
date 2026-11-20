//! One fresh clone: resolve its destination, hold its lock and publish a verified checkout.
use super::checkout::{fresh_clone, reuse_checkout};
use super::validate::validate_query;
use super::{
    CloneLocation, CloneResult, GhCloneRepoQuery, GitContext, GitError, Sparse, check_control,
    control, history_depth, policy_denied, storage,
};
use octocode_contract_types::ErrorCode;
use octocode_runtime_base::policy::PolicyErrorCode;
use octocode_tools_shared::gh_git::endpoint::{repository_url, web_root};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// One clone request's resolved checkout identity and destination.
pub(super) struct Plan<'a> {
    pub(super) query: &'a GhCloneRepoQuery,
    pub(super) context: &'a GitContext<'a>,
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
    fn target(&self, commit: &str) -> Result<PathBuf, GitError> {
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

    /// The shared store that holds `commit` for this row ([`storage::StoreKey`]).
    pub(super) fn store_key(&self, commit: &str) -> storage::StoreKey {
        storage::StoreKey::new(
            &self.context.config.home,
            &self.query.owner,
            &self.query.repo,
            &self.host,
            self.depth,
            commit,
        )
    }

    /// The served row of the checkout at `clone_dir`: where it is and what it
    /// holds. Requested sparse paths the commit lacks are named in a warning
    /// with their recovery lead; the checkout holds the others.
    pub(super) fn result(&self, commit: &str, clone_dir: &Path, checkout: Checkout) -> CloneResult {
        let bytes = checkout.size;
        let missing = self.missing_paths(clone_dir, &checkout.withheld);
        let paths = self.sparse.as_ref().map(|sparse| {
            sparse
                .paths()
                .iter()
                .filter(|path| !missing.contains(&path.as_str()))
                .cloned()
                .collect()
        });
        let path = clone_dir.to_string_lossy().into_owned();
        let mut calls = Map::new();
        let mut warnings = Vec::new();
        if !missing.is_empty() {
            warnings.push(format!(
                "Path does not exist: {} in {}/{}@{}; the checkout holds the other paths.",
                missing
                    .iter()
                    .map(|path| format!("\"{path}\""))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.query.owner,
                self.query.repo,
                checkout.commit_sha
            ));
            if let Some(Value::Object(recovery)) =
                super::recovery::sparse_next(self.context, clone_dir, self.query, &missing)
            {
                calls.extend(recovery);
            }
        }
        CloneResult {
            bytes,
            calls: (!calls.is_empty()).then_some(Value::Object(calls)),
            warnings,
            location: CloneLocation {
                path,
                commit_sha: checkout.commit_sha,
                ref_: commit.to_owned(),
                paths,
                symlinks: (!checkout.symlinks.is_empty()).then_some(checkout.symlinks),
                skipped: (!checkout.withheld.is_empty()).then_some(checkout.withheld),
                history_depth: (self.depth > 1).then_some(self.depth),
                cloned_at: None,
                reused: false,
            },
        }
    }

    /// The requested sparse paths absent from the checkout at `dir`: neither
    /// checked out under that spelling nor withheld credential files.
    pub(super) fn missing_paths<'p>(&'p self, dir: &Path, withheld: &[String]) -> Vec<&'p str> {
        self.sparse
            .as_ref()
            .map(Sparse::paths)
            .into_iter()
            .flatten()
            .filter(|path| {
                !super::checkout::checked_out(dir, path)
                    && !super::checkout::is_withheld(withheld, path)
            })
            .map(String::as_str)
            .collect()
    }
}

/// A clone with its commit pinned: [`prepare_clone`], then
/// [`execute_prepared`].
#[cfg(test)]
pub(super) fn execute_clone(
    query: &GhCloneRepoQuery,
    context: &GitContext<'_>,
) -> Result<CloneResult, GitError> {
    check_control(context)?;
    super::require_persistent_storage(context.config.persistent)?;
    validate_query(query)?;
    prepare_clone(context)?;
    execute_prepared(query, context)
}

/// What a clone needs whatever commit it checks out: the clones root
/// passes the path policy and exists, and git runs. None of it reads the
/// pin, so `run` does it while the API still answers the pin.
pub(super) fn prepare_clone(context: &GitContext<'_>) -> Result<(), GitError> {
    check_control(context)?;
    super::require_persistent_storage(context.config.persistent)?;
    authorize_home(context)?;
    context.git.assert_available(&control(context))
}

/// The clone of a pinned `query` once [`prepare_clone`] succeeded.
pub(super) fn execute_prepared(
    query: &GhCloneRepoQuery,
    context: &GitContext<'_>,
) -> Result<CloneResult, GitError> {
    check_control(context)?;
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
    // `run` pins every row to a commit before it gets here.
    let Some(reference) = query
        .ref_
        .as_deref()
        .filter(|reference| octocode_github::is_full_sha(reference))
    else {
        return Err(GitError::new(
            ErrorCode::InvalidInput,
            "ref must name the full commit SHA to check out",
        ));
    };
    let clone_dir = plan.target(reference)?;
    let _lock = storage::CloneLock::acquire(&clone_dir, context)?;
    match reuse_checkout(&plan, reference, &clone_dir)? {
        Some(reused) => Ok(reused),
        None => fresh_clone(&plan, reference, &clone_dir),
    }
}

/// The clones root under the home passes the path policy, then exists.
/// The runtime allows only the checkout trees under the Octocode home, never
/// the home itself, so the clones root is what must be authorized.
fn authorize_home(context: &GitContext<'_>) -> Result<(), GitError> {
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
    storage::create_private_dir_all(&clones_root).map_err(|error| {
        storage::write_denied(GitError::new(
            ErrorCode::FileAccessFailed,
            format!("Could not initialize the configured clone output directory: {error}"),
        ))
    })?;
    Ok(())
}

/// Facts read from a newly verified checkout.
pub(super) struct Checkout {
    pub(super) commit_sha: String,
    pub(super) size: u64,
    pub(super) symlinks: Vec<String>,
    /// Credential-named files in scope, left out of the checkout.
    pub(super) withheld: Vec<String>,
}
