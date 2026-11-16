use super::*;

fn bytes(dir: &Path) -> u64 {
    if !dir.exists() {
        return 0;
    }
    entry_files(dir)
        .iter()
        .map(|path| fs::metadata(path).unwrap().len())
        .sum()
}

#[test]
fn every_disk_write_obeys_the_byte_budget_across_store_instances() {
    let dir = tempfile::tempdir().unwrap();
    let config = CacheConfig { disk_bytes: 1024 };
    let stores: Vec<_> = (0..3)
        .map(|_| Store::<String>::new(config.clone(), Some(dir.path().into())))
        .collect();
    for index in 0..100 {
        let (_, saved) = stores[index % stores.len()].put_confirmed(
            key(&format!("{index}"), "scope"),
            "x".repeat(100),
            100,
            CacheClass::Immutable,
        );
        assert!(saved, "a fitting entry is admitted");
        assert!(
            bytes(dir.path()) <= config.disk_bytes,
            "write {index} exceeded the disk budget"
        );
    }
}

#[test]
fn entries_larger_than_the_disk_budget_are_never_confirmed() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig { disk_bytes: 1 }, Some(dir.path().into()));
    for index in 0..4 {
        let (_, saved) = store.put_confirmed(
            key(&format!("{index}"), "scope"),
            "value".into(),
            5,
            CacheClass::Immutable,
        );
        assert!(!saved);
        assert_eq!(bytes(dir.path()), 0);
    }
}

#[test]
fn missing_or_corrupt_accounting_is_rebuilt_before_admission() {
    for corrupt in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let config = CacheConfig { disk_bytes: 1024 };
        let store = Store::<String>::new(config.clone(), Some(dir.path().into()));
        for i in 0..6 {
            assert!(
                store
                    .put_confirmed(
                        key(&i.to_string(), "scope"),
                        "x".repeat(100),
                        100,
                        CacheClass::Immutable
                    )
                    .1
            );
        }
        let counter = dir.path().join(".capacity");
        if corrupt {
            fs::write(counter, b"incomplete").unwrap();
        } else {
            fs::remove_file(counter).unwrap();
        }
        let reopened = Store::<String>::new(config.clone(), Some(dir.path().into()));
        assert!(
            reopened
                .put_confirmed(
                    key("new", "scope"),
                    "n".repeat(100),
                    100,
                    CacheClass::Immutable
                )
                .1
        );
        assert!(bytes(dir.path()) <= config.disk_bytes);
        assert_eq!(
            reopened.get(&key("new", "scope")).unwrap().value.as_str(),
            "n".repeat(100)
        );
    }
}

#[test]
fn replacements_and_deleted_entries_do_not_leak_capacity() {
    let dir = tempfile::tempdir().unwrap();
    let config = CacheConfig { disk_bytes: 1024 };
    let store = Store::<String>::new(config.clone(), Some(dir.path().into()));
    for i in 0..100 {
        let value = "x".repeat(if i % 2 == 0 { 500 } else { 100 });
        assert!(
            store
                .put_confirmed(
                    key("same", "scope"),
                    value.clone(),
                    value.len(),
                    CacheClass::Immutable
                )
                .1
        );
        assert_eq!(
            store.get(&key("same", "scope")).unwrap().value.as_str(),
            value
        );
        assert!(bytes(dir.path()) <= config.disk_bytes);
        if i % 3 == 0 {
            store.remove(&key("same", "scope"));
        }
    }
}

#[test]
fn an_abandoned_reservation_is_reconciled_under_pressure() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join(".capacity"),
        serde_json::to_vec(&serde_json::json!({
            "used": u64::MAX, "scanned_at": crate::cache::disk::now_unix()
        }))
        .unwrap(),
    )
    .unwrap();
    let store = Store::<String>::new(CacheConfig { disk_bytes: 1024 }, Some(dir.path().into()));
    assert!(
        store
            .put_confirmed(
                key("new", "scope"),
                "value".into(),
                5,
                CacheClass::Immutable
            )
            .1
    );
    assert_eq!(entry_files(dir.path()).len(), 1);
}

#[test]
#[ignore = "helper process for independent_processes_share_one_disk_budget"]
fn disk_writer_process() {
    let dir = PathBuf::from(std::env::var_os("OCTOCODE_CACHE_TEST_DIR").unwrap());
    let writer = std::env::var("OCTOCODE_CACHE_TEST_WRITER").unwrap();
    let store = Store::<String>::new(CacheConfig { disk_bytes: 2048 }, Some(dir));
    // A reuse write may be refused while another process holds the store's
    // lock past `REUSE_LOCK_WAIT` (a loaded machine), and says so; the
    // budget must hold either way. Each retained write is reported.
    let retained = (0..40)
        .filter(|i| {
            store
                .put_confirmed(
                    key(&format!("{writer}-{i}"), "scope"),
                    "x".repeat(100),
                    100,
                    CacheClass::Immutable,
                )
                .1
        })
        .count();
    println!("retained={retained}");
}

#[test]
fn independent_processes_share_one_disk_budget() {
    use std::process::{Command, Stdio};
    let dir = tempfile::tempdir().unwrap();
    let children: Vec<_> = (0..4)
        .map(|writer| {
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "--nocapture",
                    "cache::tests::capacity::disk_writer_process",
                ])
                .env("OCTOCODE_CACHE_TEST_DIR", dir.path())
                .env("OCTOCODE_CACHE_TEST_WRITER", writer.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    // Each writer ends on its own: every lock wait it makes is bounded by
    // `REUSE_LOCK_WAIT`, so no deadline is set here.
    let mut retained = 0;
    for child in children {
        let output = child.wait_with_output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{}: {stdout}{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        retained += stdout
            .lines()
            .find_map(|line| line.strip_prefix("retained="))
            .and_then(|count| count.parse::<usize>().ok())
            .unwrap_or_else(|| panic!("no retained count: {stdout}"));
    }
    // A write is refused only while another holds the lock, and that one is
    // retained, so the writers retain at least one entry between them.
    assert!(retained >= 1, "no writer retained an entry");
    assert!((1..=2048).contains(&bytes(dir.path())));
}

#[test]
fn immediately_expired_work_is_never_confirmed_in_either_storage_mode() {
    let dir = tempfile::tempdir().unwrap();
    for disk in [None, Some(dir.path().join("store"))] {
        let store = Store::<String>::new(CacheConfig::derived(), disk.clone());
        let entry = key("expired", "scope");
        let (_, retained) = store.put_confirmed(entry.clone(), "value".into(), 5, volatile(0));
        assert!(
            !retained,
            "immediately expired work cannot back a continuation"
        );
        assert!(store.get(&entry).is_none());
        if let Some(disk) = disk {
            assert!(!disk.exists());
        }
    }
}

/// A repeat holds a link already counted: it restarts the link's lifetime and
/// leaves the capacity file as it was, so a call that retains many bodies again
/// rewrites nothing.
#[test]
fn linking_again_rewrites_no_capacity() {
    use std::os::unix::fs::MetadataExt as _;
    let (answers, retained) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let config = CacheConfig { disk_bytes: 1024 };
    let source = Store::<String>::new(config.clone(), Some(answers.path().into()));
    let store = Store::<String>::new(config, Some(retained.path().into()));
    let body = key("body", "scope");
    source.put_confirmed(body.clone(), "value".into(), 5, CacheClass::Immutable);
    let disk = Disk::new(answers.path().into());
    assert!(store.link(&disk, &body));
    let capacity = retained.path().join(".capacity");
    let before = fs::metadata(&capacity).unwrap().ino();
    assert!(store.link(&disk, &body));
    assert_eq!(fs::metadata(&capacity).unwrap().ino(), before);
    assert!(store.read_link(&body).is_some());
}

/// A link gone since (its idle drop, an eviction) is linked and counted anew.
#[test]
fn a_link_gone_is_linked_again() {
    use crate::cache::disk::LINK_EXTENSION;
    let (answers, retained) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let config = CacheConfig { disk_bytes: 1024 };
    let source = Store::<String>::new(config.clone(), Some(answers.path().into()));
    let store = Store::<String>::new(config, Some(retained.path().into()));
    let body = key("body", "scope");
    source.put_confirmed(body.clone(), "value".into(), 5, CacheClass::Immutable);
    let disk = Disk::new(answers.path().into());
    assert!(store.link(&disk, &body));
    for entry in fs::read_dir(retained.path()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|ext| ext.to_str()) == Some(LINK_EXTENSION) {
            fs::remove_file(path).unwrap();
        }
    }
    assert_eq!(store.read_link(&body), None);
    assert!(store.link(&disk, &body));
    assert!(store.read_link(&body).is_some());
}

/// One due sweep clears a stale backlog once, then subsequent writes reuse
/// accounting rather than repeatedly walking the same live directory prefix.
#[test]
fn a_due_write_finishes_the_expired_backlog_in_one_scan() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    for index in 0..128 {
        store.put_confirmed(
            key(&format!("old{index}"), "x"),
            "v".into(),
            1,
            volatile(60),
        );
    }
    age_disk_entries(dir.path(), 120);
    store.put_confirmed(key("live", "x"), "v".into(), 1, volatile(600));
    fs::remove_file(dir.path().join(".capacity")).unwrap();
    let before = crate::cache::disk::SCANS.with(std::cell::Cell::get);
    assert!(
        store
            .put_confirmed(key("new", "x"), "v".into(), 1, volatile(600))
            .1
    );
    assert_eq!(
        entry_files(dir.path()).len(),
        2,
        "all stale entries leave in one pass"
    );
    assert!(store.get(&key("live", "x")).is_some());
    assert!(
        store
            .put_confirmed(key("next", "x"), "v".into(), 1, volatile(600))
            .1
    );
    assert_eq!(
        crate::cache::disk::SCANS.with(std::cell::Cell::get) - before,
        1,
        "the next write must not restart the completed stale sweep"
    );
    for index in 0..128 {
        assert!(store.get(&key(&format!("old{index}"), "x")).is_none());
    }
}

/// Expiration is enforced on reads even before maintenance has run.
#[test]
fn an_expired_entry_is_never_served_before_a_sweep() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    store.put_confirmed(key("old", "x"), "v".into(), 1, volatile(60));
    age_disk_entries(dir.path(), 120);
    assert!(store.get(&key("old", "x")).is_none());
}

/// The maintenance sweep clears a whole backlog of expired entries in one
/// scan of the store, under one hold of its lock, and keeps every live entry
/// (R10-1).
#[test]
fn pruning_clears_every_expired_entry_in_one_scan() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    let backlog = 192;
    for index in 0..backlog {
        store.put_confirmed(
            key(&format!("old{index}"), "x"),
            "v".into(),
            1,
            volatile(60),
        );
        store.put_confirmed(
            key(&format!("live{index}"), "x"),
            "v".into(),
            1,
            volatile(600),
        );
    }
    age_disk_entries(dir.path(), 120);
    let scans = crate::cache::disk::SCANS.with(std::cell::Cell::get);
    Disk::new(dir.path().into()).prune_expired();
    assert_eq!(
        crate::cache::disk::SCANS.with(std::cell::Cell::get) - scans,
        1,
        "one scan clears the backlog"
    );
    assert_eq!(
        entry_files(dir.path()).len(),
        backlog,
        "only live entries stay"
    );
    for index in 0..backlog {
        assert!(store.get(&key(&format!("live{index}"), "x")).is_some());
        assert!(store.get(&key(&format!("old{index}"), "x")).is_none());
    }
}

/// Date every entry under `dir` `seconds` ahead of the wall clock: a store
/// time written by a clock that was far ahead.
fn date_entries_ahead(dir: &Path, seconds: u64) {
    for file in entry_files(dir) {
        let bytes = fs::read(&file).unwrap();
        let (header, value) = DiskHeader::split(&bytes).unwrap();
        let header = DiskHeader {
            stored_at_unix: crate::cache::disk::now_unix() + seconds,
            ..header
        };
        let mut rewritten = serde_json::to_vec(&header).unwrap();
        rewritten.push(b'\n');
        rewritten.extend_from_slice(value);
        fs::write(&file, rewritten).unwrap();
    }
}

const TEN_YEARS: u64 = 10 * 365 * 86_400;

#[test]
fn an_entry_stored_by_a_clock_far_ahead_expires_on_read_and_in_a_sweep() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    store.put_confirmed(key("ahead", "x"), "v".into(), 1, volatile(60));
    store.put_confirmed(key("kept", "x"), "v".into(), 1, CacheClass::Immutable);
    date_entries_ahead(dir.path(), TEN_YEARS);
    Disk::new(dir.path().into()).prune_expired();
    let reader = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    assert!(reader.get(&key("ahead", "x")).is_none());
    assert!(
        reader.get(&key("kept", "x")).is_some(),
        "an entry with no lifetime has none to outrun"
    );
    assert_eq!(entry_files(dir.path()).len(), 1, "the sweep removed it");
}

#[test]
fn a_small_clock_step_ahead_does_not_expire_a_live_entry() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    store.put_confirmed(key("near", "x"), "v".into(), 1, volatile(600));
    date_entries_ahead(dir.path(), 30);
    Disk::new(dir.path().into()).prune_expired();
    assert!(store.get(&key("near", "x")).is_some());
}

#[cfg(unix)]
#[test]
fn an_idle_entry_used_by_a_clock_far_ahead_expires_in_a_sweep_and_on_read() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    let idle = CacheClass::Idle {
        ttl: Duration::from_secs(600),
    };
    store.put_confirmed(key("idle", "x"), "v".into(), 1, idle);
    store.put_confirmed(key("idle2", "x"), "v".into(), 1, idle);
    let future = std::time::SystemTime::now() + Duration::from_secs(TEN_YEARS);
    for file in entry_files(dir.path()) {
        fs::File::options()
            .write(true)
            .open(file)
            .unwrap()
            .set_modified(future)
            .unwrap();
    }
    assert!(store.get(&key("idle2", "x")).is_none(), "a read refuses it");
    Disk::new(dir.path().into()).prune_expired();
    assert_eq!(
        entry_files(dir.path()).len(),
        0,
        "the sweep removed the rest"
    );
}

/// Whether this process can still create a file in the read-only `dir`
/// (a superuser can: such a test says nothing).
#[cfg(unix)]
fn read_only(dir: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o555)).unwrap();
    let writable = fs::File::create(dir.join(".probe")).is_ok();
    if writable {
        let _ = fs::remove_file(dir.join(".probe"));
        fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
    }
    !writable
}

#[cfg(unix)]
fn writable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A store the filesystem refuses access to says so, instead of "busy": a
/// busy store clears by itself, a read-only one does not.
#[cfg(unix)]
#[test]
fn a_read_only_store_says_it_is_not_writable_rather_than_busy() {
    let dir = tempfile::tempdir().unwrap();
    if !read_only(dir.path()) {
        return;
    }
    let store = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    let notes = octocode_io::notice::ResultNotes::default();
    let entered = notes.enter();
    let (_, saved) = store.put_confirmed(key("x", "x"), "v".into(), 1, volatile(60));
    drop(entered);
    writable(dir.path());
    assert!(!saved);
    let said = notes.take();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("not writable"), "{said:?}");
    assert!(!said[0].contains("busy"), "{said:?}");
}

/// The failure of a write is told apart: a full or read-only filesystem is a
/// cause the caller names, a refusal of the entry itself is not.
#[cfg(unix)]
#[test]
fn a_write_the_filesystem_refuses_reports_its_kind() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    // The first write makes the store's files, so a later refusal is the write's own.
    assert!(
        store
            .put_confirmed(key("a", "x"), "v".into(), 1, volatile(60))
            .1
    );
    if !read_only(dir.path()) {
        return;
    }
    let (_, refused) = store.put_checked(
        Instant::now() + Duration::from_secs(1),
        key("b", "x"),
        "v".into(),
        1,
        volatile(60),
    );
    writable(dir.path());
    assert!(
        matches!(
            refused,
            Err(
                crate::cache::NotStored::Failed(std::io::ErrorKind::PermissionDenied)
                    | crate::cache::NotStored::Unlocked(Some(std::io::ErrorKind::PermissionDenied))
            )
        ),
        "{refused:?}"
    );
    let (_, too_big) = Store::<String>::new(CacheConfig { disk_bytes: 1 }, Some(dir.path().into()))
        .put_checked(
            Instant::now() + Duration::from_secs(1),
            key("c", "x"),
            "v".into(),
            1,
            volatile(60),
        );
    assert_eq!(too_big, Err(crate::cache::NotStored::Refused));
}
