//! A scope fingerprint outlives its request while a stamp proof shows the
//! walk would read the same lines.
use super::fingerprint::{FingerprintScope, fingerprint, fingerprints};
use super::*;
use octocode_engine::stamp::settle_written_so_far;

fn of(root: &Path, observed: SystemTime) -> Fingerprint {
    let include = ["*.go".to_owned()];
    fingerprint(
        &FingerprintScope {
            root,
            server_root: Some(root),
            include: &include,
        },
        observed,
        &|| false,
    )
    .expect("fingerprint")
}

/// Settled, a repeat is served without a walk and says the same; an edit, a
/// new file and a removal each walk again and say what changed.
#[test]
fn a_settled_fingerprint_serves_repeats_until_a_file_changes() {
    let (_dir, root) = fixture();
    std::fs::create_dir_all(root.join(".git")).expect("git");
    write(&root.join("a.go"), "package a\n");
    write(&root.join("sub/b.go"), "package sub\n");
    settle_written_so_far();
    let first = of(&root, octocode_engine::stamp::settle_clock());
    assert_eq!(fingerprints::of(&root), 1);
    let again = of(&root, octocode_engine::stamp::settle_clock());
    assert_eq!(again.0, first.0);
    assert_eq!(again.1, first.1);
    assert_eq!(fingerprints::of(&root), 1, "a repeat reads the kept walk");
    write(&root.join("sub/b.go"), "package sub\n\nfunc F() {}\n");
    settle_written_so_far();
    let edited = of(&root, octocode_engine::stamp::settle_clock());
    assert_eq!(fingerprints::of(&root), 2);
    assert_ne!(edited.0, first.0);
    write(&root.join("sub/deeper/c.go"), "package deeper\n");
    settle_written_so_far();
    let added = of(&root, octocode_engine::stamp::settle_clock());
    assert_eq!(fingerprints::of(&root), 3);
    assert!(added.1.contains_key("sub/deeper/c.go"));
    std::fs::remove_file(root.join("a.go")).expect("remove");
    settle_written_so_far();
    let removed = of(&root, octocode_engine::stamp::settle_clock());
    assert_eq!(fingerprints::of(&root), 4);
    assert!(!removed.1.contains_key("a.go"));
}

/// A fingerprint that hashed a fresh file's content is not kept: the next
/// instant may see it settled, with another line.
#[test]
fn a_fingerprint_with_a_fresh_file_is_not_kept() {
    let (_dir, root) = fixture();
    std::fs::create_dir_all(root.join(".git")).expect("git");
    write(&root.join("a.go"), "package a\n");
    let now = SystemTime::now();
    of(&root, now);
    of(&root, now);
    assert_eq!(fingerprints::of(&root), 2);
}
