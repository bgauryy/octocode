//! Filesystem-level mutual-exclusion lock for concurrent astRewrite root directories.
use super::{RewriteError, create_private_dir_all, io_error, sha256};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LockOwner {
    version: u8,
    token: String,
    pid: u32,
    root: PathBuf,
    created_at: String,
}

pub(super) struct RootLock {
    directory: PathBuf,
    token: String,
}

impl RootLock {
    pub(super) fn acquire(root: &Path) -> Result<Self, RewriteError> {
        let home = super::state_base_dir().join(format!(
            "octocode-ast-rewrite-locks-v1-{}",
            super::state_dir_uid_suffix()
        ));
        create_private_dir_all(&home)?;
        let guard = home.join(".guard");
        let deadline = Instant::now() + Duration::from_secs(5);
        // Wait out both the short guard and any overlapping root lock: a
        // live owner usually finishes well within the window.
        loop {
            if acquire_lock_directory(&guard, Path::new(""))? {
                let attempt = try_acquire_root(&home, root);
                let _ = fs::remove_dir_all(&guard);
                if let Some(lock) = attempt? {
                    return Ok(lock);
                }
            }
            if Instant::now() > deadline {
                return Err(RewriteError::new(
                    "lockTimeout",
                    format!(
                        "Timed out waiting for an overlapping astRewrite root lock: {}",
                        root.display()
                    ),
                ));
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}

/// Under the guard: the root lock, or `None` while a live owner holds an
/// overlapping root.
fn try_acquire_root(home: &Path, root: &Path) -> Result<Option<RootLock>, RewriteError> {
    for entry in fs::read_dir(home)
        .map_err(io_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(io_error)?
    {
        if !entry.file_type().map_err(io_error)?.is_dir()
            || !entry.file_name().to_string_lossy().starts_with("root-")
        {
            continue;
        }
        let directory = entry.path();
        let Some(owner) = read_lock_owner(&directory) else {
            remove_stale_lock(&directory);
            continue;
        };
        if !crate::process_status::is_alive(owner.pid) {
            remove_stale_lock(&directory);
            continue;
        }
        if paths_overlap(root, &owner.root) {
            return Ok(None);
        }
    }
    let directory = home.join(format!(
        "root-{}",
        sha256(root.to_string_lossy().as_bytes())
    ));
    if !acquire_lock_directory(&directory, root)? {
        return Err(RewriteError::new(
            "lockUnavailable",
            "Could not acquire the astRewrite root lock.",
        ));
    }
    let owner = read_lock_owner(&directory).ok_or_else(|| {
        RewriteError::new(
            "lockUnavailable",
            "Could not read the astRewrite root lock owner.",
        )
    })?;
    Ok(Some(RootLock {
        directory,
        token: owner.token,
    }))
}

impl Drop for RootLock {
    fn drop(&mut self) {
        if read_lock_owner(&self.directory).is_some_and(|owner| owner.token == self.token) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
}

fn acquire_lock_directory(directory: &Path, root: &Path) -> Result<bool, RewriteError> {
    match fs::create_dir(directory) {
        Ok(()) => {
            let token = super::transaction_id(root, &[]);
            let owner = LockOwner {
                version: 1,
                token,
                pid: std::process::id(),
                root: root.to_path_buf(),
                created_at: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |duration| duration.as_nanos())
                    .to_string(),
            };
            let bytes = serde_json::to_vec(&owner)
                .map_err(|error| RewriteError::new("lockUnavailable", error.to_string()))?;
            fs::write(directory.join("owner.json"), bytes).map_err(io_error)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let stale = read_lock_owner(directory)
                .is_some_and(|owner| !crate::process_status::is_alive(owner.pid));
            let ownerless_old = read_lock_owner(directory).is_none()
                && fs::metadata(directory)
                    .and_then(|metadata| metadata.modified())
                    .and_then(|modified| modified.elapsed().map_err(std::io::Error::other))
                    .is_ok_and(|age| age >= Duration::from_secs(1));
            if stale || ownerless_old {
                remove_stale_lock(directory);
            }
            Ok(false)
        }
        Err(error) => Err(io_error(error)),
    }
}

fn read_lock_owner(directory: &Path) -> Option<LockOwner> {
    serde_json::from_slice(&fs::read(directory.join("owner.json")).ok()?).ok()
}

fn remove_stale_lock(directory: &Path) {
    let tombstone = directory.with_extension(format!(
        "stale-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos())
    ));
    if fs::rename(directory, &tombstone).is_ok() {
        let _ = fs::remove_dir_all(tombstone);
    }
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}
