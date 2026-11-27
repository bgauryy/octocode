use super::*;

#[test]
fn a_file_past_the_limit_is_refused_before_it_is_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("big.rs");
    std::fs::write(&path, "x".repeat(65)).expect("write");
    assert!(matches!(
        read_bounded(&path, 64),
        Err(ReadError::TooLarge(65))
    ));
}

#[test]
fn a_directory_is_no_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(read_bounded(dir.path(), 1024).is_err());
}
