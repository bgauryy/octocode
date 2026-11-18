//! Scope diagnostics for authored include and exclude globs.

/// Whether a glob has a `..` segment. Globs match paths under the listed or
/// searched `path`, so such a glob never matches anything there.
pub fn reaches_above(glob: &str) -> bool {
    glob.split(['/', '\\']).any(|segment| segment == "..")
}

/// The warning for the first include or exclude glob that reaches above
/// `path` (see [`reaches_above`]): why it never matches, and the repair.
pub fn parent_glob_warning(include: &[String], exclude: &[String]) -> Option<String> {
    let glob = include
        .iter()
        .chain(exclude)
        .find(|glob| reaches_above(glob))?;
    Some(format!(
        "Glob `{glob}` never matches: globs match paths under `path`, and a `..` segment reaches above it; widen `path` instead."
    ))
}

/// The empty result's fact when include/exclude left the walk no file: one
/// text for every tool that walks a scope (localSearch, astSearch).
pub const GLOB_MISS_HINT: &str = "Nothing searched: include/exclude matched no file. A glob with `/` matches from `path`, not the workspace root, so `src/*.rs` is `path`/src/*.rs: start it with `**/` to match at any depth. The related[] structureSearch request lists the names to fix the globs.";

#[cfg(test)]
mod tests;
