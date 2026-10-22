//! Shared 24-hour sweep of native tmp caches.
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const MARKER: &str = ".last-cache-maintenance";

/// Sweeps only an existing `<home>/tmp`: when no cache dir was ever created
/// (e.g. `storage.mode=memory` on a fresh home) there is nothing to sweep and
/// nothing is written. `capture_ttl` is the URL capture retention, the same
/// value each capture publishes as its `expiresAt`.
pub fn run_if_due(home: &Path, capture_ttl: Duration) -> bool {
    let tmp = home.join("tmp");
    if !tmp.is_dir() {
        return false;
    }
    let marker = tmp.join(MARKER);
    if let Ok(text) = fs::read_to_string(&marker)
        && let Ok(epoch) = text.trim().parse::<u64>()
    {
        let last = SystemTime::UNIX_EPOCH + Duration::from_secs(epoch);
        if last.elapsed().is_ok_and(|elapsed| elapsed < INTERVAL) {
            return false;
        }
    }
    // Clone entries may contain local edits. Their lock/status-aware eviction
    // runs through ghCloneRepo; a directory-age sweep cannot safely remove them.
    // The response and registry caches (`tmp/response`, `tmp/artifact`) age
    // by entry class and byte budget in their own store, which alone evicts
    // them: an age sweep would drop entries the store keeps.
    // `tmp/tree` and `tmp/tree-staging` are written by octocode-mcp 19.x,
    // which shares this home; native never reads them, so aged entries are
    // cleared here.
    sweep_dir(&tmp.join("tree"), INTERVAL);
    sweep_dir(&tmp.join("tree-staging"), INTERVAL);
    // Materialized checkouts: each snapshot ages by its last write and is
    // removed only under its own lock, so a materialize in progress keeps
    // it. `v2` is a retired layout no writer uses: it ages out whole.
    sweep_materialized(
        &crate::tools::gh_structure::materialize::materialize_root(home),
        INTERVAL,
    );
    sweep_dir(&tmp.join("materialize").join("v2"), INTERVAL);
    // Rate-limit mirrors only hold facts that expire within ~1h.
    sweep_dir(&tmp.join("ratelimit"), INTERVAL);
    // URL captures are content-addressed directories, rewritten on every
    // recapture of the same text, so their age is their last capture.
    sweep_dir(
        &crate::tools::source_capture::url::capture_root(home),
        capture_ttl,
    );
    let epoch = crate::civil_date::unix_secs();
    let _ = fs::write(marker, epoch.to_string());
    true
}

/// Removes each materialized snapshot (`<root>/<owner>/<repo>/<commit>`
/// with its `<commit>.manifest.json`) whose newest write is older than
/// `max_age`, holding its `<commit>.lock` as a materialize does: a snapshot
/// whose lock is held is in use and stays. The lock file stays too, so a
/// writer waiting on it never locks a file another writer recreated.
fn sweep_materialized(root: &Path, max_age: Duration) {
    let now = SystemTime::now();
    let modified = |path: &Path| fs::metadata(path).and_then(|meta| meta.modified()).ok();
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
            let newest = [modified(&snapshot), modified(&manifest)]
                .into_iter()
                .flatten()
                .max();
            let stale = newest
                .and_then(|newest| now.duration_since(newest).ok())
                .is_some_and(|age| age > max_age);
            if !stale {
                continue;
            }
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
            let _ = fs::remove_dir_all(&snapshot);
            let _ = fs::remove_file(&manifest);
        }
    }
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
mod tests {
    use super::*;

    /// Materialized snapshots of the current layout age out by their last
    /// write; one whose lock is held (a materialize in progress) stays, and
    /// so does a fresh one.
    #[test]
    fn stale_materialized_snapshots_are_swept_unless_locked() {
        let home = tempfile::tempdir().unwrap();
        let repo = crate::tools::gh_structure::materialize::materialize_root(home.path())
            .join("owner/repo");
        let old = SystemTime::now() - INTERVAL * 2;
        let age = |path: &Path| {
            fs::File::open(path)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(old))
                .unwrap();
        };
        for commit in ["stale", "busy", "fresh"] {
            let dir = repo.join(commit);
            fs::create_dir_all(dir.join("src")).unwrap();
            fs::write(dir.join("src/lib.rs"), "x").unwrap();
            let manifest = repo.join(format!("{commit}.manifest.json"));
            fs::write(&manifest, "{}").unwrap();
            if commit != "fresh" {
                age(&dir);
                age(&manifest);
            }
        }
        let held = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(repo.join("busy.lock"))
            .unwrap();
        let _lease = octocode_io::lock::lock_within(held, Duration::ZERO, Duration::ZERO)
            .expect("held lock");
        assert!(run_if_due(home.path(), INTERVAL));
        assert!(!repo.join("stale").exists());
        assert!(!repo.join("stale.manifest.json").exists());
        assert!(
            repo.join("busy/src/lib.rs").exists(),
            "a held snapshot stays"
        );
        assert!(repo.join("fresh/src/lib.rs").exists());
    }

    #[test]
    fn fresh_home_without_tmp_writes_nothing() {
        let home = tempfile::tempdir().unwrap();
        assert!(!run_if_due(home.path(), INTERVAL));
        assert!(!home.path().join("tmp").exists());
    }

    #[test]
    fn automatic_maintenance_preserves_clone_evidence_under_an_old_owner_directory() {
        let home = tempfile::tempdir().unwrap();
        let owner = home.path().join("tmp/clone/owner");
        let checkout = owner.join("repo/main");
        fs::create_dir_all(&checkout).unwrap();
        let evidence = checkout.join("local-notes.txt");
        fs::write(&evidence, "uncommitted evidence").unwrap();
        let old = SystemTime::now() - INTERVAL * 2;
        fs::File::open(&owner)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(old))
            .unwrap();
        assert!(run_if_due(home.path(), INTERVAL));
        assert_eq!(
            fs::read_to_string(evidence).unwrap(),
            "uncommitted evidence"
        );
    }

    /// Response and registry caches age by their own entry class and byte
    /// budget (`cache::Disk::prune`); the daily sweep never removes an
    /// entry the store keeps, however old its file.
    #[test]
    fn maintenance_leaves_store_owned_cache_entries_to_the_store() {
        let home = tempfile::tempdir().unwrap();
        let old = SystemTime::now() - INTERVAL * 2;
        let mut kept = Vec::new();
        for dir in ["response", "artifact"] {
            let dir = home.path().join("tmp").join(dir);
            fs::create_dir_all(&dir).unwrap();
            let pinned = dir.join("pinned.json");
            fs::write(&pinned, "{}").unwrap();
            fs::File::options()
                .write(true)
                .open(&pinned)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(old))
                .unwrap();
            kept.push(pinned);
        }
        let stale: Vec<_> = ["tmp/tree/old", "tmp/tree-staging/old"]
            .into_iter()
            .map(|dir| home.path().join(dir))
            .collect();
        for dir in &stale {
            fs::create_dir_all(dir).unwrap();
            fs::File::open(dir)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(old))
                .unwrap();
        }
        assert!(run_if_due(home.path(), INTERVAL));
        for pinned in kept {
            assert!(pinned.exists(), "{}", pinned.display());
        }
        for dir in stale {
            assert!(!dir.exists(), "{} still ages out", dir.display());
        }
    }

    fn age(path: &Path, by: Duration) {
        fs::File::open(path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(SystemTime::now() - by))
            .unwrap();
    }

    /// A URL capture outlives its published `expiresAt` by at most one sweep
    /// interval; a capture still inside its retention is kept.
    #[test]
    fn url_captures_past_their_retention_are_removed() {
        let home = tempfile::tempdir().unwrap();
        let retention = Duration::from_secs(60 * 60);
        let root = crate::tools::source_capture::url::capture_root(home.path());
        let expired = root.join("expired");
        let live = root.join("live");
        for capture in [&expired, &live] {
            fs::create_dir_all(capture).unwrap();
            fs::write(capture.join("source.txt"), "captured text").unwrap();
        }
        age(&expired, retention * 2);
        age(&live, retention / 2);
        assert!(run_if_due(home.path(), retention));
        assert!(!expired.exists(), "an expired capture is removed");
        assert_eq!(
            fs::read_to_string(live.join("source.txt")).unwrap(),
            "captured text"
        );
    }

    #[test]
    fn existing_tmp_is_swept_once_and_writes_only_the_marker() {
        let home = tempfile::tempdir().unwrap();
        let tmp = home.path().join("tmp");
        fs::create_dir_all(&tmp).unwrap();
        assert!(run_if_due(home.path(), INTERVAL));
        assert!(tmp.join(MARKER).is_file());
        assert!(!tmp.join("session-stats.json").exists());
        assert!(!run_if_due(home.path(), INTERVAL));
    }
}
