//! A fresh checkout: stage the commit, verify it, record its metadata, and publish it as the cache entry.
use super::plan::{CacheHit, Plan};
use super::{
    CloneContext, CloneError, CloneResult, Sparse, cache, control, git, policy_denied, recovery,
};
use octocode_contract_types::ErrorCode;
use std::path::Path;

/// Check out `reference` into a stage, verify it, and publish it as the
/// cache entry `clone_dir`, whose lock the caller holds.
pub(super) fn fresh_clone(
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
                ErrorCode::CacheUnavailable,
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

pub(super) fn dirty_checkout(path: &Path) -> CloneError {
    CloneError {
        hints: vec![
            "Move your edits out of this managed checkout, restore it to clean, then retry; forceRefresh never discards local files.".into(),
        ],
        ..CloneError::new(
            ErrorCode::CheckoutDirty,
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
            ErrorCode::PathNotFound,
            format!(
                "Path does not exist: {paths} in {}/{}@{reference}; nothing was checked out for it.",
                query.owner, query.repo
            ),
        )
    }
}
