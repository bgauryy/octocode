//! A fresh checkout: stage the commit, verify it, publish it as an output.
use super::plan::{Checkout, Plan};
use super::{CloneError, CloneResult, Sparse, control, git, policy_denied, recovery, storage};
use octocode_contract_types::ErrorCode;
use std::path::Path;

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
    if !clone_dir.is_dir() {
        return Ok(None);
    }
    let context = plan.context;
    context.git.assert_available(&control(context))?;
    if !git::safe_to_replace(context, clone_dir, reference)? {
        return Ok(None);
    }
    let checkout = Checkout {
        commit_sha: reference.to_ascii_lowercase(),
        size: storage::checked_out_size(clone_dir),
        symlinks: git::symlinks(context, clone_dir)?,
    };
    context
        .path_policy
        .validate(clone_dir)
        .map_err(policy_denied)?;
    let mut result = plan.result(reference, clone_dir, checkout);
    result.location.cloned_at = storage::completed_at(clone_dir);
    Ok(Some(result))
}

/// Check out `reference` into a stage, verify it, and publish it as the
/// destination `clone_dir`, whose lock the caller holds.
pub(super) fn fresh_clone(
    plan: &Plan<'_>,
    reference: &str,
    clone_dir: &Path,
) -> Result<CloneResult, CloneError> {
    let context = plan.context;
    context.git.assert_available(&control(context))?;
    if clone_dir.exists() && !git::safe_to_replace(context, clone_dir, reference)? {
        return Err(dirty_checkout(clone_dir));
    }
    let stage = storage::stage_dir(&context.config.home, clone_dir)?;
    let published = checkout_stage(plan, reference, &stage)
        .and_then(|commit_sha| publish_stage(plan, &stage, &commit_sha, clone_dir));
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
    stage: &Path,
    commit_sha: &str,
    clone_dir: &Path,
) -> Result<Checkout, CloneError> {
    let context = plan.context;
    let checkout = Checkout {
        commit_sha: commit_sha.to_owned(),
        size: storage::checked_out_size(stage),
        symlinks: git::symlinks(context, stage)?,
    };
    // The lock serializes only Octocode: a user can create or edit the
    // destination while the stage is fetched, so recheck before replacing.
    if clone_dir.exists() && !git::safe_to_replace(context, clone_dir, commit_sha)? {
        return Err(dirty_checkout(clone_dir));
    }
    storage::record_completion(stage)?;
    storage::promote(stage, clone_dir)?;
    Ok(checkout)
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
            .filter(|path| !checked_out(stage, path))
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
    Ok(commit_sha)
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
            "Move your edits out of this managed checkout, restore it to clean, then retry; cloning never discards local files.".into(),
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
