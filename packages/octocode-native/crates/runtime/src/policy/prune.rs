//! The one directory-prune policy every local walk uses.
//!
//! Structure, text and AST walks (`structureSearch`, `localSearch`,
//! `astSearch`) prune the same names: dependency/build output,
//! caches, VCS metadata, and credential stores. Tool-config directories such
//! as `.github` or `.config` are not on the list: their workflows and scripts
//! are real text and syntax, and they are dot directories, so the walk
//! reaches them only with `includeHidden`, in every tool alike.
//!
//! A caller `exclude` glob skips more paths; it never replaces the defaults.
//! The engine walkers carry no default list of their own: they prune exactly
//! the names [`directories`] hands them, and report each pruned
//! directory so a listing or an empty result can disclose it.
//!
//! Names that are routinely real source (`output/`, `cache/`, `vendor/`,
//! `release/`, `tmp/`, …) are deliberately absent: pruning them silently hid
//! source files the syntax tools still saw (nlohmann's `detail/output/`,
//! Django's `core/cache/`). Sensitive paths stay blocked by the read policy
//! (`discovery::SENSITIVE_DIRECTORY_NAMES`) regardless of this list.

/// Dependency, build-output, cache, and VCS-metadata directories.
const GENERATED_DIRECTORY_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "dist",
    "build",
    "out",
    "target",
    "coverage",
    ".nyc_output",
    ".next",
    ".svelte-kit",
    ".turbo",
    ".cache",
    ".tmp",
    ".pytest_cache",
    ".tox",
    ".venv",
    ".mypy_cache",
    "__pycache__",
    ".gradle",
    ".m2",
    "DerivedData",
];

/// Credential stores: never walked.
const CREDENTIAL_DIRECTORY_NAMES: &[&str] = &[
    ".ssh",
    ".aws",
    ".docker",
    ".azure",
    ".kube",
    ".terraform",
    ".password-store",
];

/// The default directory names a walk prunes at any depth.
pub fn defaults() -> impl Iterator<Item = &'static str> {
    GENERATED_DIRECTORY_NAMES
        .iter()
        .chain(CREDENTIAL_DIRECTORY_NAMES)
        .copied()
}

/// The names a walk prunes: the defaults, or none when the
/// caller set `defaultExcludes: false`.
pub fn directories(defaults: bool) -> Vec<String> {
    if defaults {
        self::defaults().map(str::to_owned).collect()
    } else {
        Vec::new()
    }
}

/// The names a walk's default prune holds: [`directories`] less the
/// credential stores and less the names an `include` glob reaches into by
/// name (`build/*.rs`, `**/dist/**`, a bare `build` read as `*build*`): a
/// segment that matches the name, other than a bare wildcard (`*`, `**`),
/// asks for its files. The credential stores are left to the read policy,
/// which withholds them anyway, so a walk whose path-policy callback records
/// what it denies (`discovery::Withheld`) meets them there and can disclose
/// them instead of pruning them unseen.
///
/// The one list every local walk prunes by: an engine walk that roots it
/// itself takes these names (`default_exclude_dir`), a runtime callback
/// takes [`rooted`].
pub(crate) fn walk_names(defaults: bool, include: &[String]) -> Vec<String> {
    let segments = include
        .iter()
        .flat_map(|glob| glob.split(['/', '\\']))
        .filter(|segment| !segment.is_empty() && segment.chars().any(|c| c != '*'))
        .filter_map(|segment| globset::Glob::new(segment).ok())
        .map(|glob| glob.compile_matcher())
        .collect::<Vec<_>>();
    let named = |name: &str| segments.iter().any(|segment| segment.is_match(name));
    let mut names = directories(defaults);
    names.retain(|name| !CREDENTIAL_DIRECTORY_NAMES.contains(&name.as_str()) && !named(name));
    names
}

/// The default prune of a walk of `root` ([`walk_names`] bound to it,
/// `Prune::at`: inside a git work tree a listed name is pruned only where
/// git ignores it); `None` with `defaultExcludes:false`.
pub(crate) fn rooted(
    root: &std::path::Path,
    defaults: bool,
    include: &[String],
) -> Option<octocode_engine::search::walk::RootedPrune> {
    defaults.then(|| octocode_engine::search::walk::Prune::new(walk_names(true, include)).at(root))
}

/// The prune of a tool's own walk of one root, one with no path-policy
/// callback: the default prune ([`rooted`]: a listed name is pruned outside a
/// git work tree, and inside one only where git ignores it, as every local
/// tool walks, so a git-tracked `build/` is walked), plus the names it prunes
/// wherever they sit: the credential stores (no read policy meets them in
/// such a walk) and the tool's own `extra` names.
#[derive(Clone, Debug)]
pub(crate) struct OwnWalk {
    root: std::path::PathBuf,
    defaults: Option<octocode_engine::search::walk::RootedPrune>,
    names: octocode_engine::search::walk::Prune,
}

impl OwnWalk {
    pub(crate) fn new(root: &std::path::Path, extra: impl IntoIterator<Item = String>) -> Self {
        Self {
            root: root.to_path_buf(),
            defaults: rooted(root, true, &[]),
            names: octocode_engine::search::walk::Prune::new(
                CREDENTIAL_DIRECTORY_NAMES
                    .iter()
                    .map(|name| (*name).to_owned())
                    .chain(extra),
            ),
        }
    }

    /// Whether the walk leaves out `path`: a directory below the root.
    pub(crate) fn prunes(&self, path: &std::path::Path, is_dir: bool) -> bool {
        is_dir
            && path != self.root
            && (path.file_name().is_some_and(|name| self.names.names(name))
                || self
                    .defaults
                    .as_ref()
                    .is_some_and(|defaults| defaults.prunes(path, true)))
    }
}

/// `defaultExcludes` as the generated query types spell it (a plain flag or
/// the shared named type): true unless the caller explicitly turned the
/// default prune off.
pub(crate) trait DefaultsFlag {
    fn defaults(&self) -> bool;
}

impl DefaultsFlag for Option<bool> {
    fn defaults(&self) -> bool {
        self.unwrap_or(true)
    }
}

impl DefaultsFlag for Option<octocode_contract_types::DefaultExcludes> {
    fn defaults(&self) -> bool {
        self.as_ref().is_none_or(|flag| **flag)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn credential_stores_left_to_the_policy_are_all_withheld_by_it() {
        for name in super::CREDENTIAL_DIRECTORY_NAMES {
            assert!(
                crate::policy::discovery::SENSITIVE_DIRECTORY_NAMES.contains(name),
                "{name} would be walked"
            );
            assert!(!super::walk_names(true, &[]).iter().any(|kept| kept == name));
        }
    }

    use super::*;

    #[test]
    fn the_prune_list_keeps_tool_config_and_source_like_names_walkable() {
        let names = defaults().collect::<Vec<_>>();
        for name in [".git", ".ssh", "node_modules", "target"] {
            assert!(names.contains(&name), "{name}");
        }
        // Tool config is real text and syntax: `includeHidden` alone reaches it.
        for name in [".github", ".vscode", ".config", ".cargo", ".yarn", ".idea"] {
            assert!(
                !names.contains(&name),
                "{name} is walked with includeHidden"
            );
        }
        for name in [
            "vendor", "output", "cache", "release", "tmp", "secrets", "private",
        ] {
            assert!(!names.contains(&name), "{name} is routinely real source");
        }
    }

    #[test]
    fn an_include_naming_a_pruned_directory_lifts_only_that_prune() {
        let include = [
            "build/*.rs".to_owned(),
            "src/**".to_owned(),
            "*.ts".to_owned(),
            ".ssh/*".to_owned(),
        ];
        let names = walk_names(true, &include);
        assert!(!names.iter().any(|name| name == "build"), "{names:?}");
        assert!(names.iter().any(|name| name == "dist"), "{names:?}");
        // Credential stores are the read policy's, included or not.
        assert!(!names.iter().any(|name| name == ".ssh"), "{names:?}");
        // An explicit directory glob lifts only the directory it names.
        let bare = ["dist/**".to_owned()];
        let names = walk_names(true, &bare);
        assert!(!names.iter().any(|name| name == "dist"), "{names:?}");
        assert!(names.iter().any(|name| name == "build"), "{names:?}");
        assert_eq!(walk_names(false, &bare), Vec::<String>::new());
    }

    #[test]
    fn default_excludes_false_prunes_nothing() {
        assert!(directories(false).is_empty());
        assert_eq!(directories(true).len(), defaults().count());
    }
}
