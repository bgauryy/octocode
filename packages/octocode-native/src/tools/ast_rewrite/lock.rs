//! Filesystem-level mutual-exclusion lock for concurrent astRewrite root directories.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use super::{RewriteError, io_error, sha256};

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
        let home = std::env::temp_dir().join("octocode-ast-rewrite-locks-v1");
        fs::create_dir_all(&home).map_err(io_error)?;
        let guard = home.join(".guard");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if Instant::now() > deadline {
                return Err(RewriteError::new(
                    "ast.rewrite.lock_timeout",
                    format!(
                        "Timed out waiting for an overlapping astRewrite root lock: {}",
                        root.display()
                    ),
                ));
            }
            if acquire_lock_directory(&guard, Path::new(""))? {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let result = (|| -> Result<Self, RewriteError> {
            for entry in fs::read_dir(&home)
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
                if !process_is_alive(owner.pid) {
                    remove_stale_lock(&directory);
                    continue;
                }
                if paths_overlap(root, &owner.root) {
                    return Err(RewriteError::new(
                        "ast.rewrite.lock_timeout",
                        format!(
                            "Timed out waiting for an overlapping astRewrite root lock: {}",
                            root.display()
                        ),
                    ));
                }
            }
            let directory = home.join(format!(
                "root-{}",
                sha256(root.to_string_lossy().as_bytes())
            ));
            if !acquire_lock_directory(&directory, root)? {
                return Err(RewriteError::new(
                    "ast.rewrite.lock_unavailable",
                    "Could not acquire the astRewrite root lock.",
                ));
            }
            let owner = read_lock_owner(&directory).ok_or_else(|| {
                RewriteError::new(
                    "ast.rewrite.lock_unavailable",
                    "Could not read the astRewrite root lock owner.",
                )
            })?;
            Ok(Self {
                directory,
                token: owner.token,
            })
        })();
        let _ = fs::remove_dir_all(&guard);
        result
    }
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
            let bytes = serde_json::to_vec(&owner).map_err(|error| {
                RewriteError::new("ast.rewrite.lock_unavailable", error.to_string())
            })?;
            fs::write(directory.join("owner.json"), bytes).map_err(io_error)?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let stale =
                read_lock_owner(directory).is_some_and(|owner| !process_is_alive(owner.pid));
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

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    // SAFETY: signal 0 does not mutate the target process; it only checks existence/permission.
    let result = unsafe { libc::kill(pid.cast_signed(), 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn process_is_alive(_pid: u32) -> bool {
    true
}

