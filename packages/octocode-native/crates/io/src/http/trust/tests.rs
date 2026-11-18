use super::*;

#[test]
fn no_explicit_roots_preserves_the_platform_default() {
    assert!(roots(None, &[]).expect("no configured roots").is_empty());
}

#[test]
fn configured_missing_or_malformed_roots_fail_explicitly() {
    let dir = tempfile::tempdir().expect("temporary directory");
    let file = dir.path().join("ca.pem");
    assert!(roots(Some(&file), &[]).is_err());
    std::fs::write(&file, "not a certificate").expect("write malformed bundle");
    assert!(roots(Some(&file), &[]).is_err());
    assert!(roots(None, &[dir.path().to_path_buf()]).is_err());
}

#[test]
fn an_explicit_empty_or_missing_directory_fails() {
    let dir = tempfile::tempdir().expect("temporary directory");
    assert!(roots(None, &[dir.path().to_path_buf()]).is_err());
    assert!(roots(None, &[dir.path().join("missing")]).is_err());
}
