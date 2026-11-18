//! Startup-triggered retention sweeps of native caches.
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MARKER: &str = ".last-cache-maintenance";

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
        if last.elapsed().is_ok_and(|elapsed| elapsed < INTERVAL) {
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
    sweep_dir(&tmp.join("tree"), INTERVAL);
    sweep_dir(&tmp.join("tree-staging"), INTERVAL);
    // Materialized checkouts: each snapshot ages by its last write and is
    // removed only under its own lock, so a materialize in progress keeps
    // it.
    sweep_materialized(
        &octocode_tool_github::gh_structure::materialize::materialize_root(home),
        INTERVAL,
    );
    // Rate-limit mirrors only hold facts that expire within ~1h.
    sweep_dir(&tmp.join("ratelimit"), INTERVAL);
    // URL captures are content-addressed directories, rewritten on every
    // recapture of the same text, so their age is their last capture.
    sweep_dir(
        &octocode_tools_shared::source_capture::url::capture_root(home),
        capture_ttl,
    );
    let epoch = octocode_runtime_base::civil_date::unix_secs();
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
fn sweep_materialized(root: &Path, max_age: Duration) {
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
            if !materialized_stale(&snapshot, &manifest, max_age) {
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
            if !materialized_stale(&snapshot, &manifest, max_age) {
                continue;
            }
            let _ = fs::remove_dir_all(&snapshot);
            let _ = fs::remove_file(&manifest);
        }
    }
}

fn materialized_stale(snapshot: &Path, manifest: &Path, max_age: Duration) -> bool {
    [snapshot, manifest]
        .into_iter()
        .filter_map(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok())
        .max()
        .and_then(|newest| newest.elapsed().ok())
        .is_some_and(|age| age > max_age)
}

fn sweep_dir(path: &PathBuf, max_age: Duration) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let now = SystemTime::now();
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
