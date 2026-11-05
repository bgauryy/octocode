//! A query's name filters: globs, extensions and a regex over the entry's name.

use std::path::Path;

use ignore::gitignore::Gitignore;
use regex::Regex;

use super::filters::{has_extension, normalize_extensions};
use crate::error::{Error, ErrorKind};
use crate::search::glob;

/// A query's name filters, compiled once: `names` globs (see [`glob`]: an
/// entry is listed when one matches it), `extensions` (OR-combined, files
/// only) and a `regex` over the entry's name. An empty filter lists all.
pub struct NameFilter {
    names: Option<Gitignore>,
    extensions: Vec<String>,
    regex: Option<Regex>,
}

impl NameFilter {
    /// The filters of a walk rooted at `root`. A glob or regex that does not
    /// compile fails ([`ErrorKind::InvalidGlob`], [`ErrorKind::InvalidPattern`]):
    /// a filter is never silently dropped.
    pub fn new(
        root: &Path,
        names: &[String],
        extensions: &[String],
        regex: Option<&str>,
    ) -> Result<Self, Error> {
        // An extension never holds a glob or path character: `fs_*` or
        // `src/*.ts` is a name glob, which would match nothing here.
        if let Some(glob) = extensions
            .iter()
            .find(|value| value.contains(['*', '?', '[', '/', '\\']))
        {
            return Err(Error::with_kind(
                ErrorKind::InvalidPattern,
                format!(
                    "extensions take bare extensions such as \"rs\"; \"{glob}\" is a name glob: pass it as include"
                ),
            ));
        }
        Ok(Self {
            names: glob::set(root, names.iter().map(String::as_str), "names")?,
            extensions: normalize_extensions(extensions.to_vec()),
            regex: regex
                .map(|pattern| {
                    Regex::new(pattern).map_err(|err| {
                        Error::with_kind(
                            ErrorKind::InvalidPattern,
                            format!("Invalid regex for local filesystem query: {err}"),
                        )
                    })
                })
                .transpose()?,
        })
    }

    /// Whether the entry at `path` named `name` passes every filter. A
    /// directory never passes an extension filter (the walk still enters
    /// it).
    pub fn lists(&self, path: &Path, name: &str, is_dir: bool, is_file: bool) -> bool {
        self.names
            .as_ref()
            .is_none_or(|names| glob::matches(names, path, is_dir))
            && self.regex.as_ref().is_none_or(|regex| regex.is_match(name))
            && (self.extensions.is_empty() || (is_file && has_extension(name, &self.extensions)))
    }
}
