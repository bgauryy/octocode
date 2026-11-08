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

/// A backlog of expired entries costs each write a bounded share of it, and the
/// next writes finish it, however far the last scan was; none is ever served.
#[test]
fn a_write_clears_a_bounded_share_of_expired_entries_and_none_is_served() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    let backlog = 3 * crate::cache::disk::EXPIRED_PER_SCAN;
    for index in 0..backlog {
        store.put_confirmed(
            key(&format!("old{index}"), "x"),
            "v".into(),
            1,
            volatile(60),
        );
    }
    age_disk_entries(dir.path(), 120);
    let reader = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    // A scan is due: the one that wrote them is this minute's.
    fs::remove_file(dir.path().join(".capacity")).unwrap();
    store.put_confirmed(key("new0", "x"), "v".into(), 1, volatile(600));
    let left = entry_files(dir.path()).len();
    assert_eq!(
        left,
        backlog + 1 - crate::cache::disk::EXPIRED_PER_SCAN,
        "one write removes one share"
    );
    assert!(
        reader.get(&key("new0", "x")).is_some(),
        "the entry written stays"
    );
    let mut writes = 0;
    while entry_files(dir.path()).len() > writes + 1 {
        writes += 1;
        assert!(writes <= backlog, "the backlog must drain");
        store.put_confirmed(
            key(&format!("new{writes}"), "x"),
            "v".into(),
            1,
            volatile(600),
        );
    }
    assert!(writes < backlog, "the writes after it finish the backlog");
    let live = (0..=writes).filter(|at| reader.get(&key(&format!("new{at}"), "x")).is_some());
    assert_eq!(live.count(), writes + 1, "no live entry is lost");
    assert_eq!(entry_files(dir.path()).len(), writes + 1);
}

/// An expired entry the sweep has not reached yet is never an answer.
#[test]
fn an_expired_entry_left_by_the_bound_is_never_served() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    let backlog = 2 * crate::cache::disk::EXPIRED_PER_SCAN;
    for index in 0..backlog {
        store.put_confirmed(
            key(&format!("old{index}"), "x"),
            "v".into(),
            1,
            volatile(60),
        );
    }
    age_disk_entries(dir.path(), 120);
    fs::remove_file(dir.path().join(".capacity")).unwrap();
    store.put_confirmed(key("new", "x"), "v".into(), 1, volatile(600));
    for index in 0..backlog {
        assert!(store.get(&key(&format!("old{index}"), "x")).is_none());
    }
}

/// The maintenance sweep clears a whole backlog of expired entries in one
/// scan of the store, under one hold of its lock, and keeps every live entry
/// (R10-1).
#[test]
fn pruning_clears_every_expired_entry_in_one_scan() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::<String>::new(CacheConfig::derived(), Some(dir.path().into()));
    let backlog = 6 * crate::cache::disk::EXPIRED_PER_SCAN;
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
