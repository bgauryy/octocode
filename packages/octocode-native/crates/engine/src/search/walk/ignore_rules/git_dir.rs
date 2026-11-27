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
mod tests;
