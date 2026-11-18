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
