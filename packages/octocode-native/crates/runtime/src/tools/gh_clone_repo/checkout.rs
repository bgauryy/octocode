//! A fresh checkout: stage the commit, verify it, publish it as an output.
use super::plan::{Checkout, Plan};
use super::{CloneContext, CloneError, CloneResult, Sparse, git, policy_denied, recovery, storage};
use octocode_contract_types::ErrorCode;
use std::path::{Path, PathBuf};

/// The checkout already published at `clone_dir` (whose lock the caller
/// holds), when it is still exactly the commit `reference`: the destination
/// is keyed by that commit, scope and depth, and verification is the one
/// replacement uses ([`git::safe_to_replace`]): HEAD is the commit, the work
/// tree has no modified, untracked or ignored files, and no local refs exist.
/// `None` when there is no checkout or it differs, so the caller fetches fresh.
pub(super) fn reuse_checkout(
    plan: &Plan<'_>,
    reference: &str,
    clone_dir: &Path,
) -> Result<Option<CloneResult>, CloneError> {
    let context = plan.context;
    // Only a linked worktree of the shared store is reusable: any other
    // checkout is replaced when clean, as a differing one is.
    if !clone_dir.is_dir() || !storage::is_store_worktree(&context.config.home, clone_dir) {
        return Ok(None);
    }
    // The checks read one checkout and none depends on another: they run
    // together. A checkout that is not exactly the commit is not reusable.
    let Some(inspection) = git::inspect_checkout(
        context,
        clone_dir,
        reference,
        plan.sparse.as_ref().map(Sparse::paths),
    )?
    else {
        return Ok(None);
    };
    let listed = inspection.listed;
    // A checkout made before credential-named files were withheld still
    // holds them: it is not reusable, the caller replaces it. The blobs
    // count too: an earlier full clone stored them unwritten.
    if listed
        .iter()
        .any(|file| clone_dir.join(&file.path).exists())
        || !git::blobs_absent(context, clone_dir, &listed)?
    {
        return Ok(None);
    }
    let withheld = git::withheld_paths(listed);
    let checkout = Checkout {
        commit_sha: reference.to_ascii_lowercase(),
        size: storage::checked_out_size(clone_dir),
        symlinks: inspection.symlinks,
        withheld,
    };
    context
        .path_policy
        .validate(clone_dir)
        .map_err(policy_denied)?;
    let mut result = plan.result(reference, clone_dir, checkout);
    result.location.cloned_at = storage::completed_at(clone_dir);
    result.location.reused = true;
    Ok(Some(result))
}

/// Check out `reference` as a worktree stage of the shared store, verify it,
/// and publish it as the destination `clone_dir`, whose lock the caller
/// holds. The store's lock is held from here until the stage is published or
/// discarded: the store's config, shallow boundaries and worktree list are
/// shared by every row of the repository.
pub(super) fn fresh_clone(
    plan: &Plan<'_>,
    reference: &str,
    clone_dir: &Path,
) -> Result<CloneResult, CloneError> {
    let context = plan.context;
    if clone_dir.exists() && !git::safe_to_replace(context, clone_dir, reference)? {
        return Err(dirty_checkout(clone_dir));
    }
    let key = plan.store_key(reference);
    let _store_lock = storage::CloneLock::acquire(&key.anchor(), context)?;
    let stage = storage::stage_dir(&context.config.home, clone_dir)?;
    let published =
        checkout_stage(plan, reference, &key, &stage).and_then(|(store, commit_sha, withheld)| {
            let checkout = publish_stage(plan, &store, &stage, &commit_sha, withheld, clone_dir)?;
            retire_old_stores(context, &key, &store);
            Ok(checkout)
        });
    let checkout = published.inspect_err(|_| discard_stage(&stage))?;
    context
        .path_policy
        .validate(clone_dir)
        .map_err(policy_denied)?;
    let mut result = plan.result(reference, clone_dir, checkout);
    result.location.cloned_at = storage::completed_at(clone_dir);
    Ok(result)
}

/// Publish a verified stage as the destination `clone_dir` after verification.
fn publish_stage(
    plan: &Plan<'_>,
    store: &Path,
    stage: &Path,
    commit_sha: &str,
    withheld: Vec<String>,
    clone_dir: &Path,
) -> Result<Checkout, CloneError> {
    let context = plan.context;
    let checkout = Checkout {
        commit_sha: commit_sha.to_owned(),
        size: storage::checked_out_size(stage),
        symlinks: git::symlinks(context, stage)?,
        withheld,
    };
    // The lock serializes only Octocode: a user can create or edit the
    // destination while the stage is fetched, so recheck before replacing.
    if clone_dir.exists() && !git::safe_to_replace(context, clone_dir, commit_sha)? {
        return Err(dirty_checkout(clone_dir));
    }
    storage::record_completion(stage)?;
    storage::promote(stage, clone_dir, || {
        // The destination it replaced, now moved aside, may have been a
        // worktree of this store: git refuses to move onto a registered one.
        git::prune_worktrees(context, store)?;
        git::move_worktree(context, store, stage, clone_dir)
    })?;
    Ok(checkout)
}

/// Check out the commit `reference` into `stage` and verify it: every
/// sparse path exists, and the commit is the checked-out HEAD. Returns the
/// checked-out commit and the credential-named files left out of it.
fn checkout_stage(
    plan: &Plan<'_>,
    reference: &str,
    key: &storage::StoreKey,
    stage: &Path,
) -> Result<(PathBuf, String, Vec<String>), CloneError> {
    let context = plan.context;
    let sparse_paths = plan.sparse.as_ref().map(Sparse::paths);
    let materialize = |store: &Path| {
        git::checkout(
            context,
            &plan.repository_url,
            store,
            reference,
            sparse_paths,
            plan.depth,
            stage,
        )
    };
    // A store that already holds a blob this checkout withholds is never
    // cleaned in place: worktrees may still use it. A new generation is.
    let mut store = key.current();
    let withheld = match materialize(&store)? {
        Some(withheld) => withheld,
        None => {
            store = key.next_dir();
            materialize(&store)?.ok_or_else(|| {
                CloneError::new(
                    ErrorCode::GitFailed,
                    "A fresh store already holds credential-named files this checkout withholds; nothing was cloned.",
                )
            })?
        }
    };
    if let Some(paths) = sparse_paths {
        let missing = paths
            .iter()
            .filter(|path| !checked_out(stage, path) && !is_withheld(&withheld, path))
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(sparse_not_found(plan, reference, &missing, stage));
        }
    }
    let commit_sha = git::read_head(context, stage)?;
    if commit_sha != reference.to_ascii_lowercase() {
        return Err(CloneError::new(
            ErrorCode::CommitMismatch,
            format!("Checkout HEAD {commit_sha} does not match requested commit {reference}."),
        ));
    }
    Ok((store, commit_sha, withheld))
}

/// Remove the older generations of the store `key` once no worktree uses
/// them (a crashed row's stage is removed first): after pruning the ones whose directories are gone, a store with no
/// worktree left holds nothing a checkout needs. One that cannot be pruned
/// stays.
fn retire_old_stores(context: &CloneContext<'_>, key: &storage::StoreKey, current: &Path) {
    // A crashed row's stage keeps its generation in use until it is gone.
    storage::remove_crashed_stages(&context.config.home);
    for generation in key.generations() {
        let store = key.dir(generation);
        if store != current
            && git::prune_worktrees(context, &store).is_ok()
            && storage::has_no_worktrees(&store)
        {
            storage::remove_dir(&store);
        } else if store == current {
            // Registrations of the removed stages.
            let _ = git::prune_worktrees(context, &store);
        }
    }
}

/// Whether `path` (a requested sparse path) is a withheld file or holds one:
/// its absence from the checkout is by design, not a missing path.
fn is_withheld(withheld: &[String], path: &str) -> bool {
    let path = path.trim_end_matches('/');
    withheld.iter().any(|file| {
        file == path
            || file
                .strip_prefix(path)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// Whether `path` is in the checkout `stage` under exactly that spelling.
/// A commit's paths are case-sensitive, but a case-insensitive filesystem
/// (macOS, Windows) finds `src/Nested` at `src/nested`: each segment must
/// equal a directory entry's name byte for byte.
fn checked_out(stage: &Path, path: &str) -> bool {
    let mut dir = stage.to_path_buf();
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .all(|segment| {
            let found = std::fs::read_dir(&dir).is_ok_and(|mut entries| {
                entries.any(|entry| entry.is_ok_and(|entry| entry.file_name() == segment))
            });
            dir.push(segment);
            found
        })
}

pub(super) fn dirty_checkout(path: &Path) -> CloneError {
    CloneError {
        hints: vec![
            "Move your edits out of this managed checkout, restore it to clean, then retry; cloning never discards local files. If its shared store was deleted, git cannot read it: delete the directory to have it cloned again.".into(),
        ],
        ..CloneError::new(
            ErrorCode::CheckoutDirty,
            format!("Clone checkout '{}' contains local files, changes, commits or Git refs; it was preserved without replacement.", path.display()),
        )
    }
}

fn discard_stage(stage: &Path) {
    storage::remove_dir(stage);
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
        calls: next.map(Box::new),
        ..CloneError::new(
            ErrorCode::PathNotFound,
            format!(
                "Path does not exist: {paths} in {}/{}@{reference}; nothing was checked out for it.",
                query.owner, query.repo
            ),
        )
    }
}
