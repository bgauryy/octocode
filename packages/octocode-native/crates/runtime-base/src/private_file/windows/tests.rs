use super::*;
use crate::private_file::BeneathDir;
use std::{ffi::OsStr, io::Write};

#[test]
fn held_chain_prevents_ancestor_swaps_and_allows_child_transactions() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let nested = root.join("a/b");
    let held = BeneathDir::create_all(temp.path(), &nested).unwrap();
    assert!(std::fs::rename(root.join("a"), root.join("moved")).is_err());
    assert!(std::fs::rename(&root, temp.path().join("moved-root")).is_err());
    held.write_atomic(OsStr::new("data"), b"first").unwrap();
    held.write_atomic(OsStr::new("data"), b"second").unwrap();
    assert_eq!(std::fs::read(nested.join("data")).unwrap(), b"second");
    assert!(held.still_at(temp.path(), &nested).unwrap());
    held.remove_if_present(OsStr::new("data")).unwrap();
    drop(held);
    std::fs::rename(root.join("a"), root.join("moved")).unwrap();
}

#[test]
fn held_directory_refuses_data_write_handles() {
    let temp = tempfile::tempdir().unwrap();
    let held = BeneathDir::open(temp.path(), temp.path()).unwrap();
    let open_write = || {
        OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ)
            .open(temp.path())
    };
    assert_eq!(
        open_write().unwrap_err().raw_os_error(),
        Some(windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION as i32)
    );
    drop(held);
    open_write().unwrap();
}

#[test]
fn junction_components_are_rejected_without_touching_the_target() {
    let temp = tempfile::tempdir().unwrap();
    let outside = temp.path().join("outside");
    let link = temp.path().join("junction");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("file"), b"outside").unwrap();
    // Directory junctions do not require the symbolic-link privilege.
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(BeneathDir::open(temp.path(), &link).is_err());
    assert!(BeneathDir::create_all(temp.path(), &link.join("sub")).is_err());
    assert_eq!(std::fs::read(outside.join("file")).unwrap(), b"outside");
    assert!(!outside.join("sub").exists());
    std::fs::remove_dir(link).unwrap();
}

#[test]
fn no_follow_opens_regular_files_and_preserves_nontruncating_locks() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("lock");
    let mut file = crate::private_file::open_no_follow(&path, true).unwrap();
    file.write_all(b"lock-content").unwrap();
    drop(file);
    crate::private_file::open_no_follow(&path, true).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), b"lock-content");
}

#[test]
fn relative_operations_reject_paths_and_streams_and_reuse_locks() {
    let temp = tempfile::tempdir().unwrap();
    let held = BeneathDir::open(temp.path(), temp.path()).unwrap();
    for name in [
        "../escape",
        "sub/file",
        "sub\\file",
        "file:stream",
        "C:\\escape",
    ] {
        assert!(held.create_new(OsStr::new(name), 0o600).is_err(), "{name}");
        assert!(held.open_lock(OsStr::new(name)).is_err(), "{name}");
    }
    let mut lock = held.open_lock(OsStr::new("lock")).unwrap();
    lock.write_all(b"retained").unwrap();
    drop(lock);
    held.open_lock(OsStr::new("lock")).unwrap();
    assert_eq!(
        std::fs::read(temp.path().join("lock")).unwrap(),
        b"retained"
    );
    assert!(!temp.path().join("escape").exists());
}
