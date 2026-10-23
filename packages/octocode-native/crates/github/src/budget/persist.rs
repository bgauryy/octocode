//! Optional on-disk mirror of a key's blocking facts, so short-lived CLI
//! processes honor each other's limits.
use super::{KeyState, key_state::KeyFacts, now_ms};
use crate::ProviderError;
use octocode_io::{budget::pause, lock::TryLock};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use tokio_util::sync::CancellationToken;

/// How often a held state lock is retried.
const LOCK_POLL: Duration = Duration::from_millis(5);
/// The longest a write that cannot be cancelled waits for a peer's lock
/// before it goes ahead unlocked, as on a filesystem without lock support.
const WRITE_LOCK_WAIT: Duration = Duration::from_secs(1);
/// How far past now a peer's start stamp may lie and still count. A start is
/// reserved at most one wait ahead (the 60 s code-search window plus the
/// spacing chain, inside the tool deadline), so a stamp beyond this comes from
/// a clock that jumped or a damaged file, never from a live reservation, and
/// it must not hold the window until its own time arrives.
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
    match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            warn_unshared("read", path, &error);
            None
        }
    }
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
    pub(super) fn merge(&mut self, other: KeyFacts, now_ms: u64) {
        for (name, bucket) in other.buckets {
            if bucket.remaining != 0 || bucket.reset.saturating_mul(1000) <= now_ms {
                continue;
            }
            let replace = self
                .buckets
                .get(&name)
                .is_none_or(|current| current.reset < bucket.reset || current.remaining > 0);
            if replace {
                self.buckets.insert(name, bucket);
            }
        }
        self.cooldown_until_ms = self.cooldown_until_ms.max(other.cooldown_until_ms);
        let horizon = now_ms.saturating_add(PEER_STAMP_HORIZON_MS);
        for (group, stamp) in other.last_start_ms {
            if stamp > horizon {
                continue;
            }
            let entry = self.last_start_ms.entry(group).or_default();
            *entry = (*entry).max(stamp);
        }
        // The views overlap, but distinct reservations can share a millisecond.
        // Keep the larger count per timestamp without counting mirrored slots twice.
        let mut window = BTreeMap::<u64, [usize; 2]>::new();
        for (side, stamps) in [&self.code_search_ms, &other.code_search_ms]
            .into_iter()
            .enumerate()
        {
            for &stamp in stamps {
                let peer_beyond_horizon = side == 1 && stamp > horizon;
                if stamp.saturating_add(60_000) > now_ms && !peer_beyond_horizon {
                    window.entry(stamp).or_default()[side] += 1;
                }
            }
        }
        self.code_search_ms = window
            .into_iter()
            .flat_map(|(stamp, [local, peer])| std::iter::repeat_n(stamp, local.max(peer)))
            .collect();
    }

    /// Only facts that can block a future request are persisted.
    pub(super) fn blocking_view(&self, now_ms: u64) -> KeyFacts {
        KeyFacts {
            buckets: self
                .buckets
                .iter()
                .filter(|(_, b)| b.remaining == 0 && b.reset.saturating_mul(1000) > now_ms)
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
            code_search_ms: self
                .code_search_ms
                .iter()
                .copied()
                .filter(|stamp| stamp.saturating_add(60_000) > now_ms)
                .collect(),
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
    pub(super) async fn reserve_shared<R>(
        &self,
        deadline: Instant,
        cancellation: &CancellationToken,
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
        let _lock = lock_file(&path, deadline, cancellation).await?;
        let on_disk = read_state(&path);
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
        self.write_view(&path, on_disk.as_deref(), &view);
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
        // start stamps and window slots. Released on drop. A
        // filesystem without lock support, or a peer that holds the lock
        // past `WRITE_LOCK_WAIT`, degrades to the unlocked write.
        let _lock = lock_file_blocking(&path, Instant::now() + WRITE_LOCK_WAIT);
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

/// Exclusive advisory lock on the `<state>.lock` sidecar, polled so that the
/// wait yields to the runtime and ends at the `deadline` or on cancellation.
/// `Ok(None)` when the file cannot be opened or the filesystem does not
/// support locking.
async fn lock_file(
    path: &Path,
    deadline: Instant,
    cancellation: &CancellationToken,
) -> Result<Option<octocode_io::lock::LockedFile>, ProviderError> {
    let Some(mut file) = open_lock_file(path) else {
        return Ok(None);
    };
    loop {
        match octocode_io::lock::try_lock(file) {
            Ok(TryLock::Locked(lock)) => return Ok(Some(lock)),
            Ok(TryLock::Busy(busy)) => {
                file = busy;
                pause(LOCK_POLL, deadline, cancellation).await?;
            }
            Err(error) => {
                warn_unshared("lock", path, &error);
                return Ok(None);
            }
        }
    }
}

/// [`lock_file`] for synchronous writers: `None` when the lock is not taken by
/// `deadline`, so a stalled peer cannot hold the caller indefinitely.
fn lock_file_blocking(path: &Path, deadline: Instant) -> Option<octocode_io::lock::LockedFile> {
    let file = open_lock_file(path)?;
    match octocode_io::lock::lock_within(
        file,
        deadline.saturating_duration_since(Instant::now()),
        LOCK_POLL,
    ) {
        Ok(lock) => Some(lock),
        Err(octocode_io::lock::LockError::Busy) => None,
        Err(octocode_io::lock::LockError::Io(error)) => {
            warn_unshared("lock", path, &error);
            None
        }
    }
}

#[cfg(test)]
mod shared_window_tests;
#[cfg(test)]
mod tests;
