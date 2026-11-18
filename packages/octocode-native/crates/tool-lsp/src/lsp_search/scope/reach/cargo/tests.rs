use super::*;

fn package(dir: &Path) {
    std::fs::write(
        dir.join(MANIFEST),
        "[package]\nname = \"p\"\nversion = \"0.1.0\"\n",
    )
    .expect("manifest");
}

/// `relocated_modules` of `package` on a thread: a read that blocks fails
/// the test instead of hanging it.
fn modules_of(package: &Path) -> Option<Vec<(PathBuf, PathBuf)>> {
    let package = package.to_path_buf();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(relocated_modules(&package, &|| false));
    });
    receiver
        .recv_timeout(std::time::Duration::from_secs(20))
        .expect("the scan returned")
}

#[test]
fn a_path_attribute_in_a_source_file_is_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path());
    std::fs::write(dir.path().join("a.rs"), "#[path = \"b.rs\"]\nmod b;\n").expect("source");
    let found = modules_of(dir.path()).expect("finished");
    assert_eq!(found.len(), 1);
    assert!(found[0].1.ends_with("b.rs"));
}

#[cfg(unix)]
#[test]
fn a_fifo_named_like_source_is_not_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path());
    octocode_runtime_base::test_support::make_fifo(&dir.path().join("x.rs"));
    assert_eq!(modules_of(dir.path()), Some(Vec::new()));
}

#[test]
fn a_source_file_past_the_read_limit_is_not_read() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path());
    let mut text = "#[path = \"b.rs\"]\nmod b;\n".to_owned();
    text.push_str(&" ".repeat(octocode_runtime_base::security::SourceLimits::REAL.whole_source));
    std::fs::write(dir.path().join("big.rs"), text).expect("source");
    assert_eq!(modules_of(dir.path()), Some(Vec::new()));
}

#[test]
fn a_directory_the_default_prune_leaves_out_is_not_walked() {
    let dir = tempfile::tempdir().expect("tempdir");
    package(dir.path());
    let store = dir.path().join("node_modules");
    std::fs::create_dir(&store).expect("store");
    std::fs::write(store.join("k.rs"), "#[path = \"b.rs\"]\nmod b;\n").expect("source");
    assert_eq!(modules_of(dir.path()), Some(Vec::new()));
}
