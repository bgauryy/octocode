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
