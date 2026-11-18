//! Exclusive advisory file locks: the one owner of "poll until a deadline".
//! The kernel releases the lock when its holder exits, so a crashed holder
//! never leaves a stale lock behind.
//!
//! [`Turnstile`] is the in-process side: taken before a file lock, it leaves
//! the file contended across processes only.

mod removable;
pub use removable::{lock_path_within, remove_unheld};

use std::fs::{File, TryLockError};
use std::sync::{Condvar, Mutex, PoisonError};
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

/// One holder at a time within a process. A release wakes one waiter at once;
/// `poll` bounds how long a waiter goes without checking whether to give up,
/// and how long the next one sleeps when the woken one gives up instead.
#[derive(Debug, Default)]
pub struct Turnstile {
    taken: Mutex<bool>,
    freed: Condvar,
}

/// A held turn; dropping it wakes one waiter, also while a panic unwinds.
#[derive(Debug)]
pub struct Turn<'a>(&'a Turnstile);

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        *self.0.state() = false;
        self.0.freed.notify_one();
    }
}

impl Turnstile {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            taken: Mutex::new(false),
            freed: Condvar::new(),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, bool> {
        self.taken.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Wait for the turn. `check` runs before every attempt and ends the wait
    /// with its error; it must fail once `deadline` has passed, which the wait
    /// never sleeps beyond.
    pub fn take<E>(
        &self,
        deadline: Instant,
        poll: Duration,
        check: impl Fn() -> Result<(), E>,
    ) -> Result<Turn<'_>, E> {
        let mut taken = self.state();
        loop {
            check()?;
            if !*taken {
                *taken = true;
                return Ok(Turn(self));
            }
            let wait = poll.min(deadline.saturating_duration_since(Instant::now()));
            taken = self
                .freed
                .wait_timeout(taken, wait)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

#[cfg(test)]
mod tests;
