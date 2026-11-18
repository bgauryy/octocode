use super::*;

#[test]
fn provider_only_home_without_tmp_prunes_expired_answers_and_records_the_sweep() {
    use octocode_runtime_base::cache::{CacheClass, CacheConfig, CacheKey, Directory, Disk, Store};
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
    let root = octocode_tool_github::gh_structure::materialize::materialize_root(home.path());
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
    let repo = octocode_tool_github::gh_structure::materialize::materialize_root(home.path())
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
    let _lease =
        octocode_io::lock::lock_within(held, Duration::ZERO, Duration::ZERO).expect("held lock");
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
    header["storedAtUnix"] = serde_json::json!(
        octocode_runtime_base::civil_date::unix_secs().unsigned_abs() - age.as_secs()
    );
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
    use octocode_runtime_base::cache::{CacheClass, CacheConfig, CacheKey, Directory, Disk, Store};
    use octocode_tool_clasify::clasify::sections::PLAN_RETENTION;
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

#[test]
fn response_cache_controls_retire_answers_with_lost_provider_headers() {
    use octocode_github::{ProviderAnswer, SavedAnswer, SavedAnswers};
    use octocode_runtime_base::cache::Directory;
    use octocode_runtime_base::providers::saved_answers::KeptAnswers;

    let home = tempfile::tempdir().unwrap();
    // Historical layout omitted Vary in registry answers and every OSV header.
    let legacy = home.path().join("cache/provider-answers-v2");
    let current = Directory::ProviderAnswers.path(home.path());
    let saved = SavedAnswer {
        answer: ProviderAnswer {
            request: "OSV record example@stamp".into(),
            status: 200,
            headers: Vec::new(),
            body: "{\"id\":\"example\",\"modified\":\"stamp\"}".into(),
            next: None,
            not_found: None,
        },
        observed_ms: u64::try_from(octocode_runtime_base::civil_date::unix_millis()).unwrap(),
        redirected: false,
    };
    let request = saved.answer.request.clone();
    KeptAnswers::new(Some(legacy.clone())).save("origin", &request, saved.clone());
    assert!(
        KeptAnswers::new(Some(legacy.clone()))
            .load("origin", &request)
            .is_some()
    );
    let store = KeptAnswers::new(Some(current.clone()));
    assert!(
        store.load("origin", &request).is_none(),
        "lost metadata cannot authorize reuse"
    );
    store.save("origin", &request, saved.clone());
    assert!(run_if_due(home.path(), INTERVAL));
    assert!(!legacy.exists(), "the historical provider store is retired");
    assert_eq!(
        KeptAnswers::new(Some(current)).load("origin", &request),
        Some(saved)
    );
}

/// A directory a layout bump retired goes with the daily sweep; a live
/// store directory stays.
#[test]
fn maintenance_removes_retired_directories_and_keeps_live_ones() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join("tmp")).unwrap();
    let retired: Vec<_> = octocode_runtime_base::cache::Directory::retired(home.path()).collect();
    let live: Vec<_> = octocode_runtime_base::cache::Directory::ALL
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
    for directory in octocode_runtime_base::cache::Directory::ALL {
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
    let root = octocode_tools_shared::source_capture::url::capture_root(home.path());
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
