//! A fresh checkout: stage the commit, verify it, publish it as an output.
use super::plan::{Checkout, Fresh, Plan};
use super::{CloneResult, GitContext, GitError, Sparse, git, policy_denied, recovery, storage};
use octocode_contract_types::ErrorCode;
use std::path::{Path, PathBuf};

/// The checkout already published at `clone_dir` (whose lock the caller
/// holds), when it is still exactly the commit `reference`: the destination
/// is keyed by that commit and scope, and verification is the one
/// replacement uses ([`git::safe_to_replace`]): HEAD is the commit, the work
/// tree has no modified, untracked or ignored files, and no local refs exist.
/// `None` when there is no checkout or it differs, so the caller fetches fresh.
pub(super) fn reuse_checkout(
    plan: &Plan<'_>,
    reference: &str,
    clone_dir: &Path,
) -> Result<Option<CloneResult>, GitError> {
    let context = plan.context;
    // Only a linked worktree of the shared store is reusable: any other
    // checkout is replaced when clean, as a differing one is.
    if !clone_dir.is_dir() {
        return Ok(None);
    }
    // A home that moved leaves the checkout pointing into the old one.
    let key = plan.store_key();
    if storage::moved_store(&key, clone_dir).is_some() {
        let _store_lock = storage::CloneLock::acquire(&key.anchor(), context)?;
        reconnect_moved(context, &key, clone_dir)?;
    }
    if !storage::worktree_store(&context.config.home, clone_dir)
        .is_some_and(|store| storage::has_verified_history(&store))
    {
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
        root_files: plan.root_files(clone_dir),
        withheld,
    };
    context
        .path_policy
        .validate(clone_dir)
        .map_err(policy_denied)?;
    // A checkout made before scopes were recorded gets its record now.
    storage::record_scope(clone_dir, &record(plan, clone_dir, &checkout))?;
    let mut result = plan.result(clone_dir, checkout);
    result.location.cloned_at = storage::completed_at(clone_dir);
    result.location.reused = true;
    Ok(Some(result))
}

/// Check out `reference` as a worktree stage of the shared store, verify it,
/// and publish it as the destination `clone_dir`, whose lock the caller
/// holds. The store's lock is held from here until the stage is published or
/// discarded: the store's config, shallow boundaries and worktree list are
/// shared by every row of the repository. A scope with a path the commit
/// lacks stages nothing: it is [`Fresh::Narrowed`] to the paths it has.
pub(super) fn fresh_clone(
    plan: &Plan<'_>,
    reference: &str,
    clone_dir: &Path,
) -> Result<Fresh, GitError> {
    let context = plan.context;
    if clone_dir.exists() && !git::safe_to_replace(context, clone_dir, reference)? {
        return Err(dirty_checkout(clone_dir));
    }
    let key = plan.store_key();
    let _store_lock = storage::CloneLock::acquire(&key.anchor(), context)?;
    // Before the store prunes the registrations whose directories are gone:
    // a checkout of a moved home would read as gone.
    reconnect_moved(context, &key, clone_dir)?;
    let stage = storage::stage_dir(&context.config.home, clone_dir)?;
    let staging =
        checkout_stage(plan, reference, &key, &stage).inspect_err(|_| discard_stage(&stage))?;
    let (store, commit_sha, staged) = match staging {
        Staging::Staged {
            store,
            commit_sha,
            staged,
        } => (store, commit_sha, staged),
        Staging::Narrowed { present, absent } => {
            discard_stage(&stage);
            return Ok(Fresh::Narrowed { present, absent });
        }
    };
    let checkout = publish_stage(plan, &store, &stage, &commit_sha, staged, clone_dir)
        .inspect_err(|_| discard_stage(&stage))?;
    retire_old_stores(context, &key, &store);
    context
        .path_policy
        .validate(clone_dir)
        .map_err(policy_denied)?;
    let mut result = plan.result(clone_dir, checkout);
    result.location.cloned_at = storage::completed_at(clone_dir);
    Ok(Fresh::Published(Box::new(result)))
}

/// What [`checkout_stage`] made: a verified stage, or nothing for a scope
/// the commit lacks a path of.
enum Staging {
    Staged {
        store: PathBuf,
        commit_sha: String,
        staged: git::Staged,
    },
    /// Some requested sparse paths are not in the commit (see
    /// [`Fresh::Narrowed`]).
    Narrowed {
        present: Vec<String>,
        absent: Vec<String>,
    },
}

/// What the checkout `dir` that `checkout` describes records for the local
/// tools.
fn record(
    plan: &Plan<'_>,
    dir: &Path,
    checkout: &Checkout,
) -> octocode_tools_shared::gh_git::clone_scope::CloneRecord {
    plan.record(
        &checkout.commit_sha,
        plan.held_paths(dir, &checkout.withheld),
        checkout.root_files,
    )
}

/// Publish a verified stage as the destination `clone_dir` after verification.
fn publish_stage(
    plan: &Plan<'_>,
    store: &Path,
    stage: &Path,
    commit_sha: &str,
    staged: git::Staged,
    clone_dir: &Path,
) -> Result<Checkout, GitError> {
    let context = plan.context;
    let checkout = Checkout {
        commit_sha: commit_sha.to_owned(),
        size: storage::checked_out_size(stage),
        symlinks: git::present_symlinks(stage, staged.links),
        root_files: plan.root_files(stage),
        withheld: staged.withheld,
    };
    // The lock serializes only Octocode: a user can create or edit the
    // destination while the stage is fetched, so recheck before replacing.
    if clone_dir.exists() && !git::safe_to_replace(context, clone_dir, commit_sha)? {
        return Err(dirty_checkout(clone_dir));
    }
    storage::record_completion(stage)?;
    storage::record_scope(stage, &record(plan, stage, &checkout))?;
    // The destination it replaces, when a linked worktree, moves aside and
    // back through git, which keeps its registration: the prune before the
    // placement drops only registrations of directories that are gone (a
    // checkout the user replaced by another), never the moved-aside one, so a
    // failed placement restores a worktree git still knows.
    // [`retire_old_stores`] prunes the registration of the removed one.
    let previous_store = storage::worktree_store(&context.config.home, clone_dir);
    storage::promote(
        stage,
        clone_dir,
        |from, to| match &previous_store {
            Some(previous) => git::move_worktree(context, previous, from, to),
            None => storage::rename_dir(from, to),
        },
        || {
            // Only a registration besides the stage's can be one of a
            // directory that is gone.
            if storage::registered_worktrees(store) > 1 {
                git::prune_worktrees(context, store)?;
            }
            git::move_worktree(context, store, stage, clone_dir)
        },
    )?;
    Ok(checkout)
}

/// Reconnect the checkouts beside `clone_dir` (those of one repository) whose
/// `.git` pointer names one of `key`'s stores by a path of an earlier home,
/// with the store lock of `key` held.
fn reconnect_moved(
    context: &GitContext<'_>,
    key: &storage::StoreKey,
    clone_dir: &Path,
) -> Result<(), GitError> {
    let Some(repository) = clone_dir.parent() else {
        return Ok(());
    };
    let mut by_store = Vec::<(PathBuf, Vec<PathBuf>)>::new();
    for checkout in std::fs::read_dir(repository)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
    {
        let Some(store) = storage::moved_store(key, &checkout) else {
            continue;
        };
        match by_store.iter_mut().find(|(known, _)| *known == store) {
            Some((_, checkouts)) => checkouts.push(checkout),
            None => by_store.push((store, vec![checkout])),
        }
    }
    for (store, checkouts) in &by_store {
        git::reconnect_worktrees(context, store, checkouts)?;
    }
    Ok(())
}

/// Check out the commit `reference` into `stage` and verify it: every
/// sparse path exists, and the commit is the checked-out HEAD. Returns the
/// checked-out commit and the credential-named files left out of it, or,
/// for a scope with a path the commit lacks, the scope narrowed to the
/// paths it has (nothing staged).
fn checkout_stage(
    plan: &Plan<'_>,
    reference: &str,
    key: &storage::StoreKey,
    stage: &Path,
) -> Result<Staging, GitError> {
    let context = plan.context;
    let sparse_paths = plan.sparse.as_ref().map(Sparse::paths);
    let materialize = |store: &Path| {
        git::checkout(
            context,
            &plan.repository_url,
            store,
            reference,
            sparse_paths,
            stage,
        )
    };
    // A store that already holds a blob this checkout withholds is never
    // cleaned in place: worktrees may still use it. A new generation is.
    // The same goes for one git reports damaged: a failed step is told
    // from a failure of the network or the credential by git's own check of
    // the store ([`git::store_intact`]), never by the failure's text.
    let mut store = key.current();
    let first = match materialize(&store) {
        Err(error)
            if error.code == ErrorCode::GitFailed && !git::store_intact(context, &store)? =>
        {
            None
        }
        other => other?,
    };
    let staged = match first {
        Some(staged) => staged,
        None => {
            store = key.next_dir();
            materialize(&store)?.ok_or_else(|| {
                GitError::new(
                    ErrorCode::GitFailed,
                    "A fresh store already holds credential-named files this checkout withholds; nothing was cloned.",
                )
            })?
        }
    };
    // A scope none of whose paths exists checks out nothing: the row fails.
    // With any path present the scope narrows to those, whose checkout
    // serves the row and names the missing ones ([`Plan::result`]).
    if let Some(paths) = sparse_paths
        && !staged.missing.is_empty()
    {
        if staged.missing.len() == paths.len() {
            let missing = staged
                .missing
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            return Err(sparse_not_found(plan, reference, &missing, &store));
        }
        let present = paths
            .iter()
            .filter(|path| !staged.missing.contains(path))
            .cloned()
            .collect();
        return Ok(Staging::Narrowed {
            present,
            absent: staged.missing,
        });
    }
    // `checkout --detach` leaves the commit id in the worktree's HEAD file.
    let commit_sha = match storage::detached_head(stage) {
        Some(commit) => commit,
        None => git::read_head(context, stage)?,
    };
    if commit_sha != reference.to_ascii_lowercase() {
        return Err(GitError::new(
            ErrorCode::CommitMismatch,
            format!("Checkout HEAD {commit_sha} does not match requested commit {reference}."),
        ));
    }
    Ok(Staging::Staged {
        store,
        commit_sha,
        staged,
    })
}

/// Remove the older generations of the store `key` once no worktree uses
/// them (a crashed row's stage is removed first): after pruning the ones whose directories are gone, a store with no
/// worktree left holds nothing a checkout needs. One that cannot be pruned
/// stays.
fn retire_old_stores(context: &GitContext<'_>, key: &storage::StoreKey, current: &Path) {
    // A crashed row's stage keeps its generation in use until it is gone.
    storage::remove_crashed_stages(&context.config.home);
    for generation in key.generations() {
        let store = key.dir(generation);
        if store != current
            && git::prune_worktrees(context, &store).is_ok()
            && storage::has_no_worktrees(&store)
        {
            storage::remove_dir(&store);
        } else if store == current && storage::registered_worktrees(&store) > 1 {
            // Registrations of the removed stages: the one live checkout
            // is the only registration of a store with no stale ones.
            let _ = git::prune_worktrees(context, &store);
        }
    }
}

/// Whether `path` (a requested sparse path) is a withheld file or holds one:
/// its absence from the checkout is by design, not a missing path.
pub(super) fn is_withheld(withheld: &[String], path: &str) -> bool {
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
pub(super) fn checked_out(stage: &Path, path: &str) -> bool {
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

/// The recovery of a checkout holding local work: the message says it was
/// preserved, so the hint says only how to make it clean or cloned again.
pub(super) const DIRTY_CHECKOUT_HINT: &str = "Move your edits out of this checkout and restore it to clean, then retry. If its shared store was deleted, delete the directory to have it cloned again.";

/// The recovery of a sparse clone whose missing paths no tree of the commit
/// could name: it names the request's own field.
pub(super) const UNVERIFIED_PATHS_HINT: &str =
    "Verify the paths with ghStructure, or omit paths for a full clone.";

pub(super) fn dirty_checkout(path: &Path) -> GitError {
    GitError {
        hints: vec![DIRTY_CHECKOUT_HINT.into()],
        ..GitError::new(
            ErrorCode::CheckoutDirty,
            format!(
                "Clone checkout '{}' contains local files, changes, commits or Git refs; it was preserved without replacement.",
                path.display()
            ),
        )
    }
}

fn discard_stage(stage: &Path) {
    storage::remove_dir(stage);
}

/// Requested sparse paths absent at the revision, every one of them: a
/// not-found input, named per path. The `store` holds the commit's trees, so
/// the recovery is a runnable lead ([`recovery::sparse_next`]); only when the
/// tree cannot be read is it a hint.
fn sparse_not_found(plan: &Plan<'_>, reference: &str, missing: &[&str], store: &Path) -> GitError {
    let query = plan.query;
    let next = recovery::sparse_next(plan.context, store, reference, query, missing);
    let hints = if next.is_some() {
        Vec::new()
    } else {
        vec![UNVERIFIED_PATHS_HINT.into()]
    };
    let paths = missing
        .iter()
        .map(|path| format!("\"{path}\""))
        .collect::<Vec<_>>()
        .join(", ");
    GitError {
        hints,
        calls: next.map(Box::new),
        ..GitError::new(
            ErrorCode::NotFound,
            format!(
                "Path does not exist: {paths} in {}/{}@{reference}; nothing was checked out for it.",
                query.owner, query.repo
            ),
        )
    }
}
