//! Exclusive advisory file locks: the one owner of "poll until a deadline".
//! The kernel releases the lock when its holder exits, so a crashed holder
//! never leaves a stale lock behind.

use std::fs::{File, TryLockError};
use std::time::{Duration, Instant};

/// A held exclusive lock; dropping it unlocks.
#[derive(Debug)]
pub struct LockedFile(File);

impl Drop for LockedFile {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// Why a lock was not taken.
#[derive(Debug)]
pub enum LockError {
    /// Another holder kept it for the whole wait.
    Busy,
    /// The filesystem refused the lock operation itself.
    Io(std::io::Error),
}

/// One non-blocking attempt's outcome.
#[derive(Debug)]
pub enum TryLock {
    /// The lock is held until the guard drops.
    Locked(LockedFile),
    /// Another holder has it; the file is handed back for a later attempt.
    Busy(File),
}

/// `path` opened for locking: created when missing (owner-only on Unix),
/// never truncated, so every holder of the sidecar locks the same inode.
pub fn open_lock_file(path: &std::path::Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(false).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options.open(path)
}

/// Try once to take an exclusive lock on `file`. `Err` when the filesystem
/// refuses the lock operation itself, as on one without lock support.
pub fn try_lock(file: File) -> std::io::Result<TryLock> {
    match file.try_lock() {
        Ok(()) => Ok(TryLock::Locked(LockedFile(file))),
        Err(TryLockError::WouldBlock) => Ok(TryLock::Busy(file)),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

/// How often a busy lock held across one short file read-modify-write is
/// retried. A wait on a peer's long operation (a token refresh, a download)
/// polls slower.
pub const LOCK_POLL: Duration = Duration::from_millis(10);

/// Take an exclusive lock on `file`, polling every `poll` for at most `wait`.
/// A zero `wait` tries once.
pub fn lock_within(
    mut file: File,
    wait: Duration,
    poll: Duration,
) -> Result<LockedFile, LockError> {
    let deadline = Instant::now() + wait;
    loop {
        match try_lock(file).map_err(LockError::Io)? {
            TryLock::Locked(lock) => return Ok(lock),
            TryLock::Busy(busy) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(LockError::Busy);
                }
                file = busy;
                std::thread::sleep(poll.min(remaining));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn open(path: &std::path::Path) -> std::fs::File {
        open_lock_file(path).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn a_lock_file_is_owner_only_and_keeps_its_contents() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.lock");
        std::fs::write(&path, b"held").unwrap();
        open_lock_file(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"held");
        let created = dir.path().join("b.lock");
        open_lock_file(&created).unwrap();
        let mode = std::fs::metadata(&created).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn an_uncontended_lock_is_taken_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.lock");
        let started = Instant::now();
        let held = lock_within(
            open(&path),
            Duration::from_secs(5),
            Duration::from_millis(10),
        )
        .expect("an uncontended lock");
        assert!(started.elapsed() < Duration::from_secs(1));
        // The guard holds the lock: another holder is turned away.
        assert!(matches!(
            lock_within(open(&path), Duration::ZERO, Duration::from_millis(10)),
            Err(LockError::Busy)
        ));
        drop(held);
    }

    #[test]
    fn a_held_lock_is_waited_for_until_its_holder_lets_go() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.lock");
        let held = lock_within(open(&path), Duration::ZERO, Duration::from_millis(10)).unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(held);
        });
        let started = Instant::now();
        let taken = lock_within(
            open(&path),
            Duration::from_secs(5),
            Duration::from_millis(10),
        )
        .expect("taken once the holder lets go");
        release.join().unwrap();
        // It waited for the holder rather than failing or skipping the lock.
        assert!(started.elapsed() >= Duration::from_millis(100));
        assert!(matches!(
            lock_within(open(&path), Duration::ZERO, Duration::from_millis(10)),
            Err(LockError::Busy)
        ));
        drop(taken);
    }

    #[test]
    fn a_lock_held_past_the_wait_is_busy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.lock");
        let _held = lock_within(open(&path), Duration::ZERO, Duration::from_millis(10)).unwrap();
        let result = lock_within(
            open(&path),
            Duration::from_millis(60),
            Duration::from_millis(10),
        );
        assert!(matches!(result, Err(LockError::Busy)));
    }

    #[test]
    fn try_lock_hands_the_file_back_when_busy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.lock");
        let TryLock::Locked(held) = try_lock(open(&path)).unwrap() else {
            panic!("an uncontended try_lock takes the lock");
        };
        let TryLock::Busy(file) = try_lock(open(&path)).unwrap() else {
            panic!("a held lock is busy");
        };
        drop(held);
        assert!(matches!(try_lock(file).unwrap(), TryLock::Locked(_)));
    }

    #[test]
    fn dropping_the_guard_releases_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.lock");
        drop(lock_within(open(&path), Duration::ZERO, Duration::from_millis(10)).unwrap());
        let again = lock_within(open(&path), Duration::ZERO, Duration::from_millis(10))
            .expect("released by the drop");
        assert!(matches!(
            lock_within(open(&path), Duration::ZERO, Duration::from_millis(10)),
            Err(LockError::Busy)
        ));
        drop(again);
    }
}
