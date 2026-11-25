//! Retention sweeps of native caches: once at start, then on a periodic check
//! for as long as the runtime lives. Every run passes through the same 24 h
//! marker gate.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

use octocode_io::cancellation::Cancellation;

const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MARKER: &str = ".last-cache-maintenance";
/// How often a live runtime checks whether the sweep is due. The marker gate
/// still decides; a check that finds it not due reads one small file.
const CHECK_EVERY: Duration = Duration::new(INTERVAL.as_secs() / 24, 0);

/// Source of the current time; tests inject one.
pub(super) type Clock = Arc<dyn Fn() -> SystemTime + Send + Sync>;

/// The periodic sweep of one runtime. Closing or dropping it stops the task.
#[derive(Default)]
pub(super) struct Periodic {
    stop: Cancellation,
    task: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Periodic {
    /// Starts checking every `every` on the current tokio runtime; nothing
    /// starts outside one (a host without a runtime ends with its call).
    pub(super) fn start(
        &self,
        home: PathBuf,
        capture_ttl: Duration,
        every: Duration,
        clock: Clock,
    ) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let stop = self.stop.clone();
        let task = handle.spawn(async move {
            // A fixed schedule: a sweep that runs long does not move the next check.
            let mut check = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
            loop {
                tokio::select! {
                    () = stop.cancelled() => return,
                    _ = check.tick() => {}
                }
                let (home, clock) = (home.clone(), Arc::clone(&clock));
                let _ = tokio::task::spawn_blocking(move || {
                    run_if_due_at(&home, capture_ttl, clock());
                })
                .await;
            }
        });
        *self
            .task
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(task);
    }

    /// [`Self::start`] with the production period and the wall clock.
    pub(super) fn start_default(&self, home: PathBuf, capture_ttl: Duration) {
        self.start(home, capture_ttl, CHECK_EVERY, Arc::new(SystemTime::now));
    }

    /// Stops the task and waits for a sweep in progress to finish.
    pub(super) async fn close(&self) {
        self.stop.cancel();
        let task = self
            .task
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

impl Drop for Periodic {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

#[cfg(test)]
type SnapshotLockHook = Box<dyn FnOnce(&Path, &Path)>;

#[cfg(test)]
thread_local! {
    static BEFORE_SNAPSHOT_LOCK: std::cell::RefCell<Option<SnapshotLockHook>> = const {
        std::cell::RefCell::new(None)
    };
}

/// Sweeps existing owned cache directories: when none was ever created
/// (e.g. `storage.mode=memory` on a fresh home), nothing is written. `false`
/// when no sweep is due or it could not be recorded. `capture_ttl` is the URL
/// capture retention, the same value each capture publishes as its `expiresAt`.
pub(super) fn run_if_due(home: &Path, capture_ttl: Duration) -> bool {
    run_if_due_at(home, capture_ttl, SystemTime::now())
}

/// [`run_if_due`] as of `now`.
fn run_if_due_at(home: &Path, capture_ttl: Duration, now: SystemTime) -> bool {
    let tmp = home.join("tmp");
    if !tmp.is_dir()
        && !octocode_runtime_base::cache::Directory::ALL
            .into_iter()
            .map(|directory| directory.path(home))
            .chain(octocode_runtime_base::cache::Directory::retired(home))
            .any(|dir| dir.is_dir())
    {
        return false;
    }
    let marker = home.join(MARKER);
    if let Ok(text) = fs::read_to_string(&marker)
        && let Ok(epoch) = text.trim().parse::<u64>()
    {
        let last = SystemTime::UNIX_EPOCH + Duration::from_secs(epoch);
        if now
            .duration_since(last)
            .is_ok_and(|elapsed| elapsed < INTERVAL)
        {
            return false;
        }
    }
    // Clone outputs may contain local edits. Neither current outputs nor
    // older tmp/clone checkouts are cache entries or safe age-sweep targets.
    // Retained-state stores age each entry by the class it was saved under
    // (a file age would drop entries the store keeps): each store removes
    // its own expired entries here with a complete expiration pass.
    for directory in octocode_runtime_base::cache::Directory::ALL {
        let dir = directory.path(home);
        if dir.is_dir() {
            octocode_runtime_base::cache::Disk::new(dir).prune_expired();
        }
    }
    // Retired store directories have no reader or writer: they go whole.
    for dir in octocode_runtime_base::cache::Directory::retired(home) {
        let _ = fs::remove_dir_all(dir);
    }
    // `tmp/tree` and `tmp/tree-staging` are written by octocode-mcp 19.x
    // (19.1.0 is still its latest published release), which shares this
    // home; native never reads them, so aged entries are cleared here.
    sweep_dir(&tmp.join("tree"), INTERVAL, now);
    sweep_dir(&tmp.join("tree-staging"), INTERVAL, now);
    // Materialized checkouts: each snapshot ages by its last write and is
    // removed only under its own lock, so a materialize in progress keeps
    // it.
    sweep_materialized(
        &octocode_tool_github::gh_structure::materialize::materialize_root(home),
        INTERVAL,
        now,
    );
    // Rate-limit mirrors only hold facts that expire within ~1h.
    sweep_dir(&tmp.join("ratelimit"), INTERVAL, now);
    // URL captures are content-addressed directories, rewritten on every
    // recapture of the same text, so their age is their last capture.
    sweep_dir(
        &octocode_tools_shared::source_capture::url::capture_root(home),
        capture_ttl,
        now,
    );
    let epoch = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    // Without the marker the next start sweeps again: say so, and do not
    // report the sweep as recorded.
    if let Err(error) = fs::write(&marker, epoch.to_string()) {
        octocode_io::notice::raise(
            "cacheMaintenanceMarker",
            format!(
                "The cache maintenance marker could not be written ({}); the sweep reruns at every start.",
                error.kind()
            ),
        );
        return false;
    }
    true
}

/// Removes each materialized snapshot (`<root>/<owner>/<repo>/<commit>`
/// with its `<commit>.manifest.json`) whose newest write is older than
/// `max_age`, holding its `<commit>.lock` as a materialize does: a snapshot
/// whose lock is held is in use and stays. The lock file stays too, so a
/// writer waiting on it never locks a file another writer recreated.
fn sweep_materialized(root: &Path, max_age: Duration, now: SystemTime) {
    let children = |path: &Path| {
        fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .collect::<Vec<_>>()
    };
    for repo in children(root).iter().flat_map(|owner| children(owner)) {
        for snapshot in children(&repo) {
            let Some(commit) = snapshot
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
            else {
                continue;
            };
            let manifest = repo.join(format!("{commit}.manifest.json"));
            if !materialized_stale(&snapshot, &manifest, max_age, now) {
                continue;
            }
            #[cfg(test)]
            BEFORE_SNAPSHOT_LOCK.with(|hook| {
                if let Some(hook) = hook.take() {
                    hook(&snapshot, &manifest);
                }
            });
            let Ok(file) = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(repo.join(format!("{commit}.lock")))
            else {
                continue;
            };
            let Ok(_lease) = octocode_io::lock::lock_within(file, Duration::ZERO, Duration::ZERO)
            else {
                continue;
            };
            // A writer may have refreshed the snapshot after the unlocked
            // age check. Its lock makes this verdict and deletion one transaction.
            if !materialized_stale(&snapshot, &manifest, max_age, now) {
                continue;
            }
            let _ = fs::remove_dir_all(&snapshot);
            let _ = fs::remove_file(&manifest);
        }
    }
}

fn materialized_stale(
    snapshot: &Path,
    manifest: &Path,
    max_age: Duration,
    now: SystemTime,
) -> bool {
    [snapshot, manifest]
        .into_iter()
        .filter_map(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok())
        .max()
        .and_then(|newest| now.duration_since(newest).ok())
        .is_some_and(|age| age > max_age)
}

fn sweep_dir(path: &PathBuf, max_age: Duration, now: SystemTime) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > max_age);
        if stale {
            if metadata.is_dir() {
                let _ = fs::remove_dir_all(path);
            } else {
                let _ = fs::remove_file(path);
            }
        }
    }
}

#[cfg(test)]
mod tests;
