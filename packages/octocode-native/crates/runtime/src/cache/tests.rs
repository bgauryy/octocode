use super::*;

fn key(resource: &str, credential: &str) -> CacheKey {
    CacheKey {
        namespace: "test".into(),
        resource: resource.into(),
        partition: CachePartition {
            endpoint: "https://api.github.com".into(),
            credential_fingerprint: credential.into(),
        },
    }
}

fn config(fresh: u64, ttl: u64) -> CacheConfig {
    CacheConfig {
        fresh: Duration::from_secs(fresh),
        ttl: Duration::from_secs(ttl),
        ..CacheConfig::default()
    }
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect()
}

fn age_disk_entries(dir: &Path, seconds: u64) {
    for file in json_files(dir) {
        let mut entry: serde_json::Value =
            serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        entry["storedAtUnix"] = serde_json::json!(now_unix() - seconds);
        fs::write(&file, serde_json::to_vec(&entry).unwrap()).unwrap();
    }
}

#[test]
fn classes_age_differently_in_memory() {
    let store = Store::<u32>::new(config(0, 0), None);
    store.put(key("pinned", "x"), 1, 4, CacheClass::Immutable);
    store.put(key("listing", "x"), 2, 4, CacheClass::Revalidate);
    store.put(key("search", "x"), 3, 4, CacheClass::Volatile);
    let pinned = store
        .get(&key("pinned", "x"))
        .expect("immutable never expires");
    assert!(pinned.fresh);
    let listing = store
        .get(&key("listing", "x"))
        .expect("revalidate entries are kept past the fresh window");
    assert!(
        !listing.fresh,
        "past the fresh window the caller revalidates"
    );
    assert!(store.get(&key("search", "x")).is_none(), "volatile expires");
}

#[test]
fn revalidate_entries_are_fresh_inside_the_window() {
    let store = Store::<u32>::new(config(60, 60), None);
    store.put(key("listing", "x"), 2, 4, CacheClass::Revalidate);
    store.put(key("search", "x"), 3, 4, CacheClass::Volatile);
    assert!(store.get(&key("listing", "x")).unwrap().fresh);
    assert!(store.get(&key("search", "x")).unwrap().fresh);
}

#[test]
fn memory_is_a_byte_and_entry_bounded_lru() {
    let store = Store::<u32>::new(
        CacheConfig {
            max_entries: 2,
            max_bytes: 5,
            ..config(60, 60)
        },
        None,
    );
    store.put(key("a", "x"), 1, 2, CacheClass::Immutable);
    store.put(key("b", "x"), 2, 2, CacheClass::Immutable);
    assert!(store.get(&key("a", "x")).is_some());
    store.put(key("c", "x"), 3, 2, CacheClass::Immutable);
    assert!(
        store.get(&key("b", "x")).is_none(),
        "least recently used goes"
    );
    assert!(store.get(&key("a", "x")).is_some());
    assert!(store.get(&key("c", "x")).is_some());
    store.put(key("big", "x"), 4, 6, CacheClass::Immutable);
    assert!(
        store.get(&key("big", "x")).is_none(),
        "over budget is refused"
    );
}

#[test]
fn zero_entries_disables_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<u32>::new(
        CacheConfig {
            max_entries: 0,
            ..CacheConfig::default()
        },
        Some(dir.path().into()),
    );
    store.put(key("a", "x"), 1, 4, CacheClass::Immutable);
    assert!(store.get(&key("a", "x")).is_none());
    assert!(json_files(dir.path()).is_empty());
}

#[test]
fn partitions_never_share_entries() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<u32>::new(CacheConfig::default(), Some(dir.path().into()));
    store.put(key("a", "first"), 1, 4, CacheClass::Immutable);
    let fresh = Store::<u32>::new(CacheConfig::default(), Some(dir.path().into()));
    assert_eq!(*fresh.get(&key("a", "first")).unwrap().value, 1);
    assert!(fresh.get(&key("a", "second")).is_none());
    assert!(store.get(&key("a", "second")).is_none());
}

#[test]
fn disk_tier_keeps_immutable_entries_and_expires_volatile_ones() {
    let dir = tempfile::tempdir().unwrap();
    let writer = Store::<String>::new(config(60, 60), Some(dir.path().into()));
    writer.put(key("pinned", "x"), "body".into(), 4, CacheClass::Immutable);
    writer.put(
        key("listing", "x"),
        "list".into(),
        4,
        CacheClass::Revalidate,
    );
    writer.put(key("search", "x"), "page".into(), 4, CacheClass::Volatile);
    // A day later, in a new process.
    age_disk_entries(dir.path(), 86_400);
    let reader = Store::<String>::new(config(60, 60), Some(dir.path().into()));
    let pinned = reader.get(&key("pinned", "x")).expect("immutable survives");
    assert!(pinned.fresh);
    assert_eq!(*pinned.value, "body");
    let listing = reader
        .get(&key("listing", "x"))
        .expect("kept for revalidation");
    assert!(!listing.fresh);
    assert!(reader.get(&key("search", "x")).is_none());
    assert_eq!(
        json_files(dir.path()).len(),
        2,
        "the expired file is removed"
    );
    // Promoted: later reads survive the file disappearing.
    for file in json_files(dir.path()) {
        fs::remove_file(file).unwrap();
    }
    assert!(reader.get(&key("pinned", "x")).is_some());
}

#[test]
fn disk_tier_is_byte_bounded_least_recently_used_first() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(
        CacheConfig {
            disk_bytes: 1,
            ..CacheConfig::default()
        },
        Some(dir.path().into()),
    );
    // The first write scans (here pruning everything); later writes skip the scan.
    store.put(key("old", "x"), "a".repeat(100), 100, CacheClass::Immutable);
    store.put(key("new", "x"), "b".repeat(100), 100, CacheClass::Immutable);
    assert_eq!(json_files(dir.path()).len(), 1, "pruning is amortized");
    let _ = fs::remove_file(dir.path().join(PRUNE_MARKER));
    store.disk.as_ref().unwrap().prune(300);
    assert_eq!(json_files(dir.path()).len(), 1);
    store.disk.as_ref().unwrap().prune(0);
    assert!(json_files(dir.path()).is_empty());
}

#[test]
fn clear_removes_both_tiers_and_corrupt_files_are_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<u32>::new(CacheConfig::default(), Some(dir.path().into()));
    store.put(key("a", "x"), 1, 4, CacheClass::Immutable);
    store.clear();
    assert!(store.get(&key("a", "x")).is_none());
    store.put(key("b", "x"), 2, 4, CacheClass::Immutable);
    for file in json_files(dir.path()) {
        fs::write(&file, b"corrupt").unwrap();
    }
    let reader = Store::<u32>::new(CacheConfig::default(), Some(dir.path().into()));
    assert!(reader.get(&key("b", "x")).is_none());
    assert!(json_files(dir.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn disk_entries_are_owner_only_and_oversized_values_skip_disk() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let dir = root.path().join("tmp").join("response");
    let store = Store::<String>::new(
        CacheConfig {
            max_bytes: 1024,
            ..CacheConfig::default()
        },
        Some(dir.clone()),
    );
    store.put(key("a", "x"), "private".into(), 7, CacheClass::Immutable);
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&dir), 0o700);
    let files = json_files(&dir);
    assert_eq!(files.len(), 1);
    assert_eq!(mode(&files[0]), 0o600);
    store.put(
        key("large", "x"),
        "x".repeat(4096),
        4096,
        CacheClass::Immutable,
    );
    assert_eq!(json_files(&dir).len(), 1);
}

/// A disk entry the store did not write as a regular file within its entry
/// bound is a miss and is evicted: a FIFO answers at once (never blocks), an
/// oversized entry is never loaded, and a symlink is never followed.
#[cfg(unix)]
#[test]
fn disk_entries_that_are_not_bounded_regular_files_are_evicted_misses() {
    let dir = tempfile::tempdir().unwrap();
    let config = CacheConfig {
        max_bytes: 256,
        ..CacheConfig::default()
    };
    let store = std::sync::Arc::new(Store::<String>::new(
        config.clone(),
        Some(dir.path().join("store")),
    ));
    let disk = store.disk.as_ref().unwrap();
    let entry = |value: &str| {
        serde_json::to_vec(&serde_json::json!({
            "class": "immutable", "storedAtUnix": now_unix(), "value": value,
        }))
        .unwrap()
    };

    let fifo = disk.file(&key("fifo", "x"));
    let name = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader = std::sync::Arc::clone(&store);
    std::thread::spawn(move || {
        let _ = sender.send(reader.get(&key("fifo", "x")).is_none());
    });
    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(5)),
        Ok(true),
        "a FIFO entry must be an immediate miss"
    );
    assert!(fs::symlink_metadata(&fifo).is_err(), "FIFO evicted");

    let oversized = disk.file(&key("oversized", "x"));
    fs::write(&oversized, entry(&"x".repeat(1024))).unwrap();
    assert!(store.get(&key("oversized", "x")).is_none());
    assert!(!oversized.exists(), "oversized entry evicted");

    let target = dir.path().join("elsewhere.json");
    fs::write(&target, entry("planted")).unwrap();
    let link = disk.file(&key("link", "x"));
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(store.get(&key("link", "x")).is_none());
    assert!(fs::symlink_metadata(&link).is_err(), "symlink evicted");
    assert!(target.exists(), "the link target is left alone");

    let regular = disk.file(&key("regular", "x"));
    fs::write(&regular, entry("kept")).unwrap();
    assert_eq!(
        store
            .get(&key("regular", "x"))
            .map(|hit| (*hit.value).clone()),
        Some("kept".to_owned())
    );
}
