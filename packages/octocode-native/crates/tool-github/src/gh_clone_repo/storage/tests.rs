use super::*;

#[test]
fn checkout_destinations_ignore_case() {
    let home = Path::new("/tmp/octocode-clones-root");
    let sha = "ABCDEF0123456789ABCDEF0123456789ABCDEF01";
    let entry =
        |owner, repo, commit| clone_dir(home, owner, repo, commit, None, "https://example.test");
    let lower = sha.to_ascii_lowercase();
    assert_eq!(entry("o", "r", sha), entry("o", "r", &lower));
    // Owner/repo are case-insensitive on GitHub: one checkout.
    assert_eq!(entry("Octo", "Repo", sha), entry("octo", "repo", sha));
    assert!(entry("o", "r", sha).starts_with(home.join("clones/o/r")));
}

/// Each sparse set keys its own entry, apart from the full checkout's.
#[test]
fn sparse_sets_key_their_own_entries() {
    let home = Path::new("/tmp/octocode-clones-root");
    let sha = "c".repeat(40);
    let entry = |sparse| clone_dir(home, "o", "r", &sha, sparse, HOST);
    assert_ne!(entry(None), entry(Some("src")));
    assert_ne!(entry(Some("src")), entry(Some("tests")));
}

/// Checkouts persist across releases: an entry is named exactly as an
/// earlier build named a row's that requested no history depth
/// (`<commit>__full[__sp_<scope>]__host_<host>`), so a checkout it
/// published is found again rather than cloned beside itself.
#[test]
fn an_entry_keeps_the_name_an_earlier_build_gave_it() {
    let home = Path::new("/tmp/octocode-clones-root");
    let sha = "c".repeat(40);
    let repository = home.join("clones/o/r");
    let host = hash(HOST, 16);
    assert_eq!(
        clone_dir(home, "o", "r", &sha, None, HOST),
        repository.join(format!("{sha}__full__host_{host}"))
    );
    assert_eq!(
        clone_dir(home, "o", "r", &sha, Some("src"), HOST),
        repository.join(format!("{sha}__full__sp_{}__host_{host}", hash("src", 6)))
    );
}

const HOST: &str = "https://example.test";

#[test]
fn store_identity_is_the_host_and_case_insensitive_repository() {
    let home = Path::new("/tmp/octocode-store-root");
    let store = |owner, repo, host| StoreKey::new(home, owner, repo, host).dir(1);
    assert_eq!(store("o", "r", HOST), store("O", "R", HOST));
    assert_ne!(store("o", "r", HOST), store("o", "r2", HOST));
    assert_ne!(
        store("o", "r", HOST),
        store("o", "r", "https://elsewhere.test")
    );
    assert!(!store("o", "r", HOST).starts_with(clones_root(home)));
}

/// The current store is the highest generation that finished its init; an
/// unfinished or foreign directory is never it, and the next generation
/// follows the highest one that exists.
#[test]
fn the_current_store_is_the_highest_ready_generation() {
    let root = octocode_runtime_base::test_support::Fixture::new();
    let key = StoreKey::new(&root.0, "o", "r", HOST);
    assert_eq!(key.ready(), None);
    assert_eq!(key.next_dir(), key.dir(1));
    for generation in [1, 2, 3] {
        fs::create_dir_all(key.dir(generation)).expect("store");
    }
    // Another repository's store and a stray file are not generations.
    let other = StoreKey::new(&root.0, "o", "r2", HOST);
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
    let root = octocode_runtime_base::test_support::Fixture::new();
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
            .any(|hint| hint == octocode_runtime_base::response::rows::WRITE_ACCESS_HINT),
        "{error:?}"
    );
    let read = storage_read_io(io::Error::from(io::ErrorKind::NotFound));
    assert!(read.hints.is_empty(), "{read:?}");
}

/// A recorded clone time reads to the second, as every other time: one an
/// earlier build recorded in milliseconds included.
#[test]
fn a_clone_time_reads_to_the_second() {
    assert_eq!(
        to_the_second("2026-11-23T07:09:16.158Z"),
        "2026-11-23T07:09:16Z"
    );
    assert_eq!(
        to_the_second("2026-11-23T07:09:16Z"),
        "2026-11-23T07:09:16Z"
    );
}
