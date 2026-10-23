use super::*;
#[cfg(unix)]
use std::io::Write;

#[test]
fn bounded_read_rejects_oversize_instead_of_truncating() {
    assert_eq!(read_limited(&b"abcd"[..], 4).unwrap(), b"abcd");
    assert!(read_limited(&b"abcde"[..], 4).is_err());
}

#[cfg(unix)]
#[test]
fn beneath_open_never_follows_a_swapped_directory_or_file() {
    let dir = tempfile::tempdir().unwrap();
    let boundary = dir.path().join("root");
    let outside = dir.path().join("outside");
    fs::create_dir_all(boundary.join("src")).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(boundary.join("src/a.ts"), "inside").unwrap();
    fs::write(outside.join("a.ts"), "outside").unwrap();
    let target = boundary.join("src/a.ts");
    let mut read = String::new();
    open_beneath(&boundary, &target)
        .unwrap()
        .read_to_string(&mut read)
        .unwrap();
    assert_eq!(read, "inside");
    // A directory swapped for a link to outside the boundary.
    fs::rename(boundary.join("src"), boundary.join("moved")).unwrap();
    std::os::unix::fs::symlink(&outside, boundary.join("src")).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "outside");
    let error = open_beneath(&boundary, &target).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    // A final component swapped for a link.
    fs::remove_file(boundary.join("src")).unwrap();
    fs::create_dir(boundary.join("src")).unwrap();
    std::os::unix::fs::symlink(outside.join("a.ts"), &target).unwrap();
    let error = open_beneath(&boundary, &target).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    // Nothing outside the boundary or above it.
    assert!(open_beneath(&boundary, &outside.join("a.ts")).is_err());
    assert!(open_beneath(&boundary, &boundary.join("src/../moved/a.ts")).is_err());
    assert!(open_beneath(&boundary, &boundary.join("moved/a.ts")).is_ok());
}

/// Operations through a held directory stay on the directory that was
/// walked, even after its path is swapped for a link to outside, and a
/// fresh walk tells the swap apart.
#[cfg(unix)]
#[test]
fn a_held_directory_keeps_its_operations_after_a_swap() {
    let dir = tempfile::tempdir().unwrap();
    let boundary = dir.path().join("root");
    let outside = dir.path().join("outside");
    fs::create_dir_all(boundary.join("src")).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("a.ts"), "outside").unwrap();
    let src = boundary.join("src");
    let held = BeneathDir::open(&boundary, &src).unwrap();
    assert!(held.still_at(&boundary, &src).unwrap());
    held.create_new(OsStr::new("stage"), 0o644)
        .unwrap()
        .write_all(b"new")
        .unwrap();
    fs::rename(&src, boundary.join("moved")).unwrap();
    std::os::unix::fs::symlink(&outside, &src).unwrap();
    assert_eq!(
        held.still_at(&boundary, &src).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    held.rename(OsStr::new("stage"), OsStr::new("a.ts"))
        .unwrap();
    let mut read = String::new();
    held.open_file(OsStr::new("a.ts"))
        .unwrap()
        .read_to_string(&mut read)
        .unwrap();
    assert_eq!(read, "new");
    assert_eq!(
        fs::read_to_string(boundary.join("moved/a.ts")).unwrap(),
        "new"
    );
    assert_eq!(fs::read_to_string(outside.join("a.ts")).unwrap(), "outside");
    held.remove_if_present(OsStr::new("a.ts")).unwrap();
    held.remove_if_present(OsStr::new("a.ts")).unwrap();
    assert_eq!(fs::read_dir(boundary.join("moved")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    // The boundary itself is a directory beneath itself; `..` is not.
    assert!(BeneathDir::open(&boundary, &boundary).is_ok());
    assert!(BeneathDir::open(&boundary, &boundary.join("moved/..")).is_err());
    // A link in place of a file is refused, never opened through.
    std::os::unix::fs::symlink(outside.join("a.ts"), boundary.join("moved/link")).unwrap();
    let moved = BeneathDir::open(&boundary, &boundary.join("moved")).unwrap();
    assert!(moved.open_file(OsStr::new("link")).is_err());
    assert!(moved.create_new(OsStr::new("link"), 0o600).is_err());
}

#[cfg(unix)]
#[test]
fn no_follow_open_rejects_symlinks_and_fifos() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target");
    fs::write(&target, "x").unwrap();
    let link = dir.path().join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(open_no_follow(&link, false).is_err());
    assert!(open_no_follow(&link, true).is_err());
    let fifo = dir.path().join("fifo");
    crate::tools::test_support::make_fifo(&fifo);
    assert!(open_no_follow(&fifo, false).is_err());
    assert!(open_no_follow(&target, false).is_ok());
}
