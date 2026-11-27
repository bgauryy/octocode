//! What makes a directory a repository root: the one rule the walk's
//! `.gitignore` scope, graph freshness and workspace boundaries share.

use std::path::Path;

/// The entries whose presence makes a directory a repository root: a git
/// checkout (a `.git` directory, or a worktree's `.git` file) or a Jujutsu
/// one (`.jj`), whose `.gitignore` files apply like git's.
pub const MARKERS: [&str; 2] = [".git", ".jj"];

/// Whether `dir` is a repository root ([`MARKERS`]).
#[must_use]
pub fn is_root(dir: &Path) -> bool {
    MARKERS.iter().any(|marker| dir.join(marker).exists())
}

/// The nearest repository root at or above `path`.
#[must_use]
pub fn nearest(path: &Path) -> Option<&Path> {
    path.ancestors().find(|dir| is_root(dir))
}

#[cfg(test)]
mod tests;
