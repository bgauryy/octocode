//! The gitignore-aware directory walker shared by text and structural search.

use std::path::Path;

use ignore::WalkBuilder;
use ignore::overrides::{Override, OverrideBuilder};

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

/// Compile `include` + `exclude` into the gitignore-style override set every
/// walk applies, rooted at the search path so relative globs like
/// `src/**/*.ts` resolve as users expect. Each exclude is negated once (`x`
/// and `!x` both exclude `x`), so it drops files `include` would match.
pub(crate) fn build_overrides(
    root: &Path,
    include: &[String],
    exclude: &[String],
) -> Result<Override, String> {
    let mut builder = OverrideBuilder::new(root);
    for glob in include {
        builder
            .add(glob)
            .map_err(|err| format!("invalid include glob '{glob}': {err}"))?;
    }
    for glob in exclude {
        let negated = if glob.starts_with('!') {
            glob.to_owned()
        } else {
            format!("!{glob}")
        };
        builder
            .add(&negated)
            .map_err(|err| format!("invalid exclude glob '{glob}': {err}"))?;
    }
    builder
        .build()
        .map_err(|err| format!("failed to compile include/exclude globs: {err}"))
}
