//! The gitignore-aware directory walker shared by text and structural search.

use std::path::Path;

use ignore::WalkBuilder;

pub(crate) struct WalkFlags {
    /// Walk dot-files and dot-directories.
    pub(crate) hidden: bool,
    /// Skip `.gitignore` and `.ignore` rules.
    pub(crate) no_ignore: bool,
    /// With `no_ignore`, also skip the global gitignore, `.git/info/exclude`,
    /// and ignore files above the root (ripgrep `--no-ignore`). Otherwise
    /// those sources stay on even when `no_ignore` is set.
    pub(crate) no_ignore_global: bool,
    /// Depth bound in `ignore` terms: the root is depth 0, its entries depth 1.
    pub(crate) max_depth: Option<usize>,
}

/// A walker over `root` with symlinks unfollowed; callers add overrides,
/// file types, ordering, and pruning.
pub(crate) fn walk_builder(root: &Path, flags: &WalkFlags) -> WalkBuilder {
    let global = !(flags.no_ignore && flags.no_ignore_global);
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(!flags.hidden)
        .ignore(!flags.no_ignore)
        .git_ignore(!flags.no_ignore)
        .git_global(global)
        .git_exclude(global)
        .parents(global)
        .follow_links(false)
        .max_depth(flags.max_depth);
    builder
}
