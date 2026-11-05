use super::*;
use octocode_io::lock::{LOCK_POLL, LockedFile, lock_within};
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
/// A write refused for the store's lock is said once per call, whatever wrote it.
const STORE_BUSY: &str = "cache-store-busy";
/// Expired entries one scan removes, inside a caller's write. A longer backlog
/// leaves the next write a scan of its own, so it cannot fall behind the writes;
/// a read judges an entry's age itself, so what waits here is never served.
pub(in crate::cache) const EXPIRED_PER_SCAN: usize = 32;

/// A conservative upper bound. Reserve before publishing; deletions and failed
/// writes can overcount until the next scan, but cannot admit too many bytes.
/// Writes are not fsynced; this bound does not guarantee power-loss recovery.
#[derive(Deserialize, Serialize)]
struct Capacity {
    used: u64,
    scanned_at: u64,
}

impl Disk {
    fn lock(&self, deadline: Instant) -> Option<LockedFile> {
        octocode_io::atomic::create_private_dir_all(&self.dir).ok()?;
        let file = crate::private_file::open_no_follow(&self.dir.join(LOCK), true).ok()?;
        let wait = deadline.saturating_duration_since(Instant::now());
        lock_within(file, wait, LOCK_POLL).ok()
    }

    /// The store's lock for a write, or the notice that the write was not kept.
    fn lock_to_write(&self, deadline: Instant) -> Option<LockedFile> {
        let lock = self.lock(deadline);
        if lock.is_none() {
            octocode_io::notice::raise(
                STORE_BUSY,
                format!(
                    "The cache store {} was busy or could not be locked, so something this call found was not saved; a later call reads it again.",
                    self.dir.display()
                ),
            );
        }
        lock
    }

    fn capacity(&self) -> Option<Capacity> {
        let file = crate::private_file::open_no_follow(&self.dir.join(CAPACITY), false).ok()?;
        let bytes = crate::private_file::read_limited(file, 128).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn save_capacity(&self, capacity: &Capacity) -> Option<()> {
        let bytes = serde_json::to_vec(capacity).ok()?;
        octocode_io::atomic::write_atomic(&self.dir.join(CAPACITY), &bytes, false).ok()
    }

    pub(super) fn publish(
        &self,
        path: &Path,
        parts: [&[u8]; 2],
        config: &CacheConfig,
        deadline: Instant,
    ) -> bool {
        let Some(size) = parts[0].len().checked_add(parts[1].len()) else {
            return false;
        };
        if size as u64 > config.disk_bytes {
            return false;
        }
        let Some(_lock) = self.lock_to_write(deadline) else {
            return false;
        };
        self.publish_locked(path, parts, size as u64, config.disk_bytes)
            .is_some()
    }

    fn publish_locked(&self, path: &Path, parts: [&[u8]; 2], size: u64, budget: u64) -> Option<()> {
        self.reserve(path, size, budget)?;
        octocode_io::atomic::write_atomic_with(path, 0o600, false, |file| {
            for part in parts {
                file.write_all(part)?;
            }
            Ok(())
        })
        .ok()
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
        if self.reserve(to, source.len(), config.disk_bytes).is_none() {
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
    fn reserve(&self, path: &Path, size: u64, budget: u64) -> Option<()> {
        let now = now_unix();
        let mut capacity = match self.capacity() {
            Some(capacity) if now.saturating_sub(capacity.scanned_at) < SCAN_INTERVAL => capacity,
            known => self
                .trim(budget, Some(path), now, known.map(|known| known.used))
                .ok()?,
        };
        let old_size = entry_size(path).ok()?;
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
            capacity = self.trim(target, Some(path), now, None).ok()?;
        }
        let growth = size.saturating_sub(entry_size(path).ok()?);
        capacity.used = capacity.used.checked_add(growth)?;
        if capacity.used > budget {
            return None;
        }
        self.save_capacity(&capacity)
    }

    /// Remove up to [`EXPIRED_PER_SCAN`] expired entries, then the oldest disk
    /// reads/writes, which those it left count among. A failed scan never becomes
    /// a partial byte count that could admit excess data. A scan that stopped at
    /// its share reads as long past, so the next write scans again; given the
    /// `known` count it stops there, that count less what it removed, which stays
    /// an upper bound.
    fn trim(
        &self,
        budget: u64,
        protected: Option<&Path>,
        at: u64,
        known: Option<u64>,
    ) -> io::Result<Capacity> {
        let now = SystemTime::now();
        let (mut expired_removed, mut freed) = (0, 0);
        let unix_now = now_unix();
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            if let Some(used) = known.filter(|_| expired_removed >= EXPIRED_PER_SCAN) {
                return Ok(Capacity {
                    used: used.saturating_sub(freed),
                    scanned_at: 0,
                });
            }
            let path = entry?.path();
            if let Some(entry) = self.entry_of_lock(&path) {
                if expired_removed < EXPIRED_PER_SCAN && remove_unneeded_lock(&path, &entry) {
                    expired_removed += 1;
                }
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
                        now.duration_since(meta.modified().unwrap_or(now))
                            .is_ok_and(|age| age >= crate::cache::CONTINUATION_IDLE)
                    } else {
                        expired(&path, unix_now)
                    }
                };
                let in_share = expired_removed < EXPIRED_PER_SCAN;
                if !meta.is_file() || (in_share && Some(path.as_path()) != protected && aged()) {
                    match fs::remove_file(&path) {
                        Ok(()) => {
                            expired_removed += 1;
                            freed += meta.len();
                            continue;
                        }
                        Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                        Err(error) if !meta.is_file() => return Err(error),
                        Err(_) => {}
                    }
                }
                files.push((meta.modified()?, meta.len(), path));
            } else if extension == Some(RETIRED_EXTENSION)
                || (extension == Some("tmp")
                    && now
                        .duration_since(meta.modified()?)
                        .is_ok_and(|age| age > ORPHAN_TEMP_AGE))
            {
                let _ = fs::remove_file(path);
            }
        }
        let mut total = files
            .iter()
            .try_fold(0_u64, |total, (_, len, _)| total.checked_add(*len))
            .ok_or_else(|| io::Error::other("cache byte count overflow"))?;
        if total > budget {
            files.sort_unstable();
            for (_, len, path) in files {
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
            scanned_at: if expired_removed < EXPIRED_PER_SCAN {
                at
            } else {
                0
            },
        })
    }

    #[cfg(test)]
    pub(crate) fn prune(&self, budget: u64) {
        let Some(_lock) = self.lock(Instant::now() + REUSE_LOCK_WAIT) else {
            return;
        };
        if let Ok(capacity) = self.trim(budget, None, now_unix(), None) {
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
