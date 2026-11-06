//! The `<state>.lock` sidecar that serializes the state file's
//! read-merge-write across processes, and the blocking-pool hop for its IO.
use super::warn_unshared;
use crate::ProviderError;
use octocode_io::cancellation::Cancellation;
use octocode_io::{budget::pause, lock::TryLock};
use std::{
    path::{Path, PathBuf},
    time::Instant,
};

/// The `<state>.lock` sidecar, opened for locking; `None` when it cannot be.
fn open_lock_file(path: &Path) -> Option<std::fs::File> {
    let mut name = path.as_os_str().to_os_string();
    name.push(".lock");
    let name = PathBuf::from(name);
    match octocode_io::lock::open_lock_file(&name) {
        Ok(file) => Some(file),
        Err(error) => {
            warn_unshared("open the lock file", &name, &error);
            None
        }
    }
}

/// Run `work` on the blocking pool and wait for it. A panic in it resumes
/// here, as if it had run inline; a runtime shutting down before it started
/// cancels the request.
pub(super) async fn off_worker<R: Send + 'static>(
    work: impl FnOnce() -> R + Send + 'static,
) -> Result<R, ProviderError> {
    match tokio::task::spawn_blocking(octocode_io::notice::carried(work)).await {
        Ok(value) => Ok(value),
        Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
        Err(_) => Err(octocode_io::budget::BudgetStop::Cancelled.into()),
    }
}

/// Exclusive advisory lock on the `<state>.lock` sidecar, polled so that the
/// wait yields to the runtime and ends at the `deadline` or on cancellation.
/// `Ok(None)` when the file cannot be opened or the filesystem does not
/// support locking.
pub(super) async fn lock_file(
    path: &Path,
    deadline: Instant,
    cancellation: &Cancellation,
) -> Result<Option<octocode_io::lock::LockedFile>, ProviderError> {
    let Some(mut file) = open_lock_file(path) else {
        return Ok(None);
    };
    loop {
        match octocode_io::lock::try_lock(file) {
            Ok(TryLock::Locked(lock)) => return Ok(Some(lock)),
            Ok(TryLock::Busy(busy)) => {
                file = busy;
                pause(octocode_io::lock::LOCK_POLL, deadline, cancellation).await?;
            }
            Err(error) => {
                warn_unshared("lock", path, &error);
                return Ok(None);
            }
        }
    }
}

/// What a synchronous writer's lock attempt yields.
pub(super) enum WriteLock {
    /// The peer lock is held: the read-merge-write is serialized.
    Held(octocode_io::lock::LockedFile),
    /// The filesystem refuses the lock operation itself (no lock support).
    Unsupported,
    /// A peer holds the lock past the deadline, or the lock file cannot be
    /// opened: an unlocked write could drop a peer's facts.
    Unavailable,
}

/// [`lock_file`] for synchronous writers, bounded by `deadline` so a stalled
/// peer cannot hold the caller indefinitely.
pub(super) fn lock_file_blocking(path: &Path, deadline: Instant) -> WriteLock {
    let Some(file) = open_lock_file(path) else {
        return WriteLock::Unavailable;
    };
    match octocode_io::lock::lock_within(
        file,
        deadline.saturating_duration_since(Instant::now()),
        octocode_io::lock::LOCK_POLL,
    ) {
        Ok(lock) => WriteLock::Held(lock),
        Err(octocode_io::lock::LockError::Busy) => WriteLock::Unavailable,
        Err(octocode_io::lock::LockError::Io(error)) => {
            warn_unshared("lock", path, &error);
            WriteLock::Unsupported
        }
    }
}
