//! The whole scope of a graph build: the caller's `exclude` globs, the
//! `parserGlobs` and the walk flags ([`WalkScope`]), as one value every rebuild
//! lead writes into its `graphBuild` row, so no lead can carry part of it.
use super::walk_scope::WalkScope;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BuildScope {
    /// Path globs to skip, relative to the build root.
    pub exclude: Vec<String>,
    /// AST parsers by root-relative glob.
    pub language_globs: BTreeMap<String, Vec<String>>,
    pub walk: WalkScope,
}

impl BuildScope {
    pub(crate) fn of_options(options: &super::store::IngestOptions) -> Self {
        Self {
            exclude: options.exclude.clone(),
            language_globs: options.language_globs.clone(),
            walk: options.walk,
        }
    }

    pub(crate) fn of_scan(scan: &super::graph::GraphScan) -> Self {
        Self {
            exclude: scan.exclude.clone(),
            language_globs: scan.language_globs.clone(),
            walk: scan.walk,
        }
    }

    /// The scope a manifest's `scan` records.
    pub(crate) fn of_manifest(scan: &Value) -> Self {
        Self {
            exclude: scan["exclude"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            language_globs: serde_json::from_value(scan["languageGlobs"].clone())
                .unwrap_or_default(),
            walk: WalkScope::of_manifest(scan),
        }
    }

    /// Sets the scope's fields on a graphBuild retry `row`: `exclude` and
    /// `parserGlobs` when there are any, the walk flags when they differ from
    /// the default walk.
    pub(crate) fn write_row(&self, row: &mut Value) {
        if !self.exclude.is_empty() {
            row["exclude"] = json!(self.exclude);
        }
        if !self.language_globs.is_empty() {
            row["parserGlobs"] = json!(self.language_globs);
        }
        self.walk.write_row(row);
    }

    /// The scope for a build rooted at `dir` (root-relative) inside this
    /// scope's root: its root-relative globs are rebased onto `dir`.
    pub(crate) fn inside(&self, dir: &str) -> Self {
        self.rebased(|glob| inside(glob, dir))
    }

    /// The scope for a build rooted at `prefix` (the path from the new root
    /// down to this scope's root), a directory above this scope's root.
    pub(crate) fn above(&self, prefix: &Path) -> Self {
        let prefix = octocode_engine::text::slash_path(prefix);
        self.rebased(|glob| vec![above(glob, &prefix)])
    }

    fn rebased(&self, rebase: impl Fn(&str) -> Vec<String>) -> Self {
        Self {
            exclude: self.exclude.iter().flat_map(|glob| rebase(glob)).collect(),
            language_globs: self
                .language_globs
                .iter()
                .filter_map(|(language, globs)| {
                    let globs = globs
                        .iter()
                        .flat_map(|glob| rebase(glob))
                        .collect::<Vec<_>>();
                    (!globs.is_empty()).then(|| (language.clone(), globs))
                })
                .collect(),
            walk: self.walk,
        }
    }
}

/// A gitignore-style glob split into its negation, its body and whether it
/// names directories only. A body with a `/` other than a trailing one is
/// anchored at the build root.
struct Glob<'a> {
    negated: bool,
    directories: bool,
    anchored: bool,
    segments: Vec<&'a str>,
}

impl<'a> Glob<'a> {
    fn parse(glob: &'a str) -> Self {
        let (negated, body) = match glob.strip_prefix('!') {
            Some(rest) => (true, rest),
            None => (false, glob),
        };
        let (directories, body) = match body.strip_suffix('/') {
            Some(rest) => (true, rest),
            None => (false, body),
        };
        Self {
            negated,
            directories,
            anchored: body.contains('/'),
            segments: body.trim_start_matches('/').split('/').collect(),
        }
    }

    fn render(&self, segments: &[&str]) -> String {
        format!(
            "{}/{}{}",
            if self.negated { "!" } else { "" },
            segments.join("/"),
            if self.directories { "/" } else { "" }
        )
    }
}

/// `glob` of a build rooted at `dir` below this scope's root: an unanchored
/// glob matches at any depth and stays; an anchored one becomes every glob
/// that matches under `dir` what it matched there, none when it cannot
/// match under `dir`.
fn inside(glob: &str, dir: &str) -> Vec<String> {
    let parsed = Glob::parse(glob);
    if !parsed.anchored {
        return vec![glob.to_owned()];
    }
    let dir = dir.split('/').collect::<Vec<_>>();
    let mut globs = Vec::new();
    for rest in remainders(&parsed.segments, &dir) {
        let glob = parsed.render(if rest.is_empty() { &["**"] } else { &rest });
        if !globs.contains(&glob) {
            globs.push(glob);
        }
    }
    globs
}

/// What is left of `segments` once they have matched every part of `dir`:
/// one remainder per way they can match, empty when the glob names `dir` or
/// an ancestor of it (all below is matched). A `**` matches zero or more
/// parts, so it both stays (absorbing a part) and steps aside.
fn remainders<'a>(segments: &[&'a str], dir: &[&str]) -> Vec<Vec<&'a str>> {
    let Some((part, below)) = dir.split_first() else {
        return vec![segments.to_vec()];
    };
    match segments.split_first() {
        None => vec![Vec::new()],
        Some((&"**", after)) => {
            let mut found = remainders(segments, below);
            found.extend(remainders(after, dir));
            found
        }
        Some((segment, after)) if segment_matches(segment, part) => remainders(after, below),
        Some(_) => Vec::new(),
    }
}

/// `glob` of this scope's root as a build rooted `prefix` above it reads it.
fn above(glob: &str, prefix: &str) -> String {
    let parsed = Glob::parse(glob);
    if !parsed.anchored {
        return glob.to_owned();
    }
    let mut segments = prefix.split('/').collect::<Vec<_>>();
    segments.extend(&parsed.segments);
    parsed.render(&segments)
}

/// Whether the glob segment `pattern` matches the directory name `name`.
fn segment_matches(pattern: &str, name: &str) -> bool {
    let root = Path::new("/");
    match octocode_engine::search::glob::set(root, std::iter::once(pattern), "exclude") {
        Ok(Some(set)) => octocode_engine::search::glob::matches(&set, &root.join(name), false),
        // A glob that does not compile never matched in the build either.
        _ => false,
    }
}

#[cfg(test)]
mod tests;
