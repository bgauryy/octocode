//! Where a checkout keeps its git data: its own `.git` directory, or, for a
//! linked worktree, the directory its `.git` file names and the shared one
//! behind that.

use std::path::{Path, PathBuf};

/// The git directory of the checkout whose `.git` sits in `dir`: a `.git`
/// directory itself, or the directory a worktree's `.git` file names with
/// `gitdir:` (relative to `dir` unless absolute). `None` when `dir` holds
/// neither or the file names nothing.
///
/// For a linked worktree this is its private administrative directory
/// (`HEAD`, `index`, `config.worktree`, `info/sparse-checkout`); the objects,
/// shared config and refs live in [`common_git_dir`].
#[must_use]
pub fn checkout_git_dir(dir: &Path) -> Option<PathBuf> {
    let dot_git = dir.join(".git");
    let metadata = std::fs::metadata(&dot_git).ok()?;
    if metadata.is_dir() {
        return Some(dot_git);
    }
    let pointer = std::fs::read_to_string(&dot_git).ok()?;
    let target = pointer
        .lines()
        .find_map(|line| line.strip_prefix("gitdir:"))?
        .trim();
    Some(dir.join(target))
}

/// The git directory `git_dir` shares: the one its `commondir` file names
/// (relative to `git_dir` unless absolute) for a linked worktree, `git_dir`
/// itself otherwise.
#[must_use]
pub fn common_git_dir(git_dir: &Path) -> PathBuf {
    match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(common) => git_dir.join(common.trim()),
        Err(_) => git_dir.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_git_directory_is_its_own_common_directory() {
        let base = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(base.path().join(".git")).expect("git dir");
        let git_dir = checkout_git_dir(base.path()).expect("git dir");
        assert_eq!(git_dir, base.path().join(".git"));
        assert_eq!(common_git_dir(&git_dir), git_dir);
    }

    #[test]
    fn a_worktree_file_leads_to_its_directory_and_the_shared_one() {
        let base = tempfile::tempdir().expect("tempdir");
        let admin = base.path().join("store/worktrees/w");
        std::fs::create_dir_all(&admin).expect("admin");
        std::fs::write(admin.join("commondir"), "../..\n").expect("commondir");
        let checkout = base.path().join("checkout");
        std::fs::create_dir(&checkout).expect("checkout");
        // Absolute, as git writes it.
        std::fs::write(
            checkout.join(".git"),
            format!("gitdir: {}\n", admin.display()),
        )
        .expect("gitfile");
        let git_dir = checkout_git_dir(&checkout).expect("worktree dir");
        assert_eq!(git_dir, admin);
        assert_eq!(
            common_git_dir(&git_dir),
            admin.join("../.."),
            "commondir resolves relative to the worktree directory"
        );
        // A relative pointer resolves against the checkout.
        std::fs::write(checkout.join(".git"), "gitdir: ../store/worktrees/w\n").expect("relative");
        assert_eq!(
            checkout_git_dir(&checkout),
            Some(checkout.join("../store/worktrees/w"))
        );
    }

    #[test]
    fn no_git_entry_or_an_empty_pointer_has_no_directory() {
        let base = tempfile::tempdir().expect("tempdir");
        assert_eq!(checkout_git_dir(base.path()), None);
        std::fs::write(base.path().join(".git"), "not a pointer\n").expect("junk");
        assert_eq!(checkout_git_dir(base.path()), None);
    }
}
