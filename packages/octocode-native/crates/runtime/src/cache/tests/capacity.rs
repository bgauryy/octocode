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
    let config = CacheConfig {
        max_bytes: 512,
        disk_bytes: 1024,
        ..Default::default()
    };
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
    let store = Store::<String>::new(
        CacheConfig {
            disk_bytes: 1,
            ..Default::default()
        },
        Some(dir.path().into()),
    );
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
        let config = CacheConfig {
            disk_bytes: 1024,
            ..Default::default()
        };
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
            reopened
                .get_current(&key("new", "scope"))
                .unwrap()
                .value
                .as_str(),
            "n".repeat(100)
        );
    }
}

#[test]
fn replacements_and_deleted_entries_do_not_leak_capacity() {
    let dir = tempfile::tempdir().unwrap();
    let config = CacheConfig {
        disk_bytes: 1024,
        ..Default::default()
    };
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
            store
                .get_current(&key("same", "scope"))
                .unwrap()
                .value
                .as_str(),
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
    let store = Store::<String>::new(
        CacheConfig {
            disk_bytes: 1024,
            ..Default::default()
        },
        Some(dir.path().into()),
    );
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
    let store = Store::<String>::new(
        CacheConfig {
            disk_bytes: 2048,
            ..Default::default()
        },
        Some(dir),
    );
    for i in 0..40 {
        assert!(
            store
                .put_confirmed(
                    key(&format!("{writer}-{i}"), "scope"),
                    "x".repeat(100),
                    100,
                    CacheClass::Immutable
                )
                .1
        );
    }
}

#[test]
fn independent_processes_share_one_disk_budget() {
    use std::process::{Command, Stdio};
    let dir = tempfile::tempdir().unwrap();
    let mut children: Vec<_> = (0..4)
        .map(|writer| {
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "cache::tests::capacity::disk_writer_process",
                ])
                .env("OCTOCODE_CACHE_TEST_DIR", dir.path())
                .env("OCTOCODE_CACHE_TEST_WRITER", writer.to_string())
                .stdout(Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut failures = Vec::new();
    for child in &mut children {
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                if !status.success() {
                    failures.push(format!("{status}"));
                }
                break;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                failures.push("writer exceeded deadline".into());
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
    assert!((1..=2048).contains(&bytes(dir.path())));
}

#[test]
fn immediately_expired_work_is_never_confirmed_in_either_storage_mode() {
    let dir = tempfile::tempdir().unwrap();
    for disk in [None, Some(dir.path().join("store"))] {
        let store = Store::<String>::new(CacheConfig::default(), disk.clone());
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
