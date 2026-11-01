//! Checkout destinations, process locks and atomic publication. No checkout cache.
use super::{CloneContext, CloneError, check_control, hash};
use octocode_contract_types::ErrorCode;
use octocode_io::lock::{LockedFile, TryLock};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static STAGE_ID: AtomicU64 = AtomicU64::new(0);

/// Root of every clone checkout under the Octocode home; local tools may read it.
pub(crate) fn clones_root(home: &Path) -> PathBuf {
    home.join("clones")
}

// GitHub owner/repo names are case-insensitive and share one destination.
fn repo_dir(home: &Path, owner: &str, repo: &str) -> PathBuf {
    clones_root(home)
        .join(owner.to_ascii_lowercase())
        .join(repo.to_ascii_lowercase())
}

/// The destination of `commit` (a full SHA, keyed lowercase: one commit
/// in any case is one checkout) with its sparse set and history depth.
pub(super) fn clone_dir(
    home: &Path,
    owner: &str,
    repo: &str,
    commit: &str,
    sparse_key: Option<&str>,
    depth: u64,
    host: &str,
) -> PathBuf {
    let sparse = sparse_key
        .map(|key| format!("__sp_{}", hash(key, 6)))
        .unwrap_or_default();
    // A shallow checkout's entry names no depth.
    let history = if depth > 1 {
        format!("__d{depth}")
    } else {
        String::new()
    };
    repo_dir(home, owner, repo).join(format!(
        "{}{sparse}{history}__host_{}",
        commit.to_ascii_lowercase(),
        hash(host, 16)
    ))
}

/// The lock file of the checkout `clone_dir`. It is never removed: a writer
/// waiting on it must not lock a file another writer recreated.
pub(super) fn lock_path(home: &Path, clone_dir: &Path) -> PathBuf {
    home.join("tmp")
        .join("clone-locks")
        .join(format!("{}.lock", hash(&clone_dir.to_string_lossy(), 16)))
}

/// A held checkout lock: the OS lock ghStructure's materialized snapshots
/// use ([`octocode_io::lock`]), so it spans git subprocesses and other
/// processes, and the kernel releases a crashed holder's lock.
pub(super) struct CloneLock {
    _held: LockedFile,
}

impl CloneLock {
    pub fn acquire(clone_dir: &Path, context: &CloneContext<'_>) -> Result<Self, CloneError> {
        let path = lock_path(&context.config.home, clone_dir);
        let started = std::time::Instant::now();
        let mut file = open_lock(&path).map_err(storage_io)?;
        loop {
            check_control(context)?;
            match octocode_io::lock::try_lock(file).map_err(storage_io)? {
                TryLock::Locked(lock) => return Ok(Self { _held: lock }),
                TryLock::Busy(busy) => file = busy,
            }
            if started.elapsed() >= context.config.lock_wait {
                return Err(CloneError::new(
                    ErrorCode::Timeout,
                    format!(
                        "Timed out waiting for clone destination lock '{}'.",
                        path.display()
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn open_lock(path: &Path) -> io::Result<fs::File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
}

pub(super) fn stage_dir(home: &Path, clone_dir: &Path) -> Result<PathBuf, CloneError> {
    let base = home.join("tmp").join("clone-tmp");
    fs::create_dir_all(&base).map_err(storage_io)?;
    let id = STAGE_ID.fetch_add(1, Ordering::Relaxed);
    let name = clone_dir.file_name().unwrap_or_default().to_string_lossy();
    let path = base.join(format!(
        "{}-{name}-{}-{id}",
        hash(&clone_dir.to_string_lossy(), 16),
        std::process::id()
    ));
    remove_dir(&path);
    Ok(path)
}

pub(super) fn promote(stage: &Path, destination: &Path) -> Result<(), CloneError> {
    let parent = destination.parent().ok_or_else(|| {
        CloneError::new(
            ErrorCode::FileAccessFailed,
            "Clone destination has no parent",
        )
    })?;
    fs::create_dir_all(parent).map_err(storage_io)?;
    let previous = stage.with_extension("previous");
    remove_dir(&previous);
    let had_previous = destination.exists();
    if had_previous {
        fs::rename(destination, &previous).map_err(storage_io)?;
    }
    if let Err(error) = fs::rename(stage, destination) {
        if had_previous && let Err(restore) = fs::rename(&previous, destination) {
            return Err(CloneError::new(
                ErrorCode::FileAccessFailed,
                format!(
                    "Clone publication failed ({error}); previous checkout remains at '{}' because rollback failed ({restore}).",
                    previous.display()
                ),
            ));
        }
        return Err(storage_io(error));
    }
    remove_dir(&previous);
    Ok(())
}

pub(super) fn remove_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

pub(super) fn checked_out_size(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_name() != ".git")
        .map(|entry| entry.path())
        .map(|path| directory_size(&path))
        .sum()
}

fn directory_size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.file_type().is_symlink() {
        return 0;
    }
    if meta.is_file() {
        return meta.len();
    }
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| directory_size(&entry.path()))
        .sum()
}

fn storage_io(error: io::Error) -> CloneError {
    CloneError::new(
        ErrorCode::FileAccessFailed,
        format!("Clone storage operation failed: {error}"),
    )
}

#[cfg(test)]
mod tests;
