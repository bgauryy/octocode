use super::*;
use std::path::PathBuf;
use std::time::Duration;

/// Walks of one listed file, stored as `put` stores a walk.
static ONES: PageMemo<Proven<PathBuf>> = PageMemo::new();

fn put_one(snapshot: &str, policy: &str, root: &std::path::Path, file: &std::path::Path) {
    let (size, modified) = observed(file).expect("listed file");
    let listed = vec![Seen {
        path: file,
        size,
        modified,
    }];
    let paths = octocode_runtime_base::test_support::workspace_policy(root);
    let proof = proof(root, &paths, listed, &[]).expect("proof");
    ONES.put_proven(
        snapshot.to_owned(),
        policy.to_owned(),
        file.as_os_str().len(),
        proof,
        Arc::new(file.to_path_buf()),
    );
}

#[test]
fn a_stored_walk_is_served_until_a_listed_directory_changes() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("a.txt");
    std::fs::write(&file, "x").expect("file");
    let policy = "p".to_owned();
    put_one("memo-test", &policy, root.path(), &file);
    let stored = ONES.current("memo-test", &policy);
    assert_eq!(stored.as_deref(), Some(&file));
    assert!(ONES.current("memo-test", "other policy").is_none());
    // A sibling created after the walk changes the directory's time.
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(root.path().join("b.txt"), "y").expect("newcomer");
    assert!(ONES.current("memo-test", &policy).is_none());
}

/// An in-place edit of an ignore file beside the listed entries (a
/// hidden one the listing never shows) drops the stored walk, as
/// astSearch's scan memo does: the rules decide what the walk listed.
#[test]
fn a_stored_walk_is_dropped_when_an_ignore_rule_is_edited() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("a.txt");
    std::fs::write(&file, "x").expect("file");
    let rules = root.path().join(".gitignore");
    std::fs::write(&rules, "b.txt\n").expect("rules");
    let policy = "p".to_owned();
    put_one("memo-rules", &policy, root.path(), &file);
    assert!(ONES.current("memo-rules", &policy).is_some());
    std::thread::sleep(Duration::from_millis(20));
    std::fs::OpenOptions::new()
        .append(true)
        .open(&rules)
        .and_then(|mut rules| std::io::Write::write_all(&mut rules, b"c.txt\n"))
        .expect("edit rules in place");
    assert!(ONES.current("memo-rules", &policy).is_none());
}

#[test]
fn restored_mtime_does_not_keep_stale_listing_detail() {
    let root = tempfile::tempdir().expect("root");
    let file = root.path().join("a.txt");
    std::fs::write(&file, "ab\n").expect("initial source");
    let stamp = Proof::capture([file.clone()], Link::Itself, None).expect("stamp");
    let old_time = std::fs::metadata(&file)
        .expect("metadata")
        .modified()
        .expect("mtime");
    std::fs::write(&file, "a\nb").expect("same-size edit");
    std::fs::File::open(&file)
        .expect("file")
        .set_times(std::fs::FileTimes::new().set_modified(old_time))
        .expect("restore mtime");
    assert!(!stamp.holds(), "content changed with size and mtime intact");
}

#[test]
fn directory_row_uses_the_walks_zero_size_convention() {
    let root = tempfile::tempdir().expect("root");
    let child = root.path().join("empty");
    std::fs::create_dir(&child).expect("directory");
    let modified = std::fs::symlink_metadata(&child)
        .expect("metadata")
        .modified()
        .ok();
    let seen = |size| Seen {
        path: &child,
        size,
        modified,
    };
    let paths = octocode_runtime_base::test_support::workspace_policy(root.path());
    let stamp =
        proof(root.path(), &paths, vec![seen(0)], &[]).expect("a directory stamps at size 0");
    assert!(stamp.holds(), "nothing changed since the walk");
    // Any other size is a change the walk did not read.
    assert!(proof(root.path(), &paths, vec![seen(1)], &[]).is_none());
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(child.join("new.txt"), "x").expect("newcomer");
    assert!(
        !stamp.holds(),
        "an entry created in it changes the directory"
    );
}

/// The ignore rules of a repository nested in the walk include its own
/// exclude file, outside the walk: an edit there drops the stored walk.
/// An ignore file no stamp can watch (a link) leaves the walk unstored.
#[cfg(unix)]
#[test]
fn a_nested_repository_exclude_and_a_linked_rule_decide_the_proof() {
    let root = tempfile::tempdir().expect("root");
    let nested = root.path().join("nested");
    std::fs::create_dir_all(nested.join(".git").join("info")).expect("nested repository");
    let exclude = nested.join(".git").join("info").join("exclude");
    std::fs::write(&exclude, "b.txt\n").expect("exclude");
    let file = nested.join("a.txt");
    std::fs::write(&file, "x").expect("file");
    let paths = octocode_runtime_base::test_support::workspace_policy(root.path());
    let listed = || {
        let (size, modified) = observed(&file).expect("listed file");
        vec![Seen {
            path: file.as_path(),
            size,
            modified,
        }]
    };
    let stamp = proof(root.path(), &paths, listed(), &[]).expect("stamped");
    assert!(stamp.holds());
    std::thread::sleep(Duration::from_millis(20));
    std::fs::OpenOptions::new()
        .append(true)
        .open(&exclude)
        .and_then(|mut rules| std::io::Write::write_all(&mut rules, b"c.txt\n"))
        .expect("edit the nested exclude in place");
    assert!(!stamp.holds(), "the nested repository's rules changed");
    std::os::unix::fs::symlink(&exclude, nested.join(".gitignore")).expect("link");
    assert!(proof(root.path(), &paths, listed(), &[]).is_none());
}
