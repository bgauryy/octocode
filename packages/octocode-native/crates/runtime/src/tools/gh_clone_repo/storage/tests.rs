use super::*;

#[test]
fn checkout_destinations_ignore_case() {
    let home = Path::new("/tmp/octocode-clones-root");
    let sha = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
    let entry =
        |owner, repo, commit| clone_dir(home, owner, repo, commit, None, 1, "https://example.test");
    let lower = sha.to_ascii_lowercase();
    assert_eq!(entry("o", "r", sha), entry("o", "r", &lower));
    // Owner/repo are case-insensitive on GitHub: one checkout.
    assert_eq!(entry("Octo", "Repo", sha), entry("octo", "repo", sha));
    assert!(entry("o", "r", sha).starts_with(home.join("clones/o/r")));
}
#[test]
fn deep_history_and_sparse_sets_key_their_own_entries() {
    let home = Path::new("/tmp/octocode-clones-root");
    let sha = "c".repeat(40);
    let shallow = clone_dir(home, "o", "r", &sha, None, 1, "https://example.test");
    let deep = clone_dir(home, "o", "r", &sha, None, 5, "https://example.test");
    assert_ne!(shallow, deep);
    assert!(deep.to_string_lossy().contains("__d5__host_"));
    crate::tools::test_support::checks::assert_absent(
        &serde_json::json!(shallow.to_string_lossy()),
        "__d",
    );
}

const HOST: &str = "https://example.test";

fn key(owner: &str, repo: &str, commit: &str, depth: u64) -> StoreKey {
    StoreKey::new(
        Path::new("/tmp/octocode-store-root"),
        owner,
        repo,
        HOST,
        depth,
        commit,
    )
}

/// Depth 1 (the default) shares one store per repository, whatever the
/// commit; deeper history keeps a store per commit and depth, because a
/// shallow boundary belongs to the whole repository.
#[test]
fn the_default_depth_shares_one_store_per_repository() {
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    assert_eq!(key("o", "r", &a, 1).dir(1), key("o", "r", &b, 1).dir(1));
    // Owner and repository names are case-insensitive, like the checkouts'.
    assert_eq!(key("O", "R", &a, 1).dir(1), key("o", "r", &a, 1).dir(1));
    assert_ne!(key("o", "r", &a, 1).dir(1), key("o", "other", &a, 1).dir(1));
    assert_ne!(key("o", "r", &a, 1).dir(1), key("o", "r", &a, 1).dir(2));
    let shared = key("o", "r", &a, 1).dir(1);
    assert!(shared.starts_with(Path::new("/tmp/octocode-store-root/clone-store/o")));
    let name = shared.file_name().expect("name").to_string_lossy();
    assert!(
        name.starts_with("r__host_") && name.ends_with("__g1.git"),
        "{name}"
    );
    assert!(!name.contains(&a) && !name.contains("__d"), "{name}");
    // The store never lies under the readable clones root.
    assert!(!shared.starts_with(clones_root(Path::new("/tmp/octocode-store-root"))));
}

#[test]
fn deeper_history_keeps_a_store_per_commit_and_depth() {
    let a = "a".repeat(40);
    let b = "b".repeat(40);
    let deep = key("o", "r", &a, 3).dir(1);
    assert_ne!(deep, key("o", "r", &a, 1).dir(1));
    assert_ne!(deep, key("o", "r", &b, 3).dir(1));
    assert_ne!(deep, key("o", "r", &a, 4).dir(1));
    // Case of the commit does not matter.
    assert_eq!(deep, key("o", "r", &a.to_ascii_uppercase(), 3).dir(1));
    let name = deep.file_name().expect("name").to_string_lossy();
    assert!(name.contains(&format!("r__{a}__d3__host_")), "{name}");
}

/// The current store is the highest generation that finished its init; an
/// unfinished or foreign directory is never it, and the next generation
/// follows the highest one that exists.
#[test]
fn the_current_store_is_the_highest_ready_generation() {
    let root = crate::tools::test_support::Fixture::new();
    let sha = "a".repeat(40);
    let key = StoreKey::new(&root.0, "o", "r", HOST, 1, &sha);
    assert_eq!(key.ready(), None);
    assert_eq!(key.next_dir(), key.dir(1));
    for generation in [1, 2, 3] {
        fs::create_dir_all(key.dir(generation)).expect("store");
    }
    // Another repository's store and a stray file are not generations.
    let other = StoreKey::new(&root.0, "o", "r2", HOST, 1, &sha);
    fs::create_dir_all(other.dir(9)).expect("other");
    fs::write(key.dir(1).with_extension("txt"), "").expect("stray");
    assert_eq!(key.generations(), [1, 2, 3]);
    assert_eq!(key.ready(), None, "no init finished");
    mark_ready(&key.dir(2)).expect("ready");
    assert_eq!(key.ready(), Some(key.dir(2)));
    assert_eq!(key.next_dir(), key.dir(4));
    mark_ready(&key.dir(1)).expect("ready");
    assert_eq!(key.ready(), Some(key.dir(2)), "the highest ready one");
    // One lock anchor spans every generation.
    assert!(!key.anchor().to_string_lossy().contains("__g"));
}

/// A worktree's `.git` is a file: its completion stamp lives in the git
/// directory it names, and is read back from there.
#[test]
fn the_completion_stamp_lives_in_the_checkouts_git_directory() {
    let root = crate::tools::test_support::Fixture::new();
    let admin = root.0.join("store/worktrees/w");
    fs::create_dir_all(&admin).expect("admin");
    let worktree = root.0.join("worktree");
    fs::create_dir_all(&worktree).expect("worktree");
    fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .expect("gitfile");
    assert_eq!(completed_at(&worktree), None);
    let at = record_completion(&worktree).expect("stamp");
    assert!(admin.join(COMPLETED_AT_FILE).is_file());
    assert_eq!(completed_at(&worktree), Some(at));

    // A standalone checkout keeps it in `.git/`.
    let standalone = root.0.join("standalone");
    fs::create_dir_all(standalone.join(".git")).expect("git dir");
    let at = record_completion(&standalone).expect("stamp");
    assert!(standalone.join(".git").join(COMPLETED_AT_FILE).is_file());
    assert_eq!(completed_at(&standalone), Some(at));
}

/// A write under the clones root that the filesystem refuses carries the
/// write recovery (a writable home with free space); a read's failure keeps
/// the read-side wording alone.
#[test]
fn a_clone_write_the_filesystem_refuses_carries_the_write_recovery() {
    let directory = tempfile::tempdir().expect("scratch");
    let blocker = directory.path().join("clones");
    fs::write(&blocker, b"a file where the clones root should be").expect("blocker");
    let destination = blocker.join("o").join("r").join("checkout");
    let error = promote(
        &directory.path().join("stage"),
        &destination,
        |_, _| Ok(()),
        || Ok(()),
    )
    .expect_err("no folder can be made under a file");
    assert_eq!(error.code, ErrorCode::FileAccessFailed);
    assert!(
        error
            .hints
            .iter()
            .any(|hint| hint == crate::response::rows::WRITE_ACCESS_HINT),
        "{error:?}"
    );
    let read = storage_read_io(io::Error::from(io::ErrorKind::NotFound));
    assert!(read.hints.is_empty(), "{read:?}");
}
