//! Exclusive advisory file locks: the one owner of "poll until a deadline".
//! The kernel releases the lock when its holder exits, so a crashed holder
//! never leaves a stale lock behind.
//!
//! [`Turnstile`] is the in-process side: taken before a file lock, it leaves
//! the file contended across processes only.

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
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    const UNBOUNDED: Duration = Duration::from_secs(600);

    fn take(gate: &Turnstile, poll: Duration, wait: Duration) -> Result<Turn<'_>, ()> {
        let deadline = Instant::now() + wait;
        gate.take(deadline, poll, || {
            if Instant::now() >= deadline {
                Err(())
            } else {
                Ok(())
            }
        })
    }

    /// With a poll far longer than the test, only the release can wake it.
    #[test]
    fn a_release_wakes_the_waiter_without_waiting_for_the_poll() {
        let gate = Turnstile::new();
        let held = take(&gate, UNBOUNDED, UNBOUNDED).expect("free");
        std::thread::scope(|scope| {
            let waiter = scope.spawn(|| take(&gate, UNBOUNDED, UNBOUNDED).is_ok());
            std::thread::sleep(Duration::from_millis(50));
            drop(held);
            assert!(waiter.join().expect("woken"));
        });
    }

    #[test]
    fn a_turn_held_past_the_deadline_cuts_the_wait() {
        let gate = Turnstile::new();
        let _held = take(&gate, UNBOUNDED, UNBOUNDED).expect("free");
        let started = Instant::now();
        assert!(take(&gate, UNBOUNDED, Duration::from_millis(30)).is_err());
        assert!(started.elapsed() < Duration::from_secs(60));
    }

    /// A turn's check is seen within the poll, for what no release signals.
    #[test]
    fn a_check_ends_the_wait_within_the_poll() {
        let gate = Turnstile::new();
        let _held = take(&gate, UNBOUNDED, UNBOUNDED).expect("free");
        let cancelled = std::sync::atomic::AtomicBool::new(false);
        std::thread::scope(|scope| {
            let waiter = scope.spawn(|| {
                gate.take(Instant::now() + UNBOUNDED, Duration::from_millis(5), || {
                    if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                        Err("cancelled")
                    } else {
                        Ok(())
                    }
                })
                .map(|_| ())
            });
            std::thread::sleep(Duration::from_millis(30));
            cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
            assert_eq!(waiter.join().expect("ended"), Err("cancelled"));
        });
    }

    /// A panicking holder frees its turn.
    #[test]
    fn a_holder_that_panics_frees_the_turn() {
        let gate = Turnstile::new();
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = take(&gate, UNBOUNDED, UNBOUNDED).expect("free");
            panic!("holder");
        }));
        assert!(unwound.is_err());
        assert!(take(&gate, UNBOUNDED, Duration::from_millis(100)).is_ok());
    }

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
