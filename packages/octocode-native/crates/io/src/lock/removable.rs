//! Lock files that do not outlive what they guard. The kernel holds a lock on a
//! file, not on its name, so one removed between a locker's open and its lock
//! excludes nobody who opens the name afterwards. Here a remover holds the lock
//! while it removes, and a locker checks what it locked: holders of one name are
//! always holders of one file.

use super::{LockError, LockedFile, lock_within, open_lock_file};
use std::time::{Duration, Instant};

impl LockedFile {
    /// Whether `path` still names the file this lock is on. A sweep may have
    /// unlinked the file between its open and this lock ([`remove_unheld`]):
    /// a lock on that file excludes nobody who opens the path now. Identity
    /// cannot be read on a platform without inode numbers, where nothing
    /// removes a lock file either, so there the answer is yes.
    #[must_use]
    pub fn still_at(&self, path: &std::path::Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            let same = |held: &std::fs::Metadata, named: &std::fs::Metadata| {
                held.dev() == named.dev() && held.ino() == named.ino()
            };
            matches!(
                (self.0.metadata(), std::fs::symlink_metadata(path)),
                (Ok(held), Ok(named)) if same(&held, &named)
            )
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            true
        }
    }
}

/// Take the lock file at `path`, created when missing, polling every `poll` for
/// at most `wait`: every locker of a file a sweep may remove takes it this way,
/// so a lock held is a lock on what the path names, and a sweep cannot remove
/// that while it is held.
pub fn lock_path_within(
    path: &std::path::Path,
    wait: Duration,
    poll: Duration,
) -> Result<LockedFile, LockError> {
    let deadline = Instant::now() + wait;
    loop {
        let file = open_lock_file(path).map_err(LockError::Io)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        let lock = lock_within(file, remaining, poll)?;
        if lock.still_at(path) {
            return Ok(lock);
        }
    }
}

/// Remove the lock file at `path` if nobody holds it and `idle` still says
/// nothing needs it, asked with the lock held; whether it was removed. Every
/// locker takes the file through [`lock_path_within`]: one that opened it before
/// this removal finds on locking that the path names something else, and goes
/// again. Without inode numbers a locker could not tell, so nothing is removed.
pub fn remove_unheld(path: &std::path::Path, idle: impl FnOnce() -> bool) -> bool {
    #[cfg(unix)]
    {
        if !std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file()) {
            return false;
        }
        let Ok(file) = std::fs::File::open(path) else {
            return false;
        };
        let Ok(super::TryLock::Locked(lock)) = super::try_lock(file) else {
            return false;
        };
        lock.still_at(path) && idle() && std::fs::remove_file(path).is_ok()
    }
    #[cfg(not(unix))]
    {
        let _ = (path, idle);
        false
    }
}
