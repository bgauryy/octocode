//! Optional on-disk mirror of a key's blocking facts, so short-lived CLI
//! processes honor each other's limits.
use super::{KeyState, key_state::KeyFacts, now_ms};
use crate::ProviderError;
use lock::{WriteLock, lock_file, lock_file_blocking, off_worker};
use octocode_io::cancellation::Cancellation;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};

/// The longest a write that cannot be cancelled waits for a peer's lock.
/// Past it the write is deferred, never made unlocked: the facts stay in
/// memory and the next write carries them ([`KeyState::persist_with`]).
const WRITE_LOCK_WAIT: Duration = Duration::from_secs(1);
/// How far past now a peer's start stamp may lie and still count. A start is
/// reserved at most one quota window ahead (the spacing chain until the
/// bucket resets, inside the tool deadline), so a stamp beyond this comes
/// from a clock that jumped or a damaged file, never from a live reservation,
/// and it must not hold the group until its own time arrives.
const PEER_STAMP_HORIZON_MS: u64 = 120_000;

/// Say on stderr, once per process ([`octocode_io::notice`]), that the shared
/// record cannot be used and this process paces alone: the request still goes
/// ahead under its own limits.
fn warn_unshared(action: &str, path: &Path, error: &dyn std::fmt::Display) {
    octocode_io::notice::raise(
        "github-budget-unshared",
        format!(
            "[octocode] warning: GitHub rate-limit state shared with other octocode processes cannot be used ({action} {}: {error}); this process paces its GitHub requests alone.",
            path.display()
        ),
    );
}

/// The state file's bytes: `None` when it does not exist yet, or cannot be
/// read (with the warning).
fn read_state(path: &Path) -> Option<Vec<u8>> {
    #[cfg(test)]
    record_io(path);
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            warn_unshared("read", path, &error);
            None
        }
    }
}

/// Which thread touched which state file, for the tests that keep this file
/// IO off the async workers.
#[cfg(test)]
static IO_THREADS: std::sync::Mutex<Vec<(PathBuf, std::thread::ThreadId)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
fn record_io(path: &Path) {
    IO_THREADS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push((path.to_path_buf(), std::thread::current().id()));
}

/// The threads that read or wrote `path`'s state file.
#[cfg(test)]
fn io_threads(path: &Path) -> Vec<std::thread::ThreadId> {
    IO_THREADS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|(seen, _)| seen == path)
        .map(|(_, thread)| *thread)
        .collect()
}

pub(super) struct Persist {
    pub(super) path: PathBuf,
    pub(super) seen: Option<FileStamp>,
}

/// What tells one write of the state file from the next: every write renames
/// a fresh file, so its inode changes even when two writes share an mtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
    #[cfg(unix)]
    inode: (u64, u64),
}

fn file_stamp(path: &Path) -> Option<FileStamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(FileStamp {
        modified: meta.modified().ok(),
        len: meta.len(),
        #[cfg(unix)]
        inode: {
            use std::os::unix::fs::MetadataExt;
            (meta.dev(), meta.ino())
        },
    })
}

impl KeyFacts {
    /// Keep the most restrictive of two views (memory vs. disk).
    pub(super) fn merge(&mut self, other: Self, now_ms: u64) {
        for (name, bucket) in other.buckets {
            if bucket.reset.saturating_mul(1000) <= now_ms {
                continue;
            }
            let replace = self
                .buckets
                .get(&name)
                .is_none_or(|current| bucket.restricts_more_than(current));
            if replace {
                self.buckets.insert(name, bucket);
            }
        }
        self.cooldown_until_ms = self.cooldown_until_ms.max(other.cooldown_until_ms);
        self.pull_request_max_age = match (self.pull_request_max_age, other.pull_request_max_age) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (left, right) => left.or(right),
        };
        let horizon = now_ms.saturating_add(PEER_STAMP_HORIZON_MS);
        for (group, stamp) in other.last_start_ms {
            if stamp > horizon {
                continue;
            }
            let entry = self.last_start_ms.entry(group).or_default();
            *entry = (*entry).max(stamp);
        }
    }

    /// Only facts that can block, pace or answer a future request are
    /// persisted: exhausted buckets, the live quota of quota-paced groups, and
    /// the lifetime GitHub states for a pull request.
    pub(super) fn blocking_view(&self, now_ms: u64) -> Self {
        Self {
            buckets: self
                .buckets
                .iter()
                .filter(|(name, b)| {
                    (b.remaining == 0 || super::Group::paces_bucket(name))
                        && b.reset.saturating_mul(1000) > now_ms
                })
                .map(|(name, b)| (name.clone(), *b))
                .collect(),
            cooldown_until_ms: if self.cooldown_until_ms > now_ms {
                self.cooldown_until_ms
            } else {
                0
            },
            last_start_ms: self
                .last_start_ms
                .iter()
                .filter(|(_, stamp)| stamp.saturating_add(60_000) > now_ms)
                .map(|(group, stamp)| (group.clone(), *stamp))
                .collect(),
            pull_request_max_age: self.pull_request_max_age,
            circuit_failures: 0,
            circuit_open_until_ms: 0,
            circuit_cause: None,
        }
    }
}

impl KeyState {
    /// Mirror this key's facts to its file under `dir`, created here once:
    /// a directory that cannot be created leaves the key in-memory only.
    pub(super) fn attach_dir(&self, dir: &Path) {
        let mut persist = self
            .persist
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if persist.is_some() {
            return;
        }
        let path = dir.join(self.key.file_name());
        if let Err(error) = std::fs::create_dir_all(dir) {
            warn_unshared("create the directory of", &path, &error);
            return;
        }
        *persist = Some(Persist { path, seen: None });
    }

    /// Merge the on-disk view when another process has written it since we
    /// last looked (one `stat` per logical request; see [`FileStamp`]).
    pub(crate) fn refresh_from_disk(&self) {
        let mut persist = self
            .persist
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(persist) = persist.as_mut() else {
            return;
        };
        let Some(stamp) = file_stamp(&persist.path) else {
            return;
        };
        if persist.seen == Some(stamp) {
            return;
        }
        persist.seen = Some(stamp);
        let Some(disk) = read_state(&persist.path)
            .and_then(|bytes| serde_json::from_slice::<KeyFacts>(&bytes).ok())
        else {
            return;
        };
        self.facts().merge(disk, now_ms());
    }

    /// Run `reserve` on this key's facts with the on-disk facts of every
    /// other process merged in, and write the result, all under the file
    /// lock: a slot one process takes is visible to the next before it
    /// reserves. Without a mirror it runs on the in-memory facts alone. A lock
    /// a peer process holds is waited for without blocking the runtime, until
    /// the request's `deadline` or `cancellation`.
    ///
    /// The read and the write run on the blocking pool, not on the async
    /// worker that polls this request, and each carries the lock: a request
    /// dropped mid-write releases it only once the write is done.
    pub(super) async fn reserve_shared<R>(
        self: &Arc<Self>,
        deadline: Instant,
        cancellation: &Cancellation,
        reserve: impl FnOnce(&mut KeyFacts) -> R,
    ) -> Result<R, ProviderError> {
        let path = self
            .persist
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|persist| persist.path.clone());
        let Some(path) = path else {
            return Ok(reserve(&mut self.facts()));
        };
        let lock = lock_file(&path, deadline, cancellation).await?;
        let (lock, path, on_disk) = off_worker(move || {
            let on_disk = read_state(&path);
            (lock, path, on_disk)
        })
        .await?;
        let (result, view) = {
            let mut facts = self.facts();
            if let Some(disk) = on_disk
                .as_deref()
                .and_then(|bytes| serde_json::from_slice::<KeyFacts>(bytes).ok())
            {
                facts.merge(disk, now_ms());
            }
            let result = reserve(&mut facts);
            (result, facts.blocking_view(now_ms()))
        };
        let state = Arc::clone(self);
        off_worker(move || {
            state.write_view(&path, on_disk.as_deref(), &view);
            drop(lock);
        })
        .await?;
        Ok(result)
    }

    /// Load-merge-write via a unique temp file + atomic rename.
    /// Mirror the blocking facts to disk. On an async worker the write runs on
    /// the blocking pool: it takes the peer lock and may wait for it up to
    /// `WRITE_LOCK_WAIT`. Each write merges the current view into the file, so
    /// two writes finishing out of order still leave the latest facts. Runtime
    /// shutdown waits for pending blocking writes, so none is lost at exit.
    pub(super) fn persist(self: &Arc<Self>) {
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let state = Arc::clone(self);
                drop(handle.spawn_blocking(octocode_io::notice::carried(move || {
                    state.persist_with(|_| {});
                })));
            }
            Err(_) => self.persist_with(|_| {}),
        }
    }

    /// [`Self::persist`] with `adjust` applied to the merged view before the
    /// write. A merged view equal to the file is not written again (a repeat
    /// exhaust or cooldown with the same reset); a new start reservation
    /// always changes it, and it must reach the file, because it is what
    /// spaces the next process's search.
    pub(super) fn persist_with(&self, adjust: impl FnOnce(&mut KeyFacts)) {
        let path = {
            let persist = self
                .persist
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match persist.as_ref() {
                Some(persist) => persist.path.clone(),
                None => return,
            }
        };
        // Serialize read-merge-write across processes: without it, two CLI
        // processes read the same file and the later rename drops the other's
        // start stamps and window slots. Released on drop. Only a filesystem
        // without lock support writes unlocked (as every process on it does).
        // A lock not taken in time defers the write: `adjust` lands in the
        // in-memory facts, which the next write merges in, so a skipped write
        // costs freshness while an unlocked one could drop a peer's facts.
        let _lock = match lock_file_blocking(&path, Instant::now() + WRITE_LOCK_WAIT) {
            WriteLock::Held(lock) => Some(lock),
            WriteLock::Unsupported => None,
            WriteLock::Unavailable => {
                adjust(&mut self.facts());
                return;
            }
        };
        let now = now_ms();
        let mut view = self.facts().blocking_view(now);
        let on_disk = read_state(&path);
        if let Some(disk) = on_disk
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<KeyFacts>(bytes).ok())
        {
            view.merge(disk, now);
        }
        adjust(&mut view);
        self.write_view(&path, on_disk.as_deref(), &view);
    }

    /// Write `view` unless the file already holds it, through a unique temp
    /// file and an atomic rename; the caller holds the file lock.
    fn write_view(&self, path: &Path, on_disk: Option<&[u8]>, view: &KeyFacts) {
        #[cfg(test)]
        record_io(path);
        let Ok(bytes) = serde_json::to_vec(view) else {
            return;
        };
        if on_disk == Some(bytes.as_slice()) {
            return;
        }
        // No fsync: this is a rebuildable cache, and a flush here would hold the
        // peer lock for a disk write (see `WRITE_LOCK_WAIT`). The rename keeps it whole.
        if let Err(error) = octocode_io::atomic::write_atomic(path, &bytes, false) {
            warn_unshared("write", path, &error);
            return;
        }
        if let Some(stamp) = file_stamp(path)
            && let Some(persist) = self
                .persist
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut()
        {
            persist.seen = Some(stamp);
        }
    }
}

mod lock;
#[cfg(test)]
mod recovery_tests;
#[cfg(test)]
mod tests;
