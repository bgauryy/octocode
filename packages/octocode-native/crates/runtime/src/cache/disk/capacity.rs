use super::*;
use octocode_io::lock::{LOCK_POLL, LockError, LockedFile, lock_within};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

const CAPACITY: &str = ".capacity";
const LOCK: &str = ".capacity.lock";
const SCAN_INTERVAL: u64 = 60;
/// How long a write only later reuse reads waits for the store's lock, a holder
/// being one write or one scan as long as the store is large. Past it the call
/// goes on with its whole answer and says so ([`STORE_BUSY`]); a later read pays
/// a fresh read. A write its call's answer depends on waits for the request's own
/// deadline instead ([`crate::cache::Store::put_until`]).
pub(in crate::cache) const REUSE_LOCK_WAIT: Duration = Duration::from_secs(1);
/// How often a wait for the store's lock looks for its request's cancel.
#[cfg_attr(
    octocode_clippy_ratchet,
    expect(
        clippy::disallowed_methods,
        reason = "Poll interval for a cancel during a lock wait, not a deadline or evidence limit"
    )
)]
const CANCEL_SLICE: Duration = Duration::from_millis(100);

/// Why the store's lock was not taken.
enum NotLocked {
    /// Busy to the deadline (`None`), or the lock file or lock operation
    /// failed with this kind.
    Unavailable(Option<io::ErrorKind>),
    /// The request this thread works for was cancelled: nobody reads its
    /// answer, so nothing is said.
    Cancelled,
}
/// A write refused for the store's lock is said once per call, whatever wrote it.
const STORE_BUSY: &str = "cache-store-busy";
#[cfg(test)]
thread_local! {
    /// Directory scans [`Disk::trim`] ran on this thread.
    pub(in crate::cache) static SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A conservative upper bound. Reserve before publishing; deletions and failed
/// writes can overcount until the next scan, but cannot admit too many bytes.
/// Writes are not fsynced; this bound does not guarantee power-loss recovery.
#[derive(Deserialize, Serialize)]
struct Capacity {
    used: u64,
    scanned_at: u64,
}

/// The store's lock: its file lock, which other processes contend for, and
/// this process's turn at the store, held as long (dropped after the file).
pub(in crate::cache) struct StoreLock {
    _file: LockedFile,
    _turn: TurnHeld,
}

mod turn;
use turn::{Turn, TurnHeld};

impl Disk {
    /// The store's lock. This process's turn is waited for as long as the
    /// write before it takes; the file lock up to `deadline`. Either wait ends
    /// when the request this thread works for is cancelled
    /// ([`octocode_engine::deep_stack::current_stop`]): a cancelled request
    /// must not stay joined to a wait only the holder ends.
    fn lock(&self, deadline: Instant) -> Result<StoreLock, NotLocked> {
        let unavailable = |error: io::Error| NotLocked::Unavailable(Some(error.kind()));
        octocode_io::atomic::create_private_dir_all(&self.dir).map_err(unavailable)?;
        let file =
            crate::private_file::open_no_follow(&self.dir.join(LOCK), true).map_err(unavailable)?;
        let stop = octocode_engine::deep_stack::current_stop();
        let stopped = || {
            stop.as_ref()
                .is_some_and(|stop| stop.load(std::sync::atomic::Ordering::Relaxed))
        };
        let turn = Turn::of(&self.dir)
            .take(stopped)
            .ok_or(NotLocked::Cancelled)?;
        loop {
            if stopped() {
                return Err(NotLocked::Cancelled);
            }
            let wait = deadline
                .saturating_duration_since(Instant::now())
                .min(CANCEL_SLICE);
            match lock_within(file.try_clone().map_err(unavailable)?, wait, LOCK_POLL) {
                Ok(lock) => {
                    return Ok(StoreLock {
                        _file: lock,
                        _turn: turn,
                    });
                }
                Err(LockError::Busy) if Instant::now() < deadline => {}
                Err(LockError::Busy) => return Err(NotLocked::Unavailable(None)),
                Err(LockError::Io(error)) => return Err(unavailable(error)),
            }
        }
    }

    /// The store's lock for a write, or the note in the call's `warnings` that
    /// the write was not kept (none for a cancelled request, whose answer
    /// nobody reads). The store is named by its folder: where the home is says
    /// nothing the agent acts on.
    fn lock_to_write(&self, deadline: Instant) -> Option<StoreLock> {
        let lock = self.lock(deadline);
        if let Err(NotLocked::Unavailable(cause)) = &lock {
            self.say_not_locked(*cause);
        }
        lock.ok()
    }

    /// The call's note that a write was not kept because the store's lock
    /// could not be taken: the cause is access when the filesystem refused
    /// the lock file, busy otherwise.
    fn say_not_locked(&self, cause: Option<io::ErrorKind>) {
        let store = self.dir.file_name().unwrap_or(self.dir.as_os_str());
        let cause = match cause {
            Some(kind) if refused_for_access(kind) => "is not writable",
            _ => "was busy or could not be locked",
        };
        octocode_io::notice::raise_in_result(
            STORE_BUSY,
            format!(
                "The cache store {} {cause}, so something this call found was not saved; a later call reads it again.",
                store.to_string_lossy()
            ),
        );
    }

    fn capacity(&self) -> Option<Capacity> {
        let file = crate::private_file::open_no_follow(&self.dir.join(CAPACITY), false).ok()?;
        let bytes = crate::private_file::read_limited(file, 128).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn save_capacity(&self, capacity: &Capacity) -> io::Result<()> {
        let bytes = serde_json::to_vec(capacity).map_err(io::Error::other)?;
        octocode_io::atomic::write_atomic(&self.dir.join(CAPACITY), &bytes, false)
    }

    pub(super) fn publish(
        &self,
        path: &Path,
        parts: [&[u8]; 2],
        config: &CacheConfig,
        deadline: Instant,
    ) -> Result<(), NotStored> {
        let Some(size) = parts[0].len().checked_add(parts[1].len()) else {
            return Err(NotStored::Refused);
        };
        if size as u64 > config.disk_bytes {
            return Err(NotStored::Refused);
        }
        let lock = match self.lock(deadline) {
            Ok(lock) => lock,
            Err(NotLocked::Cancelled) => return Err(NotStored::Cancelled),
            Err(NotLocked::Unavailable(cause)) => {
                self.say_not_locked(cause);
                return Err(NotStored::Unlocked(cause));
            }
        };
        let stored = self.publish_locked(path, parts, size as u64, config.disk_bytes);
        drop(lock);
        stored
    }

    fn publish_locked(
        &self,
        path: &Path,
        parts: [&[u8]; 2],
        size: u64,
        budget: u64,
    ) -> Result<(), NotStored> {
        self.reserve(path, size, budget)?;
        octocode_io::atomic::write_atomic_with(path, 0o600, false, |file| {
            for part in parts {
                file.write_all(part)?;
            }
            Ok(())
        })
        .map_err(|error| NotStored::Failed(error.kind()))
    }

    /// Link `from` as `to`, counted at its whole size: the budget does not
    /// know who else holds it. A link already there holds the same content.
    pub(super) fn publish_link(&self, from: &Path, to: &Path, config: &CacheConfig) -> bool {
        let Ok(source) = fs::symlink_metadata(from) else {
            return false;
        };
        if !source.is_file() || source.len() > config.disk_bytes {
            return false;
        }
        // A link already there is already counted: a repeat only restarts its
        // lifetime, with no capacity to rewrite.
        if let Ok(file) = crate::private_file::open_no_follow(to, false) {
            touch_file(&file, to);
            return true;
        }
        let Some(_lock) = self.lock_to_write(Instant::now() + REUSE_LOCK_WAIT) else {
            return false;
        };
        if self.reserve(to, source.len(), config.disk_bytes).is_err() {
            return false;
        }
        match fs::hard_link(from, to) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(_) => return false,
        }
        crate::private_file::open_no_follow(to, false)
            .map(|file| touch_file(&file, to))
            .is_ok()
    }

    /// The entry the lock file at `path` is named for ([`Disk::entry_lock`]).
    fn entry_of_lock(&self, path: &Path) -> Option<PathBuf> {
        let stem = path
            .file_name()?
            .to_str()?
            .strip_prefix(ENTRY_LOCK_PREFIX)?
            .strip_suffix(".lock")?;
        Some(self.dir.join(format!("{stem}.{ENTRY_EXTENSION}")))
    }

    /// Make room for an entry of `size` at `path`.
    fn reserve(&self, path: &Path, size: u64, budget: u64) -> Result<(), NotStored> {
        let failed = |error: io::Error| NotStored::Failed(error.kind());
        let now = now_unix();
        let mut capacity = match self.capacity() {
            Some(capacity) if now.saturating_sub(capacity.scanned_at) < SCAN_INTERVAL => capacity,
            _ => self.trim(budget, Some(path), now).map_err(failed)?,
        };
        let old_size = entry_size(path).map_err(failed)?;
        // Reserve max(old,new) through the rename: a process crash must not leave the
        // old, larger entry paired with a smaller reservation.
        let growth = size.saturating_sub(old_size);
        if capacity
            .used
            .checked_add(growth)
            .is_none_or(|used| used > budget)
        {
            // Reconcile reader deletions and make headroom in one scan. Batch
            // eviction keeps a full cache from scanning on every insertion.
            let target = budget.saturating_sub(growth).saturating_sub(budget / 8);
            capacity = self.trim(target, Some(path), now).map_err(failed)?;
        }
        let growth = size.saturating_sub(entry_size(path).map_err(failed)?);
        capacity.used = capacity
            .used
            .checked_add(growth)
            .ok_or(NotStored::Refused)?;
        if capacity.used > budget {
            return Err(NotStored::Refused);
        }
        self.save_capacity(&capacity).map_err(failed)
    }

    /// Reconcile the whole directory once, removing expired entries before
    /// oldest-first capacity eviction. A completed pass advances the scan
    /// time, so a stale backlog never forces each writer to rescan a live prefix.
    fn trim(&self, budget: u64, protected: Option<&Path>, at: u64) -> io::Result<Capacity> {
        #[cfg(test)]
        SCANS.with(|scans| scans.set(scans.get() + 1));
        let now = SystemTime::now();
        let unix_now = now_unix();
        let mut files = Vec::new();
        let mut total = 0_u64;
        let stop = octocode_engine::deep_stack::current_stop();
        let check_stop = || {
            if stop
                .as_ref()
                .is_some_and(|stop| stop.load(std::sync::atomic::Ordering::Relaxed))
            {
                Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "Cache cleanup cancelled",
                ))
            } else {
                Ok(())
            }
        };
        for entry in fs::read_dir(&self.dir)? {
            check_stop()?;
            let path = entry?.path();
            if let Some(entry) = self.entry_of_lock(&path) {
                remove_unneeded_lock(&path, &entry);
                continue;
            }
            let extension = path.extension().and_then(|ext| ext.to_str());
            // A store's other files (a plan's lock stays for good) ask nothing.
            if !matches!(
                extension,
                Some(ENTRY_EXTENSION | LINK_EXTENSION | RETIRED_EXTENSION | "tmp")
            ) {
                continue;
            }
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let link = extension == Some(LINK_EXTENSION);
            if extension == Some(ENTRY_EXTENSION) || link {
                let aged = || {
                    if link {
                        // A date ahead of the clock by more than the link's own
                        // lifetime cannot be true: it is as old as that.
                        now.duration_since(meta.modified().unwrap_or(now))
                            .map_or_else(
                                |ahead| ahead.duration() > crate::cache::CONTINUATION_IDLE,
                                |age| age >= crate::cache::CONTINUATION_IDLE,
                            )
                    } else {
                        expired(&path, unix_now)
                    }
                };
                if !meta.is_file() || (Some(path.as_path()) != protected && aged()) {
                    match fs::remove_file(&path) {
                        Ok(()) => {
                            continue;
                        }
                        Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                        Err(error) if !meta.is_file() => return Err(error),
                        Err(_) => {}
                    }
                }
                total = total
                    .checked_add(meta.len())
                    .ok_or_else(|| io::Error::other("cache byte count overflow"))?;
                // Expiration-only maintenance does not need a collection
                // proportional to the number of live entries.
                if budget != u64::MAX {
                    files.push((meta.modified()?, meta.len(), path));
                }
            } else if extension == Some(RETIRED_EXTENSION)
                || (extension == Some("tmp")
                    && now
                        .duration_since(meta.modified()?)
                        .is_ok_and(|age| age > ORPHAN_TEMP_AGE))
            {
                let _ = fs::remove_file(path);
            }
        }
        if total > budget {
            check_stop()?;
            files.sort_unstable();
            for (_, len, path) in files {
                check_stop()?;
                if total <= budget {
                    break;
                }
                if Some(path.as_path()) == protected {
                    continue;
                }
                match fs::remove_file(&path) {
                    Ok(()) => total -= len,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => total -= len,
                    Err(_) => {}
                }
            }
        }
        Ok(Capacity {
            used: total,
            scanned_at: at,
        })
    }

    /// A failed reader may hold an older file than the path now names.
    /// Recheck under the publication lock before unlinking a current entry.
    pub(super) fn discard_invalid<V, C: DiskCodec<V>>(&self, key: &CacheKey, config: &CacheConfig) {
        let Ok(_lock) = self.lock(Instant::now() + REUSE_LOCK_WAIT) else {
            return;
        };
        let path = self.file(key);
        if super::read::entry::<V, C>(&path, config).is_err_and(|error| entry_is_invalid(&error)) {
            let _ = fs::remove_file(path);
        }
    }

    /// Remove every expired entry, and every lock its entry outlived, in one
    /// scan under the store's lock, making one pass over the directory. Nothing live is
    /// evicted (no byte budget applies), and the capacity file is rewritten
    /// from the scan. The daily maintenance sweep runs this, so expired saved
    /// work (a section plan holds source text) does not wait on disk for the
    /// next write to its store. A busy store is left to its writers, whose
    /// scans remove the same entries.
    pub(crate) fn prune_expired(&self) {
        let Ok(_lock) = self.lock(Instant::now() + REUSE_LOCK_WAIT) else {
            return;
        };
        if let Ok(capacity) = self.trim(u64::MAX, None, now_unix()) {
            let _ = self.save_capacity(&capacity);
        }
    }

    /// The store's lock as one of this process's writes holds it.
    #[cfg(test)]
    pub(in crate::cache) fn hold(&self) -> StoreLock {
        self.lock(Instant::now() + REUSE_LOCK_WAIT)
            .unwrap_or_else(|_| panic!("the store is free"))
    }

    #[cfg(test)]
    pub(crate) fn prune(&self, budget: u64) {
        let Ok(_lock) = self.lock(Instant::now() + REUSE_LOCK_WAIT) else {
            return;
        };
        if let Ok(capacity) = self.trim(budget, None, now_unix()) {
            let _ = self.save_capacity(&capacity);
        }
    }
}

/// Remove the lock file at `lock` if `entry` is gone and nobody holds it: a plan's
/// lock must not outlive the plan, or a home grows one file per plan for good.
/// While `entry` stands, its lock file stands too.
fn remove_unneeded_lock(lock: &Path, entry: &Path) -> bool {
    let gone =
        || fs::symlink_metadata(entry).is_err_and(|error| error.kind() == io::ErrorKind::NotFound);
    gone() && octocode_io::lock::remove_unheld(lock, gone)
}

fn entry_size(path: &Path) -> io::Result<u64> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => Ok(meta.len()),
        Ok(_) => Err(io::Error::other("cache entry is not a regular file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}
