use super::*;
fn credential(host: &str, token: &str) -> StoredCredentials {
    serde_json::from_value(serde_json::json!({"hostname":host,"username":"fixture-user","token":{"token":token,"tokenType":"oauth","refreshToken":"synthetic-refresh","expiresAt":"2099-01-01T00:00:00Z"},"gitProtocol":"https","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z"})).unwrap()
}
fn fixture(home: &Path) {
    fs::write(
        home.join("credentials.json"),
        include_str!("../../../../../tests/fixtures/auth/main-credentials.enc"),
    )
    .unwrap();
    fs::write(
        home.join(".key"),
        include_str!("../../../../../tests/fixtures/auth/main-key.hex"),
    )
    .unwrap();
}
#[test]
fn reads_main_node_fixture_without_migrating_or_rotating_it() {
    let dir = tempfile::tempdir().unwrap();
    fixture(dir.path());
    let before = fs::read(dir.path().join("credentials.json")).unwrap();
    let stored = HomeStore::new(dir.path())
        .load("127.0.0.1")
        .unwrap()
        .unwrap();
    assert_eq!(stored.username, "legacy-home-user");
    assert_eq!(stored.token.token, "synthetic-legacy-home-token");
    assert_eq!(
        fs::read(dir.path().join("credentials.json")).unwrap(),
        before
    );
}
#[test]
fn fresh_roundtrip_host_isolation_update_and_last_logout_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("new/home");
    let store = HomeStore::new(&home);
    assert!(store.load("example.test").unwrap().is_none());
    assert!(!home.exists());
    store
        .save(&credential("EXAMPLE.TEST", "first-secret"))
        .unwrap();
    let key = fs::read(home.join(".key")).unwrap();
    let first = fs::read(home.join("credentials.json")).unwrap();
    assert!(!String::from_utf8_lossy(&first).contains("first-secret"));
    store
        .save(&credential("example.test", "rotated-secret"))
        .unwrap();
    store
        .save(&credential("other.test", "other-secret"))
        .unwrap();
    assert_eq!(
        store.load("example.test").unwrap().unwrap().token.token,
        "rotated-secret"
    );
    assert!(store.load("absent.test").unwrap().is_none());
    assert_eq!(fs::read(home.join(".key")).unwrap(), key);
    store.delete("EXAMPLE.TEST").unwrap();
    assert!(store.load("example.test").unwrap().is_none());
    assert!(store.load("other.test").unwrap().is_some());
    store.delete("other.test").unwrap();
    store.delete("other.test").unwrap();
    assert!(!home.join("credentials.json").exists());
    assert!(!home.join(".key").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&home).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}
#[test]
fn corruption_missing_key_and_wrong_key_fail_without_overwrite() {
    for damage in ["format", "tag", "key", "missing", "oversized"] {
        let dir = tempfile::tempdir().unwrap();
        fixture(dir.path());
        let path = dir.path().join("credentials.json");
        match damage {
            "format" => fs::write(&path, "invalid").unwrap(),
            "tag" => {
                let mut data = fs::read(&path).unwrap();
                data[33] = if data[33] == b'0' { b'1' } else { b'0' };
                fs::write(&path, data).unwrap();
            }
            "key" => fs::write(dir.path().join(".key"), "08".repeat(32)).unwrap(),
            "missing" => fs::remove_file(dir.path().join(".key")).unwrap(),
            _ => fs::write(&path, vec![b'x'; MAX_FILE_BYTES as usize + 1]).unwrap(),
        }
        let before = fs::read(&path).unwrap();
        let store = HomeStore::new(dir.path());
        assert!(store.load("127.0.0.1").is_err(), "{damage}");
        assert!(
            store.save(&credential("other.test", "new-secret")).is_err(),
            "{damage}"
        );
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
#[test]
fn concurrent_writers_preserve_all_hosts_and_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let store = HomeStore::new(dir.path());
    std::thread::scope(|scope| {
        for i in 0..8 {
            let store = store.clone();
            scope.spawn(move || {
                store
                    .save(&credential(&format!("host-{i}.test"), "synthetic-secret"))
                    .unwrap()
            });
        }
    });
    for i in 0..8 {
        assert!(store.load(&format!("host-{i}.test")).unwrap().is_some());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in ["credentials.json", ".key", ".credentials.lock"] {
            assert_eq!(
                fs::metadata(dir.path().join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}
#[cfg(unix)]
#[test]
fn symlinks_and_hard_links_are_rejected_without_touching_target() {
    use std::os::unix::fs::symlink;
    for (name, hard_link) in ["credentials.json", ".key", ".credentials.lock"]
        .into_iter()
        .flat_map(|name| [(name, false), (name, true)])
    {
        let dir = tempfile::tempdir().unwrap();
        fixture(dir.path());
        let other = tempfile::tempdir().unwrap();
        let target = other.path().join("target");
        fs::write(&target, "untouched").unwrap();
        let path = dir.path().join(name);
        let _ = fs::remove_file(&path);
        if hard_link {
            fs::hard_link(&target, &path).unwrap();
        } else {
            symlink(&target, &path).unwrap();
        }
        assert!(
            HomeStore::new(dir.path())
                .save(&credential("a.test", "synthetic-secret"))
                .is_err()
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "untouched");
    }
}
