use super::*;
use crate::cache::disk::PRUNE_MARKER;
#[cfg(unix)]
use crate::cache::disk::{DiskHeader, now_unix};

#[test]
fn disk_tier_keeps_immutable_entries_and_expires_volatile_ones() {
    let dir = tempfile::tempdir().unwrap();
    let writer = Store::<String>::new(CacheConfig::default(), Some(dir.path().into()));
    writer.put(key("pinned", "x"), "body".into(), 4, CacheClass::Immutable);
    writer.put(key("listing", "x"), "list".into(), 4, revalidate(60));
    writer.put(key("search", "x"), "page".into(), 4, volatile(60));
    // A day later, in a new process.
    age_disk_entries(dir.path(), 86_400);
    let reader = Store::<String>::new(CacheConfig::default(), Some(dir.path().into()));
    let pinned = reader.get(&key("pinned", "x")).expect("immutable survives");
    assert!(pinned.fresh);
    assert_eq!(*pinned.value, "body");
    let listing = reader
        .get(&key("listing", "x"))
        .expect("kept for revalidation");
    assert!(!listing.fresh);
    assert!(reader.get(&key("search", "x")).is_none());
    assert_eq!(
        entry_files(dir.path()).len(),
        2,
        "the expired file is removed"
    );
    // Promoted: later reads survive the file disappearing.
    for file in entry_files(dir.path()) {
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
    assert_eq!(entry_files(dir.path()).len(), 1, "pruning is amortized");
    let _ = fs::remove_file(dir.path().join(PRUNE_MARKER));
    store.disk.as_ref().unwrap().prune(300);
    assert_eq!(entry_files(dir.path()).len(), 1);
    store.disk.as_ref().unwrap().prune(0);
    assert!(entry_files(dir.path()).is_empty());
}

/// A crash between the temp write and the rename leaves `.tmp` files; prune
/// sweeps old ones and never touches a write that may still be in flight.
#[test]
fn prune_sweeps_orphaned_temporary_files_but_not_fresh_ones() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<u32>::new(CacheConfig::default(), Some(dir.path().into()));
    let orphan = dir.path().join(".abc.entry-0011.tmp");
    let fresh = dir.path().join(".def.entry-0022.tmp");
    fs::write(&orphan, b"partial").unwrap();
    fs::write(&fresh, b"in flight").unwrap();
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3_600);
    fs::File::options()
        .write(true)
        .open(&orphan)
        .unwrap()
        .set_modified(old)
        .unwrap();
    store.disk.as_ref().unwrap().prune(u64::MAX);
    assert!(
        !orphan.exists(),
        "an hour-old temp file is a crash leftover"
    );
    assert!(fresh.exists(), "a fresh temp file may be a write in flight");
}

#[test]
fn corrupt_files_are_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<u32>::new(CacheConfig::default(), Some(dir.path().into()));
    store.put(key("b", "x"), 2, 4, CacheClass::Immutable);
    for file in entry_files(dir.path()) {
        fs::write(&file, b"corrupt").unwrap();
    }
    let reader = Store::<u32>::new(CacheConfig::default(), Some(dir.path().into()));
    assert!(reader.get(&key("b", "x")).is_none());
    assert!(entry_files(dir.path()).is_empty());
}

/// A read that fails for an environmental reason (no permission, too many
/// open files) is a miss, not a reason to delete a healthy entry.
#[cfg(unix)]
#[test]
fn a_transient_read_failure_keeps_the_entry() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<u32>::new(CacheConfig::default(), Some(dir.path().into()));
    store.put(key("b", "x"), 2, 4, CacheClass::Immutable);
    let file = entry_files(dir.path()).pop().unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
    let reader = Store::<u32>::new(CacheConfig::default(), Some(dir.path().into()));
    assert!(
        reader.get(&key("b", "x")).is_none(),
        "an unreadable entry is a miss"
    );
    assert!(
        file.exists(),
        "a healthy entry survives a transient failure"
    );
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let healed = Store::<u32>::new(CacheConfig::default(), Some(dir.path().into()));
    assert!(
        healed.get(&key("b", "x")).is_some(),
        "the entry serves again once readable"
    );
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
    let files = entry_files(&dir);
    assert_eq!(files.len(), 1);
    assert_eq!(mode(&files[0]), 0o600);
    store.put(
        key("large", "x"),
        "x".repeat(4096),
        4096,
        CacheClass::Immutable,
    );
    assert_eq!(entry_files(&dir).len(), 1);
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
        let mut bytes = serde_json::to_vec(&DiskHeader {
            class: CacheClass::Immutable,
            stored_at_unix: now_unix(),
        })
        .unwrap();
        bytes.push(b'\n');
        bytes.extend_from_slice(&serde_json::to_vec(value).unwrap());
        bytes
    };

    let fifo = disk.file(&key("fifo", "x"));
    crate::tools::test_support::make_fifo(&fifo);
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

    let target = dir.path().join("elsewhere.entry");
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

/// A prune deletes entries past their TTL first, whatever the budget: an
/// expired `Volatile` entry never waits for the byte cap. It reads only each
/// entry's header; live and immutable entries stay.
#[test]
fn prune_removes_expired_entries_before_any_live_one() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::default(), Some(dir.path().into()));
    store.put(key("short", "x"), "x".repeat(4096), 4096, volatile(60));
    age_disk_entries(dir.path(), 120);
    store.put(key("pinned", "x"), "p".into(), 1, CacheClass::Immutable);
    store.put(key("live", "x"), "l".into(), 1, volatile(600));
    store.disk.as_ref().unwrap().prune(u64::MAX);
    assert_eq!(
        entry_files(dir.path()).len(),
        2,
        "only the expired entry goes"
    );
    let reader = Store::<String>::new(CacheConfig::default(), Some(dir.path().into()));
    assert!(reader.get(&key("pinned", "x")).is_some());
    assert!(reader.get(&key("live", "x")).is_some());
    assert!(reader.get(&key("short", "x")).is_none());
}

#[test]
fn confirmed_retention_requires_capacity_and_every_configured_tier() {
    let config = CacheConfig {
        max_entries: 2,
        max_bytes: 1024,
        disk_bytes: 4096,
    };
    let memory = Store::<String>::new(config.clone(), None);
    assert!(
        memory
            .put_confirmed(key("fits", "x"), "value".into(), 5, volatile(120))
            .1
    );
    assert!(
        !memory
            .put_confirmed(key("oversize", "x"), "large".into(), 1025, volatile(120))
            .1
    );
    assert!(memory.get(&key("oversize", "x")).is_none());
    let root = tempfile::tempdir().unwrap();
    let blocked = root.path().join("blocked");
    fs::write(&blocked, b"a file cannot hold cache entries").unwrap();
    let failed_disk = Store::<String>::new(config.clone(), Some(blocked));
    assert!(
        !failed_disk
            .put_confirmed(key("disk-failure", "x"), "value".into(), 5, volatile(120))
            .1
    );
    let dir = root.path().join("working");
    let disk = Store::<String>::new(config.clone(), Some(dir.clone()));
    assert!(
        disk.put_confirmed(key("saved", "x"), "value".to_owned(), 5, volatile(120))
            .1
    );
    let reopened = Store::<String>::new(config, Some(dir.clone()));
    assert_eq!(
        reopened.get(&key("saved", "x")).unwrap().value.as_str(),
        "value"
    );
    age_disk_entries(&dir, 121);
    reopened.clear_memory();
    assert!(reopened.get(&key("saved", "x")).is_none());
}
