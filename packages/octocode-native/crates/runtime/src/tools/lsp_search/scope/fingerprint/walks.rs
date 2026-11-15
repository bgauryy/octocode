use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Entries a fingerprint of `root` (server at `server`) visits.
fn visited(root: &Path, server: &Path) -> (usize, Vec<String>) {
    let include = ["*.rs".to_owned()];
    let scope = FingerprintScope {
        root,
        server_root: Some(server),
        include: &include,
    };
    let visits = AtomicUsize::new(0);
    let stopped = || {
        visits.fetch_add(1, Ordering::Relaxed);
        false
    };
    let (_, entries) = fingerprint(&scope, SystemTime::now(), &stopped).expect("fingerprint");
    (visits.into_inner(), entries.keys().cloned().collect())
}

/// A scope inside the server's root is covered by the server's walk:
/// each of its files is visited, stamped and hashed once per request.
#[test]
fn a_scope_under_the_server_root_is_walked_once() {
    let dir = tempfile::tempdir().expect("dir");
    let server = dir.path();
    for file in ["a.rs", "sub/b.rs", "sub/deep/c.rs"] {
        let path = server.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(path, "pub fn f() {}\n").expect("file");
    }
    let (server_walk, _) = visited(server, server);
    let (visits, entries) = visited(&server.join("sub"), server);
    assert_eq!(visits, server_walk, "one walk of the server's root");
    assert!(
        ["b.rs", "deep/c.rs"]
            .iter()
            .all(|file| entries.iter().any(|entry| entry == file)),
        "{entries:?}"
    );
}

#[test]
fn server_fingerprints_cover_ignored_build_and_hidden_sources() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    std::fs::create_dir(root.join(".git")).expect("repository marker");
    std::fs::write(root.join(".gitignore"), "dist/\n").expect("ignore rule");
    for file in [
        "src/lib.rs",
        "dist/generated.rs",
        ".generated/module.rs",
        ".ssh/private.rs",
    ] {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("source directory");
        std::fs::write(path, "pub fn helper() {}\n").expect("source");
    }
    let (_, entries) = visited(root, root);
    for file in ["src/lib.rs", "dist/generated.rs", ".generated/module.rs"] {
        assert!(
            entries.iter().any(|entry| entry == file),
            "a server may compile {file} regardless of search exclusions: {entries:?}"
        );
    }
    assert!(
        !entries.iter().any(|entry| entry == ".ssh/private.rs"),
        "credential directories stay excluded: {entries:?}"
    );
}

#[test]
fn lexical_fingerprints_keep_search_exclusions() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    std::fs::create_dir(root.join(".git")).expect("repository marker");
    std::fs::write(root.join(".gitignore"), "dist/\n").expect("ignore rule");
    for file in ["src/lib.rs", "dist/generated.rs", ".generated/module.rs"] {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("source directory");
        std::fs::write(path, "pub fn helper() {}\n").expect("source");
    }
    let include = ["*.rs".to_owned()];
    let scope = FingerprintScope {
        root,
        server_root: None,
        include: &include,
    };
    let (_, entries) = fingerprint(&scope, SystemTime::now(), &|| false).expect("fingerprint");
    assert!(entries.contains_key("src/lib.rs"), "{entries:?}");
    assert!(!entries.contains_key("dist/generated.rs"), "{entries:?}");
    assert!(!entries.contains_key(".generated/module.rs"), "{entries:?}");
}

#[test]
fn large_fresh_sources_preserve_the_complete_fingerprint() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    let source = vec![b'x'; 2 * 1024 * 1024];
    std::fs::write(root.join("large.rs"), &source).expect("large source");
    std::fs::write(root.join("changed.rs"), "helper()\n").expect("other source");
    let include = ["*.rs".to_owned()];
    let scope = FingerprintScope {
        root,
        server_root: Some(root),
        include: &include,
    };
    let (_, before) = fingerprint(&scope, SystemTime::now(), &|| false)
        .expect("a large source must not discard the other files' revision");
    assert_eq!(
        split_stamp(before.get("large.rs").expect("large source")).1,
        octocode_engine::digest::sha256(&source),
        "streaming hashes every byte with the shared digest algorithm"
    );
    std::fs::write(root.join("changed.rs"), "otherx()\n").expect("same-length change");
    let (_, after) = fingerprint(&scope, SystemTime::now(), &|| false).expect("new fingerprint");
    assert_ne!(before["changed.rs"], after["changed.rs"]);
    assert_eq!(before["large.rs"], after["large.rs"]);
}

#[test]
fn source_hashing_observes_cancellation_between_reads() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("large.rs");
    std::fs::write(&path, vec![b'x'; 2 * 1024 * 1024]).expect("source");
    let checks = AtomicUsize::new(0);
    assert!(source_digest(&path, None, &|| checks.fetch_add(1, Ordering::Relaxed) > 1).is_none());
    assert!(checks.load(Ordering::Relaxed) > 1);
}

#[test]
fn poisoned_fingerprint_state_abandons_the_walk() {
    fn poison<T>(lock: &Mutex<T>) {
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = lock.lock().expect("unpoisoned fixture");
            panic!("poison the fixture");
        }));
        assert!(panic.is_err());
    }

    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("a.rs"), "pub fn f() {}\n").expect("file");
    let scope = FingerprintScope {
        root: dir.path(),
        server_root: None,
        include: &[],
    };
    for poison_read in [false, true] {
        let lines = Mutex::new(Vec::new());
        let read = Mutex::new(Read::default());
        if poison_read {
            poison(&read);
        } else {
            poison(&lines);
        }
        assert!(!walk_lines(
            &scope,
            dir.path(),
            Default::default(),
            SystemTime::now(),
            &|| false,
            &lines,
            &read,
        ));
        if poison_read {
            assert!(walked_fingerprint(&scope, SystemTime::now(), &|| false, &read).is_none());
        }
    }
}
