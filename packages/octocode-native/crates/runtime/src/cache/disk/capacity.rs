use super::*;
use octocode_io::lock::{LOCK_POLL, LockedFile, lock_within};
use std::io::{self, Write};
use std::path::Path;

const CAPACITY: &str = ".capacity";
const LOCK: &str = ".capacity.lock";
const SCAN_INTERVAL: u64 = 60;

/// A conservative upper bound. Reserve before publishing; deletions and failed
/// writes can overcount until the next scan, but cannot admit too many bytes.
/// Writes are not fsynced; this bound does not guarantee power-loss recovery.
#[derive(Deserialize, Serialize)]
struct Capacity {
    used: u64,
    scanned_at: u64,
}

impl Disk {
    fn lock(&self) -> Option<LockedFile> {
        octocode_io::atomic::create_private_dir_all(&self.dir).ok()?;
        let file = crate::private_file::open_no_follow(&self.dir.join(LOCK), true).ok()?;
        lock_within(file, Duration::from_secs(1), LOCK_POLL).ok()
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

    pub(super) fn publish(&self, path: &Path, parts: [&[u8]; 2], config: &CacheConfig) -> bool {
        let Some(size) = parts[0].len().checked_add(parts[1].len()) else {
            return false;
        };
        if size > config.max_bytes || size as u64 > config.disk_bytes {
            return false;
        }
        let Some(_lock) = self.lock() else {
            return false;
        };
        self.publish_locked(path, parts, size as u64, config.disk_bytes)
            .is_some()
    }

    fn publish_locked(&self, path: &Path, parts: [&[u8]; 2], size: u64, budget: u64) -> Option<()> {
        let now = now_unix();
        let mut capacity = match self.capacity() {
            Some(capacity) if now.saturating_sub(capacity.scanned_at) < SCAN_INTERVAL => capacity,
            _ => Capacity {
                used: self.trim(budget, Some(path)).ok()?,
                scanned_at: now,
            },
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
            capacity.used = self.trim(target, Some(path)).ok()?;
            capacity.scanned_at = now;
        }
        let growth = size.saturating_sub(entry_size(path).ok()?);
        capacity.used = capacity.used.checked_add(growth)?;
        if capacity.used > budget {
            return None;
        }
        self.save_capacity(&capacity)?;
        octocode_io::atomic::write_atomic_with(path, 0o600, false, |file| {
            for part in parts {
                file.write_all(part)?;
            }
            Ok(())
        })
        .ok()
    }

    /// Remove expired entries, then the oldest disk reads/writes. A failed
    /// scan never becomes a partial byte count that could admit excess data.
    fn trim(&self, budget: u64, protected: Option<&Path>) -> io::Result<u64> {
        let now = SystemTime::now();
        let unix_now = now_unix();
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let path = entry?.path();
            let meta = match fs::symlink_metadata(&path) {
                Ok(meta) => meta,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let extension = path.extension().and_then(|ext| ext.to_str());
            if extension == Some(ENTRY_EXTENSION) {
                if !meta.is_file()
                    || (Some(path.as_path()) != protected && expired(&path, unix_now))
                {
                    match fs::remove_file(&path) {
                        Ok(()) => continue,
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
        Ok(total)
    }

    #[cfg(test)]
    pub(crate) fn prune(&self, budget: u64) {
        let Some(_lock) = self.lock() else {
            return;
        };
        if let Ok(used) = self.trim(budget, None) {
            let _ = self.save_capacity(&Capacity {
                used,
                scanned_at: now_unix(),
            });
        }
    }
}

fn entry_size(path: &Path) -> io::Result<u64> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => Ok(meta.len()),
        Ok(_) => Err(io::Error::other("cache entry is not a regular file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}
