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
pub fn run_if_due(home: &Path, capture_ttl: Duration) -> bool {
    let tmp = home.join("tmp");
    if !tmp.is_dir()
        && !crate::cache::Directory::ALL
            .into_iter()
            .map(|directory| directory.path(home))
            .chain(crate::cache::Directory::retired(home))
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
    for directory in crate::cache::Directory::ALL {
        let dir = directory.path(home);
        if dir.is_dir() {
            crate::cache::Disk::new(dir).prune_expired();
        }
    }
    // Retired store directories have no reader or writer: they go whole.
    for dir in crate::cache::Directory::retired(home) {
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
        &crate::tools::gh_structure::materialize::materialize_root(home),
        INTERVAL,
    );
    // Rate-limit mirrors only hold facts that expire within ~1h.
    sweep_dir(&tmp.join("ratelimit"), INTERVAL);
    // URL captures are content-addressed directories, rewritten on every
    // recapture of the same text, so their age is their last capture.
    sweep_dir(
        &crate::tools::source_capture::url::capture_root(home),
        capture_ttl,
    );
    let epoch = crate::civil_date::unix_secs();
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
mod tests {
    use super::*;

    #[test]
    fn provider_only_home_without_tmp_prunes_expired_answers_and_records_the_sweep() {
        use crate::cache::{CacheClass, CacheConfig, CacheKey, Directory, Disk, Store};
        let home = tempfile::tempdir().unwrap();
        let dir = Directory::ProviderAnswers.path(home.path());
        let store = Store::<u32>::new(CacheConfig::derived(), Some(dir.clone()));
        let key = CacheKey {
            namespace: "maintenance".into(),
            scope: "provider-only".into(),
            resource: "expired".into(),
        };
        let class = CacheClass::Volatile { ttl: INTERVAL };
        assert!(store.put_confirmed(key.clone(), 1, 4, class).1);
        let file = Disk::new(dir).file(&key);
        age_entry(&file, INTERVAL * 2);
        assert!(!home.path().join("tmp").exists());
        assert!(run_if_due(home.path(), INTERVAL));
        assert!(!file.exists(), "the provider answer expired");
        assert!(
            !home.path().join("tmp").exists(),
            "maintenance creates no tmp"
        );
        assert!(!run_if_due(home.path(), INTERVAL), "the sweep was recorded");
    }

    #[test]
    fn a_materialized_snapshot_refreshed_before_maintenance_locks_it_is_kept() {
        let home = tempfile::tempdir().unwrap();
        let root = crate::tools::gh_structure::materialize::materialize_root(home.path());
        let repo = root.join("owner/repo");
        let snapshot = repo.join("commit");
        let manifest = repo.join("commit.manifest.json");
        fs::create_dir_all(&snapshot).unwrap();
        fs::write(&manifest, "{}").unwrap();
        age(&snapshot, INTERVAL * 2);
        age(&manifest, INTERVAL * 2);
        BEFORE_SNAPSHOT_LOCK.with(|hook| {
            hook.replace(Some(Box::new(move |snapshot, manifest| {
                let file = fs::OpenOptions::new()
                    .create(true)
                    .truncate(false)
                    .write(true)
                    .open(repo.join("commit.lock"))
                    .unwrap();
                let _writer = octocode_io::lock::lock_within(file, Duration::ZERO, Duration::ZERO)
                    .expect("writer lock");
                fs::write(snapshot.join("refreshed.txt"), "fresh evidence").unwrap();
                fs::File::open(manifest)
                    .unwrap()
                    .set_times(fs::FileTimes::new().set_modified(SystemTime::now()))
                    .unwrap();
            })));
        });
        sweep_materialized(&root, INTERVAL);
        assert_eq!(
            fs::read_to_string(snapshot.join("refreshed.txt")).unwrap(),
            "fresh evidence"
        );
        assert!(manifest.is_file(), "a writer refreshed this manifest");
        BEFORE_SNAPSHOT_LOCK.with(|hook| assert!(hook.borrow().is_none()));
    }

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

    /// A store entry `age` past its save and its last use: its header's save
    /// time and its file's modification time both moved back.
    fn age_entry(file: &Path, age: Duration) {
        let bytes = fs::read(file).unwrap();
        let split = bytes.iter().position(|byte| *byte == b'\n').unwrap();
        let mut header: serde_json::Value = serde_json::from_slice(&bytes[..split]).unwrap();
        header["storedAtUnix"] =
            serde_json::json!(crate::civil_date::unix_secs().unsigned_abs() - age.as_secs());
        let mut aged = serde_json::to_vec(&header).unwrap();
        aged.extend_from_slice(&bytes[split..]);
        fs::write(file, aged).unwrap();
        self::age(file, age);
    }

    /// Every store directory's expired entries go with the daily sweep, by
    /// the class each entry was saved under: an expired section plan's text
    /// does not wait on disk for the next write to that store (CLS-C30).
    /// A live entry stays, and so does an immutable one however old its
    /// file: only the store's byte budget evicts it.
    #[test]
    fn maintenance_prunes_expired_store_entries_and_keeps_live_ones() {
        use crate::cache::{CacheClass, CacheConfig, CacheKey, Directory, Disk, Store};
        use crate::tools::clasify::sections::PLAN_RETENTION;
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join("tmp")).unwrap();
        let key = |resource: &str| CacheKey {
            namespace: "maintenance".into(),
            scope: "policy".into(),
            resource: resource.into(),
        };
        let ttl = PLAN_RETENTION.ttl().expect("a plan expires");
        let mut expired = Vec::new();
        let mut kept = Vec::new();
        for directory in Directory::ALL {
            let dir = directory.path(home.path());
            let store = Store::<u32>::new(CacheConfig::derived(), Some(dir.clone()));
            let disk = Disk::new(dir);
            for (name, class) in [
                ("expired", PLAN_RETENTION),
                ("live", PLAN_RETENTION),
                ("immutable", CacheClass::Immutable),
            ] {
                assert!(store.put_confirmed(key(name), 1, 4, class).1);
                let file = disk.file(&key(name));
                if name == "live" {
                    kept.push(file);
                } else {
                    age_entry(&file, ttl + INTERVAL);
                    if name == "expired" {
                        expired.push(file);
                    } else {
                        kept.push(file);
                    }
                }
            }
        }
        assert!(run_if_due(home.path(), INTERVAL));
        for file in expired {
            assert!(!file.exists(), "{} is past its lifetime", file.display());
        }
        for file in kept {
            assert!(file.exists(), "{} is kept", file.display());
        }
    }

    /// A directory a layout bump retired goes with the daily sweep; a live
    /// store directory stays.
    #[test]
    fn maintenance_removes_retired_directories_and_keeps_live_ones() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join("tmp")).unwrap();
        let retired: Vec<_> = crate::cache::Directory::retired(home.path()).collect();
        let live: Vec<_> = crate::cache::Directory::ALL
            .into_iter()
            .map(|directory| directory.path(home.path()))
            .collect();
        for dir in retired.iter().chain(&live) {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join("entry"), "x").unwrap();
        }
        assert!(run_if_due(home.path(), INTERVAL));
        for dir in &retired {
            assert!(!dir.exists(), "{} is retired", dir.display());
        }
        for dir in &live {
            assert!(dir.join("entry").exists(), "{} is live", dir.display());
        }
    }

    /// The sweep never creates a store directory that no write made.
    #[test]
    fn maintenance_creates_no_store_directory() {
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join("tmp")).unwrap();
        assert!(run_if_due(home.path(), INTERVAL));
        for directory in crate::cache::Directory::ALL {
            assert!(!directory.path(home.path()).exists());
        }
    }

    /// Clone checkouts and the trees an older server wrote keep their own
    /// rules: tree directories still age out by their modification time.
    #[test]
    fn maintenance_still_ages_out_old_tree_directories() {
        let home = tempfile::tempdir().unwrap();
        let old = SystemTime::now() - INTERVAL * 2;
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

    /// A due startup sweep removes URL captures past their published
    /// retention; a capture still inside its retention is kept.
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
    fn an_unwritable_marker_is_not_a_recorded_sweep() {
        let home = tempfile::tempdir().unwrap();
        // A directory where the marker file belongs makes its write fail.
        fs::create_dir_all(home.path().join("tmp")).unwrap();
        fs::create_dir(home.path().join(MARKER)).unwrap();
        assert!(!run_if_due(home.path(), INTERVAL));
    }

    #[test]
    fn existing_tmp_is_swept_once_and_writes_only_the_marker() {
        let home = tempfile::tempdir().unwrap();
        let tmp = home.path().join("tmp");
        fs::create_dir_all(&tmp).unwrap();
        assert!(run_if_due(home.path(), INTERVAL));
        assert!(home.path().join(MARKER).is_file());
        assert!(!tmp.join("session-stats.json").exists());
        assert!(!run_if_due(home.path(), INTERVAL));
    }
}
