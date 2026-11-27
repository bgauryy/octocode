//! A linked git worktree, the shape every `ghCloneRepo` checkout has: its
//! `.git` is a file pointing into the shared store, not a directory.

use super::*;
use std::path::{Path, PathBuf};
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

/// A detached linked worktree of a repository with `src/lib.rs`, outside
/// that repository, as `ghCloneRepo` publishes one.
fn worktree(root: &Path) -> PathBuf {
    let store = root.join("store");
    std::fs::create_dir_all(store.join("src")).expect("store");
    git(root, &["init", "--initial-branch=main", "store"]);
    std::fs::write(store.join("src/lib.rs"), "pub fn f() {}\n").expect("lib");
    git(&store, &["add", "."]);
    git(&store, &["commit", "-m", "one"]);
    let work = root.join("work");
    git(&store, &["worktree", "add", "--detach", "../work"]);
    assert!(work.join(".git").is_file(), "a worktree's .git is a file");
    work
}

/// A worktree root's `.git` is a pointer file, the repository's own
/// bookkeeping as `.git/` is: never listed, never counted as a hidden
/// entry, and counted as withheld only by a listing that would otherwise
/// show it (`includeHidden`), under the path policy's own reason for it.
#[test]
fn a_worktree_root_lists_no_git_file_and_says_so_when_it_shows_dot_entries() {
    let root = Fixture::new();
    let work = worktree(&root.0);
    for operation in ["tree", "files"] {
        for hidden in [false, true] {
            let out = run(
                &work,
                json!({"operation":operation,"path":work,"maxDepth":2,"includeHidden":hidden}),
            )
            .expect("listing");
            assert!(
                paths(&out).iter().any(|path| path.ends_with("src/lib.rs")),
                "{operation} {hidden}: {out}"
            );
            octocode_runtime_base::test_support::checks::assert_absent(&out["files"], ".git");
            let reason = octocode_runtime_base::policy::discovery::withheld::WithheldKind::GitFile
                .reason()
                .into_owned();
            assert_eq!(
                skipped_reasons(&out, "withheld"),
                if hidden {
                    vec![(reason, 1)]
                } else {
                    Vec::new()
                },
                "{operation} {hidden}: {out}"
            );
            assert_eq!(
                skipped_count(&out, "hidden"),
                0,
                "{operation} {hidden}: {out}"
            );
        }
    }
}
