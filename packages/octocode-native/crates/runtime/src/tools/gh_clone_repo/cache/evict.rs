//! Cache eviction: stale stages, expired or invalid checkouts, and the size and count limits.
use super::{CloneLock, clones_root, directory_size, remove_dir, valid_clone};
use crate::cache::evictions::log_eviction;
use crate::civil_date::rfc3339_millis;
use crate::tools::gh_clone_repo::CloneContext;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const STALE_ARTIFACT_AGE: Duration = Duration::from_secs(15 * 60);

pub(in crate::tools::gh_clone_repo) fn cleanup_stale_artifacts(home: &Path) {
    let Ok(entries) = fs::read_dir(home.join("tmp").join("clone-tmp")) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|value| value.modified())
            .ok()
            .and_then(|value| SystemTime::now().duration_since(value).ok())
            .is_some_and(|age| age > STALE_ARTIFACT_AGE);
        if old {
            let path = entry.path();
            log_eviction(home, "stale-artifact", &path, directory_size(&path));
            remove_dir(&path);
        }
    }
}

pub(in crate::tools::gh_clone_repo) fn evict(context: &CloneContext<'_>, protected: Option<&Path>) {
    let home = &context.config.cache_home;
    let ttl = context.config.cache_ttl;
    let max_bytes = context.config.max_cache_size_bytes;
    let max_count = context.config.max_clone_count;
    let base = clones_root(home);
    let mut live = vec![];
    for branch in clone_entries(home, &base) {
        if context.path_policy.validate(&branch).is_err() {
            continue;
        }
        let meta = if protected == Some(branch.as_path()) {
            valid_clone(&branch, ttl)
        } else {
            let Some(_guard) = CloneLock::try_acquire(home, &branch) else {
                continue;
            };
            let meta = valid_clone(&branch, ttl);
            if meta.is_none() {
                // Expired/invalid bookkeeping does not authorize deleting the
                // caller's files. Unverifiable trees also remain untouched.
                if !crate::tools::gh_clone_repo::git::checkout_status(context, &branch)
                    .is_ok_and(|status| status.safe_to_replace)
                {
                    continue;
                }
                log_eviction(home, "expired-or-invalid", &branch, directory_size(&branch));
                remove_dir(&branch);
            }
            meta
        };
        let Some(meta) = meta else { continue };
        live.push((
            branch.clone(),
            rfc3339_millis(&meta.cloned_at).unwrap_or(0),
            meta.size_bytes,
        ));
    }
    live.sort_by_key(|entry| entry.1);
    let mut bytes: u64 = live.iter().map(|entry| entry.2).sum();
    let mut count = live.len();
    for (path, _, size) in live {
        if bytes <= max_bytes && count <= max_count {
            break;
        }
        if protected == Some(path.as_path()) {
            continue;
        }
        let Some(_guard) = CloneLock::try_acquire(home, &path) else {
            continue;
        };
        if !crate::tools::gh_clone_repo::git::checkout_status(context, &path)
            .is_ok_and(|status| status.safe_to_replace)
        {
            continue;
        }
        if fs::remove_dir_all(&path).is_ok() {
            log_eviction(home, "size-limit", &path, size);
            bytes = bytes.saturating_sub(size);
            count = count.saturating_sub(1);
        }
    }
}

/// Every checkout entry (`<owner>/<repo>/<entry>/`) under `base`. A file
/// beside the owner, repo or entry directories is no entry and nothing reads
/// it (a retired layout's `.default-branch__host_*.json`): it is removed as
/// the walk passes it.
fn clone_entries(home: &Path, base: &Path) -> Vec<PathBuf> {
    let mut values = vec![];
    for owner in directories(home, base) {
        for repo in directories(home, &owner) {
            values.extend(directories(home, &repo));
        }
    }
    values
}

/// The directories directly under `parent`; every other entry there (a
/// file or a symlink, never followed) is an orphan and is removed.
fn directories(home: &Path, parent: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut directories = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => directories.push(path),
            Ok(_) => {
                let size = fs::symlink_metadata(&path).map_or(0, |meta| meta.len());
                if fs::remove_file(&path).is_ok() {
                    log_eviction(home, "orphaned-file", &path, size);
                }
            }
            Err(_) => {}
        }
    }
    directories
}
