//! The one glob dialect of every local walk: gitignore glob semantics, as the
//! `ignore` crate compiles them. `*` and `?` never cross `/`, `**` spans
//! directories, `{a,b}` alternates and `[...]` is a class. A glob without a
//! `/` matches an entry's name at any depth; one with a `/` matches its path
//! below the root, and a trailing `/` matches directories only. A glob that
//! does not compile is an [`ErrorKind::InvalidGlob`] error, never a pattern
//! silently dropped.

use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::overrides::{Override, OverrideBuilder};

use crate::error::{Error, ErrorKind};

/// The characters that make a pattern a glob rather than a literal path or
/// name: every reading of an `include` entry tests against this one set.
pub const GLOB_META: [char; 4] = ['*', '?', '[', '{'];

/// Whether `pattern` is a bare word: no `/` and no [`GLOB_META`] character.
/// An `include` entry that is one names a file or directory by a part of its
/// name rather than a path or a glob.
pub fn is_bare_word(pattern: &str) -> bool {
    !pattern.contains('/') && !pattern.contains(GLOB_META)
}

/// A request's `include` and `exclude` globs, compiled once for a walk rooted
/// at `root`.
#[derive(Clone, Debug)]
pub struct Globs {
    include: Option<Gitignore>,
    exclude: Option<Gitignore>,
    overrides: Override,
}

/// Compile `include` + `exclude` against `root`. `!x` in `exclude` means the
/// same as `x`.
pub fn compile(root: &Path, include: &[String], exclude: &[String]) -> Result<Globs, Error> {
    let mut overrides = OverrideBuilder::new(root);
    for glob in include {
        overrides
            .add(glob)
            .map_err(|error| invalid("include", glob, &error))?;
    }
    let exclude: Vec<&str> = exclude
        .iter()
        .map(|glob| glob.strip_prefix('!').unwrap_or(glob))
        .collect();
    for glob in &exclude {
        overrides
            .add(&format!("!{glob}"))
            .map_err(|error| invalid("exclude", glob, &error))?;
    }
    Ok(Globs {
        include: set(root, include.iter().map(String::as_str), "include")?,
        exclude: set(root, exclude.into_iter(), "exclude")?,
        overrides: overrides
            .build()
            .map_err(|error| Error::with_kind(ErrorKind::InvalidGlob, error.to_string()))?,
    })
}

/// One set of globs (`names`, one `languageGlobs` grammar) against `root`;
/// `None` when `globs` is empty. `label` names the field in an error.
pub fn set<'a>(
    root: &Path,
    globs: impl Iterator<Item = &'a str>,
    label: &str,
) -> Result<Option<Gitignore>, Error> {
    let mut builder = GitignoreBuilder::new(root);
    builder.allow_unclosed_class(false);
    let mut any = false;
    for glob in globs {
        any = true;
        builder
            .add_line(None, glob)
            .map_err(|error| invalid(label, glob, &error))?;
    }
    if !any {
        return Ok(None);
    }
    builder
        .build()
        .map(Some)
        .map_err(|error| Error::with_kind(ErrorKind::InvalidGlob, error.to_string()))
}

/// Whether one compiled set matches `path` (under the set's root).
pub fn matches(set: &Gitignore, path: &Path, is_dir: bool) -> bool {
    set.matched(path, is_dir).is_ignore()
}

fn invalid(label: &str, glob: &str, error: &ignore::Error) -> Error {
    Error::with_kind(
        ErrorKind::InvalidGlob,
        format!("invalid {label} glob '{glob}': {error}"),
    )
}

impl Globs {
    /// The override set an `ignore` walk applies: a file must match an
    /// include glob when there are any, and no exclude glob.
    pub fn overrides(&self) -> Override {
        self.overrides.clone()
    }

    /// Whether `path` matches an include glob (true when there are none).
    pub fn includes(&self, path: &Path, is_dir: bool) -> bool {
        self.include
            .as_ref()
            .is_none_or(|include| matches(include, path, is_dir))
    }

    /// Whether `path` matches an exclude glob.
    pub fn excludes(&self, path: &Path, is_dir: bool) -> bool {
        self.exclude
            .as_ref()
            .is_some_and(|exclude| matches(exclude, path, is_dir))
    }

    /// Whether neither glob list was given.
    pub fn is_empty(&self) -> bool {
        self.include.is_none() && self.exclude.is_none()
    }
}

/// Which entries a walk left out could hold a path its `include` globs
/// match, so a disclosure of skipped paths names only those: a skipped file
/// that an include glob matches, or a skipped directory below which one
/// could. Unsure answers (a glob it cannot split by directory) are yes.
#[derive(Clone, Debug, Default)]
pub struct IncludeScope {
    /// One entry per include glob: its directory segments below the root,
    /// or `None` for a glob that matches a name at any depth.
    globs: Vec<Option<Vec<String>>>,
    files: Option<Gitignore>,
}

impl IncludeScope {
    /// The scope of `include` (globs as the walk compiles them) under `root`;
    /// no globs reach everything.
    pub fn new(root: &Path, include: &[String]) -> Self {
        let Ok(files) = set(root, include.iter().map(String::as_str), "include") else {
            return Self::default();
        };
        let globs = include
            .iter()
            .map(|glob| {
                let anchored = glob.strip_prefix('/').unwrap_or(glob);
                let body = anchored.trim_end_matches('/');
                // gitignore: a glob with no `/` but a trailing one matches a
                // name at any depth.
                (anchored.len() != glob.len() || body.contains('/'))
                    .then(|| body.split('/').map(str::to_owned).collect())
            })
            .collect();
        Self { globs, files }
    }

    /// Whether `path` (under the root) could hold an included path: a file
    /// an include glob matches, or a directory below which one could match.
    pub fn reaches(&self, path: &Path, relative: &Path, is_dir: bool) -> bool {
        let Some(files) = &self.files else {
            return true;
        };
        if !is_dir {
            return matches(files, path, false);
        }
        let dir = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>();
        self.globs.iter().any(|glob| {
            let Some(segments) = glob else {
                return true;
            };
            for (at, name) in dir.iter().enumerate() {
                let Some(segment) = segments.get(at) else {
                    // The glob ends at or above this directory: nothing
                    // deeper matches it.
                    return false;
                };
                if segment.contains("**") {
                    return true;
                }
                // A segment that does not compile alone (a `{a/b}` split
                // at its `/`) may match: unsure is yes.
                let matched = match globset::GlobBuilder::new(segment)
                    .literal_separator(true)
                    .build()
                {
                    Ok(one) => one.compile_matcher().is_match(name.as_ref()),
                    Err(_) => true,
                };
                if !matched {
                    return false;
                }
            }
            dir.len() < segments.len()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_word_has_no_slash_and_no_glob_character() {
        for word in ["local_fetch", "main.rs", "", "a-b"] {
            assert!(is_bare_word(word), "{word:?}");
        }
        for pattern in ["src/api", "*.rs", "a?", "[ab]", "{a,b}", "src/**"] {
            assert!(!is_bare_word(pattern), "{pattern:?}");
        }
    }

    /// A skipped directory is in an include list's scope when a path below it
    /// could match a glob; a skipped file when a glob matches it.
    #[test]
    fn an_include_scope_reaches_only_directories_that_could_hold_a_match() {
        let root = Path::new("/r");
        let scope = |include: &[&str]| {
            IncludeScope::new(
                root,
                &include
                    .iter()
                    .map(|glob| (*glob).to_owned())
                    .collect::<Vec<_>>(),
            )
        };
        let dir = |scope: &IncludeScope, relative: &str| {
            scope.reaches(&root.join(relative), Path::new(relative), true)
        };
        let agents = scope(&[".agents/**"]);
        assert!(dir(&agents, ".agents"));
        assert!(dir(&agents, ".agents/skills/deep"));
        assert!(!dir(&agents, "packages/repos/asm"));
        assert!(!dir(&agents, "releases"));
        let anchored = scope(&["/src/*/lib.rs", "docs/a.md"]);
        assert!(dir(&anchored, "src"));
        assert!(dir(&anchored, "src/x"));
        assert!(!dir(&anchored, "src/x/y"));
        assert!(dir(&anchored, "docs"));
        assert!(!dir(&anchored, "docs/a.md"));
        assert!(!dir(&anchored, "lib"));
        for anywhere in [&["*.rs"][..], &["build/"], &["**/gen/*.ts"], &[]] {
            assert!(dir(&scope(anywhere), "any/deep/dir"), "{anywhere:?}");
        }
        let files = scope(&["*.go"]);
        assert!(files.reaches(&root.join("a/b.go"), Path::new("a/b.go"), false));
        assert!(!files.reaches(&root.join("a/b.ts"), Path::new("a/b.ts"), false));
    }

    fn globs(include: &[&str], exclude: &[&str]) -> Result<Globs, Error> {
        let owned = |list: &[&str]| {
            list.iter()
                .map(|glob| (*glob).to_owned())
                .collect::<Vec<_>>()
        };
        compile(Path::new("/r"), &owned(include), &owned(exclude))
    }

    #[test]
    fn a_slash_free_glob_matches_a_name_at_any_depth_and_star_never_crosses_a_slash() {
        let set = globs(&["*.ts", "src/*.rs"], &[]).expect("compiles");
        assert!(set.includes(Path::new("/r/a/b/c.ts"), false));
        assert!(set.includes(Path::new("/r/src/lib.rs"), false));
        assert!(!set.includes(Path::new("/r/src/deep/lib.rs"), false));
        assert!(!set.includes(Path::new("/r/lib.rs"), false));
    }

    #[test]
    fn braces_classes_and_double_stars_follow_gitignore() {
        let set = globs(&["packages/{react,react-dom}/src/**", "*.[jt]s"], &[]).expect("compiles");
        assert!(set.includes(Path::new("/r/packages/react/src/a/b.js"), false));
        assert!(set.includes(Path::new("/r/packages/react-dom/src/x.js"), false));
        assert!(!set.includes(Path::new("/r/packages/scheduler/src/x.c"), false));
        assert!(set.includes(Path::new("/r/deep/a.ts"), false));
        assert!(!set.includes(Path::new("/r/deep/a.cs"), false));
    }

    #[test]
    fn excludes_skip_names_paths_and_directories() {
        let set = globs(&[], &["vendor", "!*.min.js", "src/gen/**"]).expect("compiles");
        assert!(set.excludes(Path::new("/r/a/vendor"), true));
        assert!(set.excludes(Path::new("/r/a/b.min.js"), false));
        assert!(set.excludes(Path::new("/r/src/gen/deep/b.ts"), false));
        assert!(!set.excludes(Path::new("/r/src/a.ts"), false));
        assert!(set.includes(Path::new("/r/src/a.ts"), false));
    }

    #[test]
    fn an_invalid_glob_is_an_invalid_glob_error() {
        for (include, exclude) in [
            (&["[.ts"][..], &[][..]),
            (&[], &["{a,b"]),
            (&["a/**/[z-a]"], &[]),
        ] {
            let error = globs(include, exclude).expect_err("invalid");
            assert_eq!(*error.kind(), ErrorKind::InvalidGlob, "{error}");
        }
        let error = set(Path::new("/r"), ["["].into_iter(), "names").expect_err("invalid");
        assert!(
            error.reason.starts_with("invalid names glob '['"),
            "{error}"
        );
    }
}
