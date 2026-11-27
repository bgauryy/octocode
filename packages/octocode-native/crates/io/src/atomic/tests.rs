use super::*;

#[test]
fn a_directory_syncs_and_a_missing_one_is_reported_on_unix() {
    let dir = tempfile::tempdir().unwrap();
    sync_dir(dir.path()).unwrap();
    #[cfg(unix)]
    assert!(sync_dir(&dir.path().join("missing")).is_err());
}

#[cfg(unix)]
#[test]
fn a_new_private_directory_is_owner_only_and_nested_levels_too() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache").join("deep");
    create_private_dir_all(&path).unwrap();
    for level in [dir.path().join("cache"), path] {
        assert_eq!(
            std::fs::metadata(&level).unwrap().permissions().mode() & 0o077,
            0
        );
    }
}

#[cfg(unix)]
#[test]
fn an_existing_directory_keeps_its_mode_when_others_cannot_write_it() {
    // A user-chosen cache directory is not ours to re-permission.
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shared-looking");
    std::fs::create_dir(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    create_private_dir_all(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

#[cfg(unix)]
#[test]
fn a_group_writable_directory_from_a_umask_002_system_is_accepted() {
    // umask 002 (user-private groups) creates 0775 directories; an older
    // build left such directories behind, and they must keep working.
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lsp");
    std::fs::create_dir(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o775)).unwrap();
    create_private_dir_all(&path).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o775
    );
}

#[cfg(unix)]
#[test]
fn a_directory_others_can_write_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("world-writable");
    std::fs::create_dir(&path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o777)).unwrap();
    let error = create_private_dir_all(&path).expect_err("writable by others");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
}

#[cfg(unix)]
#[test]
fn a_symlink_at_a_private_directory_path_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("real");
    std::fs::create_dir(&target).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let error = create_private_dir_all(&link).expect_err("symlink");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

#[test]
fn creating_an_existing_private_directory_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();
    create_private_dir_all(dir.path()).unwrap();
    create_private_dir_all(dir.path()).unwrap();
}

#[test]
fn atomic_write_replaces_owner_only_and_leaves_no_temporary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    std::fs::write(&path, "old").unwrap();
    for durable in [false, true] {
        write_atomic(&path, b"new", durable).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn a_failed_streamed_write_keeps_the_old_file_and_no_temporary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tool");
    std::fs::write(&path, "old").unwrap();
    let error = write_atomic_with(&path, 0o700, false, |file| {
        file.write_all(b"partial")?;
        Err::<(), _>(io::Error::other("source ended early"))
    })
    .unwrap_err();
    assert_eq!(error.to_string(), "source ended early");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

    let written = write_atomic_with(&path, 0o700, false, |file| {
        io::copy(&mut &b"streamed"[..], file)
    })
    .unwrap();
    assert_eq!(written, 8);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "streamed");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}
