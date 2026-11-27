use super::*;
fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let path = fs::canonicalize(root.path()).unwrap().join(".env");
    (root, path)
}
#[test]
fn revision_backup_and_permissions() {
    let (_root, path) = fixture();
    assert!(
        replace_private_config(&path, "missing", "KEY=one\n", true)
            .unwrap()
            .0
    );
    assert!(replace_private_config(&path, "missing", "KEY=two\n", true).is_err());
    let revision = config_revision(read_private_config(&path).unwrap().as_deref());
    let (changed, backup) = replace_private_config(&path, &revision, "KEY=two\n", true).unwrap();
    assert!(changed);
    let backup = backup.unwrap();
    assert_eq!(fs::read_to_string(&backup).unwrap(), "KEY=one\n");
    let revision = config_revision(Some("KEY=two\n"));
    assert_eq!(
        replace_private_config(&path, &revision, "KEY=three\n", true)
            .unwrap()
            .1,
        Some(backup.clone())
    );
    assert_eq!(fs::read_to_string(&backup).unwrap(), "KEY=two\n");
    let backups = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter(|entry| {
            entry
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".bak")
        })
        .count();
    assert_eq!(backups, 1, "one rolling backup, never one per save");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}
#[cfg(unix)]
#[test]
fn rejects_symlink_and_retains_directory_permissions() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (_root, path) = fixture();
    let parent = path.parent().unwrap();
    fs::set_permissions(parent, fs::Permissions::from_mode(0o755)).unwrap();
    replace_private_config(&path, "missing", "KEY=one\n", false).unwrap();
    assert_eq!(
        fs::metadata(parent).unwrap().permissions().mode() & 0o777,
        0o755
    );
    let link = parent.join("linked.env");
    symlink(&path, &link).unwrap();
    assert!(read_private_config(&link).is_err());
    assert!(replace_private_config(&link, "missing", "KEY=two\n", false).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "KEY=one\n");
}
/// A symlinked directory above the file (macOS `/tmp`, dotfile managers)
/// is followed; only the file itself must not be a symlink.
#[cfg(unix)]
#[test]
fn symlinked_parent_directory_is_readable_and_writable() {
    let (_root, path) = fixture();
    let real = path.parent().unwrap().join("real");
    fs::create_dir(&real).unwrap();
    let linked = path.parent().unwrap().join("linked");
    std::os::unix::fs::symlink(&real, &linked).unwrap();
    let file = linked.join(".env");
    assert!(edit_scoped_env(&file, "CUSTOM_KEY", Some("one"), false, None).unwrap());
    assert_eq!(
        parse_env(read_private_config(&file).unwrap().as_deref())["CUSTOM_KEY"],
        "one"
    );
    assert!(real.join(".env").is_file());
}
fn hold_lock(path: &Path) -> fs::File {
    let lock = open_no_follow(&path.with_file_name(".env.lock"), true).unwrap();
    lock.lock().unwrap();
    lock
}
#[test]
fn an_edit_waits_out_a_briefly_held_lock() {
    let (_root, path) = fixture();
    let held = hold_lock(&path);
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        drop(held);
    });
    assert!(
        replace_private_config(&path, "missing", "KEY=one\n", false)
            .unwrap()
            .0
    );
    release.join().unwrap();
}
#[test]
fn an_edit_reports_busy_when_the_lock_stays_held() {
    let (_root, path) = fixture();
    let _held = hold_lock(&path);
    let started = std::time::Instant::now();
    let error = replace_private_config(&path, "missing", "KEY=one\n", false).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    assert!(started.elapsed() >= LOCK_WAIT, "{:?}", started.elapsed());
    assert!(!path.exists());
}
#[test]
fn owned_env_edits_leave_no_backup_of_removed_secrets() {
    let (_root, path) = fixture();
    edit_scoped_env(&path, "CUSTOM_KEY", Some("secret-one"), false, None).unwrap();
    edit_scoped_env(&path, "CUSTOM_KEY", None, false, None).unwrap();
    for entry in fs::read_dir(path.parent().unwrap()).unwrap() {
        let entry = entry.unwrap();
        assert!(
            !fs::read_to_string(entry.path())
                .unwrap_or_default()
                .contains("secret-one"),
            "{:?}",
            entry.file_name()
        );
    }
}
#[test]
fn scoped_policy_and_roundtrip_quotes() {
    let (_root, path) = fixture();
    assert!(edit_scoped_env(&path, "PATH", Some("bad"), false, Some("missing")).is_err());
    assert!(
        edit_scoped_env(
            &path,
            "OCTOCODE_STORAGE_MODE",
            Some("persistent"),
            true,
            Some("missing")
        )
        .is_err()
    );
    assert!(
        edit_scoped_env(
            &path,
            "OCTOCODE_STORAGE_MODE",
            Some("memory"),
            true,
            Some("missing")
        )
        .unwrap()
    );
    let revision = config_revision(read_private_config(&path).unwrap().as_deref());
    let value = "quoted\"value'\\literal";
    edit_scoped_env(&path, "CUSTOM_KEY", Some(value), false, Some(&revision)).unwrap();
    assert_eq!(
        parse_env(read_private_config(&path).unwrap().as_deref())["CUSTOM_KEY"],
        value
    );
}
