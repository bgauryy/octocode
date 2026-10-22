//! Optional on-disk mirror of a key's blocking facts, so short-lived CLI
//! processes honor each other's limits.
use super::{KeyState, key_state::KeyFacts, now_ms, pause};
use crate::ProviderError;
use octocode_io::lock::TryLock;
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
    match std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&name)
    {
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
mod tests {
    use super::super::slots::{SlotPlan, Subject};
    use super::super::*;
    use super::KeyFacts;
    use reqwest::header::HeaderMap;
    use std::collections::BTreeMap;

    #[test]
    fn code_search_window_merge_preserves_same_millisecond_reservations() {
        let now = 1_000_000;
        let stamp = now - 1;
        for (local, peer, count) in [
            (vec![stamp, stamp], vec![stamp, stamp], 2),
            (vec![stamp, stamp], vec![stamp], 2),
            (vec![stamp], vec![stamp, stamp], 2),
            (vec![stamp, stamp], vec![stamp, stamp, stamp], 3),
            (vec![stamp, stamp, stamp], vec![stamp, stamp], 3),
        ] {
            let mut facts = KeyFacts {
                code_search_ms: local.into(),
                ..KeyFacts::default()
            };
            facts.merge(
                KeyFacts {
                    code_search_ms: peer.into(),
                    ..KeyFacts::default()
                },
                now,
            );
            assert_eq!(facts.code_search_ms, vec![stamp; count]);
            let plan = SlotPlan::new(
                Group::CodeSearch,
                &ExecutorConfig {
                    code_search_per_minute: 2,
                    ..ExecutorConfig::relaxed()
                },
                true,
                Duration::from_secs(1),
                Instant::now() + Duration::from_secs(10),
            );
            assert!(
                plan.refusal(plan.start(&facts, now), Subject::Request)
                    .is_some()
            );
        }
    }

    #[test]
    fn code_search_window_merge_is_idempotent_sorted_and_expires_stamps() {
        let now = 1_000_000;
        let expired = now - 60_000;
        let stamp = now - 2;
        let mut facts = KeyFacts {
            code_search_ms: vec![now - 1, stamp, stamp, expired].into(),
            ..KeyFacts::default()
        };
        let peer = KeyFacts {
            code_search_ms: vec![stamp, now - 1, expired - 1].into(),
            ..KeyFacts::default()
        };
        for _ in 0..3 {
            facts.merge(peer.clone(), now);
            assert_eq!(facts.code_search_ms, vec![stamp, stamp, now - 1]);
            facts.merge(facts.clone(), now);
            assert_eq!(facts.code_search_ms, vec![stamp, stamp, now - 1]);
        }
    }

    /// A rate-limit fact learned on an async worker never waits for the peer
    /// lock there; it still reaches the file once the peer lets go.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_cooldown_on_an_async_worker_does_not_wait_for_the_peer_lock() {
        let dir = tempfile::tempdir().expect("tempdir");
        let key = LimiterKey::new("ghe.example", Some("tok"));
        let budget = GitHubBudget::relaxed();
        let state = budget.key_state(&key, Some(dir.path()));
        let file = dir.path().join(key.file_name());
        let mut lock_name = file.as_os_str().to_os_string();
        lock_name.push(".lock");
        let peer = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(std::path::PathBuf::from(lock_name))
            .expect("lock file");
        let held = octocode_io::lock::lock_within(peer, Duration::ZERO, Duration::from_millis(10))
            .expect("peer holds the lock");
        let until = now_ms() + 60_000;
        let started = Instant::now();
        state.cool_down(until);
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "cool_down blocked the worker for {:?}",
            started.elapsed()
        );
        drop(held);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let written = std::fs::read(&file)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<KeyFacts>(&bytes).ok())
                .is_some_and(|facts| facts.cooldown_until_ms >= until);
            if written {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the cooldown never reached the file"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_persisted_budget_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let key = LimiterKey::new("ghe.example", Some("tok"));
        {
            let budget = GitHubBudget::relaxed();
            let state = budget.key_state(&key, Some(dir.path()));
            state.cool_down(now_ms() + 30_000);
        }
        let mode = std::fs::metadata(dir.path().join(key.file_name()))
            .expect("state file")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn disk_state_round_trips_blocking_facts_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let key = LimiterKey::new("ghe.example", Some("tok"));
        let reset = now_ms() / 1000 + 300;
        {
            let budget = GitHubBudget::relaxed();
            let state = budget.key_state(&key, Some(dir.path()));
            let mut headers = HeaderMap::new();
            headers.insert("x-ratelimit-remaining", "42".parse().expect("header"));
            headers.insert(
                "x-ratelimit-reset",
                reset.to_string().parse().expect("header"),
            );
            headers.insert("x-ratelimit-resource", "core".parse().expect("header"));
            state.observe(&headers, "core");
            state.exhaust("search", reset);
            state.cool_down(now_ms() + 30_000);
        }
        let file = dir.path().join(key.file_name());
        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).expect("state file")).expect("json");
        assert!(raw["buckets"].get("core").is_none(), "{raw}");
        assert_eq!(raw["buckets"]["search"]["remaining"], 0);
        // A fresh process (new registry) sees the other process's facts.
        let budget = GitHubBudget::relaxed();
        let state = budget.key_state(&key, Some(dir.path()));
        let search = state.blocked("search", budget.config()).expect("search");
        assert_eq!(search.reset, Some(reset));
        let core = state.blocked("core", budget.config()).expect("cooldown");
        assert_eq!(core.reset, None);
        let leftovers = std::fs::read_dir(dir.path())
            .expect("dir")
            .filter(|entry| {
                entry
                    .as_ref()
                    .is_ok_and(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    /// Concurrent writers that do not share memory (separate
    /// processes; here separate registries) never drop each other's facts.
    #[test]
    fn concurrent_writers_keep_every_fact() {
        let dir = tempfile::tempdir().expect("tempdir");
        let key = LimiterKey::new("ghe.example", Some("tok"));
        let now = now_ms();
        std::thread::scope(|scope| {
            for writer in 0..8 {
                let (dir, key) = (dir.path(), &key);
                scope.spawn(move || {
                    let budget = GitHubBudget::relaxed();
                    let state = budget.key_state(key, Some(dir));
                    for round in 0..25 {
                        state.persist_with(|view| {
                            view.last_start_ms.insert(format!("w{writer}-{round}"), now);
                        });
                    }
                });
            }
        });
        let disk: KeyFacts =
            serde_json::from_slice(&std::fs::read(dir.path().join(key.file_name())).expect("file"))
                .expect("json");
        assert_eq!(disk.last_start_ms.len(), 8 * 25);
    }

    /// A peer's write in the same clock tick as the last one seen is still
    /// merged: every write renames a fresh file, so its identity changes even
    /// when its mtime does not.
    #[test]
    fn a_peer_write_with_an_unchanged_mtime_is_still_merged() {
        let dir = tempfile::tempdir().expect("tempdir");
        let key = LimiterKey::new("ghe.example", Some("tok"));
        let file = dir.path().join(key.file_name());
        let (peer_budget, budget) = (GitHubBudget::relaxed(), GitHubBudget::relaxed());
        let peer = peer_budget.key_state(&key, Some(dir.path()));
        let first = now_ms() + 30_000;
        peer.cool_down(first);
        let mtime = std::fs::metadata(&file)
            .and_then(|meta| meta.modified())
            .expect("mtime");
        let state = budget.key_state(&key, Some(dir.path()));
        state.refresh_from_disk();
        assert_eq!(state.facts().cooldown_until_ms, first);

        let second = first + 30_000;
        peer.cool_down(second);
        std::fs::File::options()
            .write(true)
            .open(&file)
            .and_then(|file| file.set_modified(mtime))
            .expect("same tick");
        state.refresh_from_disk();
        assert_eq!(state.facts().cooldown_until_ms, second);
    }

    /// A repeat of the same blocking fact leaves the file in place; a new
    /// fact replaces it.
    #[cfg(unix)]
    #[test]
    fn an_unchanged_view_is_not_rewritten() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().expect("tempdir");
        let key = LimiterKey::new("ghe.example", Some("tok"));
        let budget = GitHubBudget::relaxed();
        let state = budget.key_state(&key, Some(dir.path()));
        let file = dir.path().join(key.file_name());
        let inode = || std::fs::metadata(&file).expect("state file").ino();
        let reset = now_ms() / 1000 + 300;
        state.exhaust("search", reset);
        let first = inode();
        state.exhaust("search", reset);
        assert_eq!(inode(), first, "same view, no rewrite");
        state.cool_down(now_ms() + 30_000);
        assert_ne!(inode(), first, "a new cooldown is written");
    }

    async fn admit_code_search(
        state: &Arc<KeyState>,
        budget: &GitHubBudget,
        cap: Duration,
    ) -> Result<Admission, ProviderError> {
        state
            .admit(
                Some(Group::CodeSearch),
                budget.config(),
                true,
                cap,
                Instant::now() + Duration::from_secs(10),
                &CancellationToken::new(),
            )
            .await
    }

    /// Two budgets with separate key states and one state file stand in for
    /// two processes: together they stay within one window, and their starts
    /// keep the spacing across both. The spacing is read from the starts the
    /// shared file reserved, and the wait from the last admission against the
    /// loop's start: a task resumed late after one admission would shorten
    /// the gap observed to the next, so observed gaps prove nothing.
    #[tokio::test]
    async fn two_processes_share_one_window_and_one_spacing() {
        let dir = tempfile::tempdir().expect("state directory");
        let key = LimiterKey::new("h", Some("tok"));
        let config = ExecutorConfig {
            code_search_per_minute: 4,
            search_spacing: Duration::from_millis(150),
            ..ExecutorConfig::relaxed()
        };
        let budgets = [
            GitHubBudget::with_config(config.clone()),
            GitHubBudget::with_config(config),
        ];
        let states = budgets
            .iter()
            .map(|budget| budget.key_state(&key, Some(dir.path())))
            .collect::<Vec<_>>();
        let mut starts = Vec::new();
        let mut refused = 0;
        let began = Instant::now();
        for turn in 0..6 {
            let which = turn % 2;
            match admit_code_search(&states[which], &budgets[which], Duration::from_secs(1)).await {
                Ok(admission) => {
                    starts.push(Instant::now());
                    drop(admission);
                }
                Err(error) => {
                    assert_eq!(error.kind, ProviderErrorKind::RateLimited);
                    refused += 1;
                }
            }
        }
        assert_eq!((starts.len(), refused), (4, 2), "one window of 4 for both");
        let disk: KeyFacts =
            serde_json::from_slice(&std::fs::read(dir.path().join(key.file_name())).expect("file"))
                .expect("json");
        assert_eq!(disk.code_search_ms.len(), 4);
        let reserved = Vec::from(disk.code_search_ms);
        for pair in reserved.windows(2) {
            assert!(
                pair[1] - pair[0] >= 150,
                "reserved starts {}ms apart across processes",
                pair[1] - pair[0]
            );
        }
        // Each admission waits for its reserved start: the fourth is three
        // spacings after the first, which is no earlier than the loop began
        // (a few ms for the stamps' millisecond rounding).
        assert!(
            starts[3] - began >= Duration::from_millis(445),
            "the fourth start came {:?} after the first request",
            starts[3] - began
        );
    }

    /// A corrupt file, stamps older than the window, and stamps from a clock
    /// far ahead hold nothing back: the admission goes ahead at once and the
    /// file is valid again.
    #[tokio::test]
    async fn a_corrupt_stale_or_future_stamped_file_is_recovered() {
        let key = LimiterKey::new("h", Some("tok"));
        let now = now_ms();
        let hour = 3_600_000;
        let full_window = |stamp: u64| KeyFacts {
            code_search_ms: vec![stamp; 10].into(),
            last_start_ms: BTreeMap::from([("code_search".to_owned(), stamp)]),
            ..KeyFacts::default()
        };
        let files = [
            b"{ not json".to_vec(),
            serde_json::to_vec(&full_window(now - 2 * 60_000)).expect("stale"),
            serde_json::to_vec(&full_window(now + hour)).expect("future"),
        ];
        for bytes in files {
            let dir = tempfile::tempdir().expect("state directory");
            std::fs::write(dir.path().join(key.file_name()), &bytes).expect("seed");
            let budget = GitHubBudget::with_config(ExecutorConfig {
                search_spacing: Duration::from_secs(2),
                ..ExecutorConfig::octokit()
            });
            let state = budget.key_state(&key, Some(dir.path()));
            let started = Instant::now();
            let admission = admit_code_search(&state, &budget, Duration::from_secs(1)).await;
            assert!(admission.is_ok(), "{:?}", admission.err());
            assert!(started.elapsed() < Duration::from_millis(500));
            let disk: KeyFacts = serde_json::from_slice(
                &std::fs::read(dir.path().join(key.file_name())).expect("file"),
            )
            .expect("the file is valid json again");
            assert_eq!(disk.code_search_ms.len(), 1, "{disk:?}");
        }
    }

    /// A state directory that cannot be created degrades to this process's
    /// own pacing: the request is admitted and its window still applies.
    #[tokio::test]
    async fn an_unusable_state_directory_degrades_to_per_process_pacing() {
        let parent = tempfile::tempdir().expect("tempdir");
        let blocker = parent.path().join("not-a-directory");
        std::fs::write(&blocker, b"file").expect("blocker");
        let budget = GitHubBudget::with_config(ExecutorConfig {
            code_search_per_minute: 1,
            ..ExecutorConfig::relaxed()
        });
        let state = budget.key_state(
            &LimiterKey::new("h", None),
            Some(&blocker.join("ratelimit")),
        );
        let first = admit_code_search(&state, &budget, Duration::from_secs(1)).await;
        assert!(first.is_ok(), "{:?}", first.err());
        let second = admit_code_search(&state, &budget, Duration::from_secs(1)).await;
        assert_eq!(
            second.err().map(|error| error.kind),
            Some(ProviderErrorKind::RateLimited)
        );
    }

    #[test]
    fn peer_stamps_beyond_the_horizon_are_not_merged() {
        let now = 1_000_000_000;
        let beyond = now + super::PEER_STAMP_HORIZON_MS + 1;
        let mut facts = KeyFacts::default();
        facts.merge(
            KeyFacts {
                code_search_ms: vec![beyond, now].into(),
                last_start_ms: BTreeMap::from([("code_search".to_owned(), beyond)]),
                ..KeyFacts::default()
            },
            now,
        );
        assert_eq!(facts.code_search_ms, vec![now]);
        assert!(facts.last_start_ms.is_empty());
    }
}
