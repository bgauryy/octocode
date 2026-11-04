//! The ignore rules one directory holds: the files they come from, looked for
//! with stats or as its listing shows them.

use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use super::IGNORE_FILES;

/// The ignore rules of one directory.
pub(super) struct Level {
    /// `.ignore`: applies with or without a repository.
    pub(super) dot_ignore: Gitignore,
    /// `.gitignore`: applies inside a git repository only.
    pub(super) gitignore: Gitignore,
    /// The repository's `info/exclude`, for a directory holding `.git`.
    pub(super) exclude: Gitignore,
    /// The directory is a repository root ([`crate::search::repository`]).
    pub(super) has_git: bool,
}

#[cfg(test)]
thread_local! {
    /// Directories whose rule files this thread looked for with stats.
    pub(crate) static LEVELS_STATED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Level {
    pub(super) fn load(dir: &Path) -> Self {
        #[cfg(test)]
        LEVELS_STATED.with(|levels| levels.set(levels.get() + 1));
        let [gitignore, dot_ignore] = IGNORE_FILES.map(|name| rules_file(dir, &dir.join(name)));
        Self {
            dot_ignore,
            gitignore,
            exclude: git_exclude_file(dir)
                .map_or_else(Gitignore::empty, |file| rules_file(dir, &file)),
            has_git: crate::search::repository::is_root(dir),
        }
    }

    /// [`Self::load`] of a directory whose listing was just read: it shows
    /// which rule files exist, so only those are looked at.
    pub(super) fn listed(dir: &Path, listed: ListedRules) -> Self {
        let rules = |present: bool, name: &str| {
            if present {
                rules_file(dir, &dir.join(name))
            } else {
                Gitignore::empty()
            }
        };
        Self {
            dot_ignore: rules(listed.ignore, ".ignore"),
            gitignore: rules(listed.gitignore, ".gitignore"),
            exclude: git_exclude_file_when(listed.git, dir)
                .map_or_else(Gitignore::empty, |file| rules_file(dir, &file)),
            has_git: (listed.git || listed.jj) && crate::search::repository::is_root(dir),
        }
    }
}

fn git_exclude_file_when(git: bool, dir: &Path) -> Option<PathBuf> {
    git.then(|| git_exclude_file(dir)).flatten()
}

/// What a directory's listing shows of the files its rules come from: each
/// is a name that exists there. What it shows only says where to look, so a
/// link to a directory or a dangling one still reads as it always did.
#[derive(Clone, Copy, Debug, Default)]
pub struct ListedRules {
    pub gitignore: bool,
    pub ignore: bool,
    pub git: bool,
    pub jj: bool,
}

/// The rules of `file`, rooted at `dir`; empty when it is absent. A
/// malformed line is skipped and the remaining rules still apply.
fn rules_file(dir: &Path, file: &Path) -> Gitignore {
    if !file.is_file() {
        return Gitignore::empty();
    }
    let mut builder = GitignoreBuilder::new(dir);
    let _ = builder.add(file);
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// `info/exclude` of the repository whose `.git` sits in `dir`: a `.git`
/// directory, or a `.git` file pointing at a worktree's git directory (and
/// through its `commondir` at the shared one).
pub(super) fn git_exclude_file(dir: &Path) -> Option<PathBuf> {
    let dot_git = dir.join(".git");
    let metadata = std::fs::metadata(&dot_git).ok()?;
    let git_dir = if metadata.is_dir() {
        dot_git
    } else {
        let pointer = std::fs::read_to_string(&dot_git).ok()?;
        let target = pointer
            .lines()
            .find_map(|line| line.strip_prefix("gitdir:"))?
            .trim();
        let git_dir = dir.join(target);
        match std::fs::read_to_string(git_dir.join("commondir")) {
            Ok(common) => git_dir.join(common.trim()),
            Err(_) => git_dir,
        }
    };
    Some(git_dir.join("info").join("exclude"))
}
