//! The `include` globs of the GitHub tools: the local walks' one glob
//! dialect ([`octocode_engine::search::glob`]), compiled by the same code,
//! over paths relative to a base (a listing's `path`, else the repository
//! root). A glob without a `/` matches a name at any depth; one with a `/`
//! matches the path below the base.
use octocode_engine::search::glob::Globs;
use std::path::Path;

/// A base no repository path names: the globs compile against it and every
/// candidate is joined to it, so no file system is read.
const BASE: &str = "/octocode-include-base";

/// `include` globs (ORed) over base-relative paths.
pub(crate) struct IncludeGlobs(Globs);

impl IncludeGlobs {
    /// The compiled globs; the error names the glob that does not compile.
    pub(crate) fn new<'a>(globs: impl IntoIterator<Item = &'a str>) -> Result<Self, String> {
        let globs = globs.into_iter().map(str::to_owned).collect::<Vec<_>>();
        octocode_engine::search::glob::compile(Path::new(BASE), &globs, &[])
            .map(Self)
            .map_err(|error| error.to_string())
    }

    /// Whether a glob keeps `relative` (a `/`-separated path below the base).
    pub(crate) fn matches(&self, relative: &str, is_dir: bool) -> bool {
        self.0.includes(&Path::new(BASE).join(relative), is_dir)
    }
}

/// The glob that keeps exactly the entry at `path` (below the base) and no
/// other: its characters read literally, and a path of one segment, which
/// would name an entry at any depth, is anchored with a leading `/` (so is
/// one a leading `!` or `#` would turn into a negation or a comment).
pub(crate) fn exact(path: &str) -> String {
    let literal = octocode_engine::search::glob::literal(path);
    if literal.contains('/') && !literal.starts_with(['!', '#']) {
        literal
    } else {
        format!("/{literal}")
    }
}

#[cfg(test)]
pub(crate) mod tests;
