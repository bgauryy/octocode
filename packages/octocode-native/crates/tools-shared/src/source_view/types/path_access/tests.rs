use super::*;
use octocode_runtime_base::test_support::{Fixture, workspace_policy};
use std::process::Command;

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Octocode Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Octocode Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A repository with `src/lib.rs` and `docs/guide.md`, and linked worktrees
/// of it: one sparse (`src` only), one full.
fn repository_with_worktrees(root: &Path) -> (PathBuf, PathBuf) {
    let main = root.join("main");
    fs::create_dir_all(main.join("src")).expect("src");
    fs::create_dir_all(main.join("docs")).expect("docs");
    git(root, &["init", "--initial-branch=main", "main"]);
    fs::write(main.join("src/lib.rs"), "\n").expect("file");
    fs::write(main.join("docs/guide.md"), "\n").expect("file");
    git(&main, &["add", "."]);
    git(&main, &["commit", "-m", "one"]);
    git(&main, &["config", "extensions.worktreeConfig", "true"]);
    let sparse = root.join("sparse");
    let full = root.join("full");
    git(&main, &["worktree", "add", "--detach", "../sparse"]);
    git(&main, &["worktree", "add", "--detach", "../full"]);
    git(&sparse, &["sparse-checkout", "set", "--cone", "src"]);
    (sparse, full)
}

fn missing(policy: &octocode_runtime_base::policy::path::PathPolicy, path: &Path) -> PathFailure {
    *PathAccess::validate_read(policy, path).expect_err("the file is not checked out")
}

/// A linked worktree keeps its sparse-checkout list and its
/// `core.sparseCheckout` in its own git directory (its `.git` is a file), not
/// in `.git/`: a file missing from it may still exist upstream.
#[test]
fn a_missing_file_in_a_sparse_linked_worktree_may_exist_upstream() {
    let root = Fixture::new();
    let (sparse, full) = repository_with_worktrees(&root.0);
    assert!(sparse.join(".git").is_file(), "a worktree's .git is a file");
    assert!(!sparse.join("docs/guide.md").exists());
    let policy = workspace_policy(&root.0);

    let failure = missing(&policy, &sparse.join("docs/guide.md"));
    assert!(failure.resource_missing, "{failure:?}");
    assert!(failure.sparse_checkout, "{failure:?}");

    // A full worktree of the same repository has no sparse scope.
    let failure = missing(&policy, &full.join("docs/absent.md"));
    assert!(failure.resource_missing, "{failure:?}");
    assert!(!failure.sparse_checkout, "{failure:?}");
}

/// A ghCloneRepo checkout that records its scope names it on a missing path
/// outside that scope, with the clone that checks the path out; a path
/// inside the scope is missing at the commit itself, and a checkout with no
/// record keeps only the bare sparse flag (its scope is unknown).
#[test]
fn a_recorded_clone_scope_rides_on_a_path_it_excludes() {
    let root = Fixture::new();
    let (sparse, _) = repository_with_worktrees(&root.0);
    let policy = workspace_policy(&root.0);
    let unrecorded = missing(&policy, &sparse.join("docs/guide.md"));
    assert!(
        unrecorded.sparse_checkout && unrecorded.clone_scope.is_none(),
        "{unrecorded:?}"
    );

    let git_dir = octocode_engine::search::walk::checkout_git_dir(&sparse).expect("git dir");
    crate::gh_git::clone_scope::write_record(
        &git_dir,
        &crate::gh_git::clone_scope::CloneRecord {
            owner: "o".into(),
            repo: "r".into(),
            commit_sha: "6ecc29b9c8c84d08a7e860f7f69c22b113b480ea".into(),
            paths: Some(vec!["src".into()]),
            root_files: true,
        },
    )
    .expect("record");
    let outside = missing(&policy, &sparse.join("docs/guide.md"));
    let scope = outside.clone_scope.as_ref().expect("recorded scope");
    assert_eq!(scope.paths(), ["src"], "{outside:?}");
    let resolved = outside.resolved.as_ref().expect("judged path");
    let reclone = scope.reclone(resolved).expect("re-clone");
    assert_eq!(
        reclone["input"]["queries"][0]["paths"],
        serde_json::json!(["src", "docs/guide.md"]),
        "{reclone}"
    );
    let inside = missing(&policy, &sparse.join("src/absent.rs"));
    assert!(inside.clone_scope.is_none(), "{inside:?}");
}

/// A read given a whole listing entry is corrected to the entry's own path
/// when that file exists; a file whose real name ends in `(…)` opens, and an
/// entry of an absent file is an ordinary missing path.
#[test]
fn a_whole_listing_entry_is_corrected_to_its_existing_path() {
    use octocode_runtime_base::response::groups::entry_text;
    let root = Fixture::new();
    fs::write(root.0.join("big.ts"), "x\n").expect("file");
    fs::write(root.0.join("notes (old)"), "y\n").expect("file");
    let policy = workspace_policy(&root.0);
    let fields = ["700".to_owned()];

    let failure = missing(&policy, &root.0.join(entry_text("big.ts", false, &fields)));
    assert!(failure.listing_entry, "{failure:?}");
    assert_eq!(
        failure.corrected.as_deref().map(Path::new),
        Some(root.0.join("big.ts").as_path())
    );

    assert!(PathAccess::validate_read(&policy, &root.0.join("notes (old)")).is_ok());

    let failure = missing(&policy, &root.0.join(entry_text("gone.ts", false, &fields)));
    assert!(
        failure.resource_missing && !failure.listing_entry,
        "{failure:?}"
    );
}
