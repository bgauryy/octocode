//! The one directory-prune policy every local walk uses.
//!
//! Two modes share one set of names:
//!
//! * [`PruneMode::SyntaxVisible`] — structure and AST walks (`structureSearch`,
//!   `astSearch`, `astRewrite`). Prunes dependency/build output, caches, VCS
//!   metadata, and credential stores; keeps tool-config directories such as
//!   `.github` or `.config`, whose workflows and scripts are real syntax.
//! * [`PruneMode::SearchSafe`] — text search (`localSearch`). Everything
//!   `SyntaxVisible` prunes plus the tool-config directories, where a text hit
//!   is noise or leaks editor/CI state.
//!
//! A caller `excludeDir` always adds names; it never replaces the defaults.
//! The engine walkers carry no default list of their own: they prune exactly
//! the names [`PruneMode::directories`] hands them.
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

/// Credential stores: never walked by any mode.
const CREDENTIAL_DIRECTORY_NAMES: &[&str] = &[
    ".ssh",
    ".aws",
    ".docker",
    ".azure",
    ".kube",
    ".terraform",
    "secrets",
    ".password-store",
];

/// Editor, CI, and package-manager configuration: pruned from text search only.
const TOOL_CONFIG_DIRECTORY_NAMES: &[&str] = &[
    ".github",
    ".vscode",
    ".devcontainer",
    ".config",
    ".cargo",
    ".yarn",
    ".idea",
    ".vs",
    ".history",
];

/// Which walk a prune list is for. See the module docs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PruneMode {
    SearchSafe,
    SyntaxVisible,
}

impl PruneMode {
    /// The default directory names this mode prunes at any depth.
    pub fn defaults(self) -> impl Iterator<Item = &'static str> {
        let tool_config: &[&str] = match self {
            Self::SearchSafe => TOOL_CONFIG_DIRECTORY_NAMES,
            Self::SyntaxVisible => &[],
        };
        GENERATED_DIRECTORY_NAMES
            .iter()
            .chain(CREDENTIAL_DIRECTORY_NAMES)
            .chain(tool_config)
            .copied()
    }

    /// The names a walk prunes: this mode's defaults (unless the caller set
    /// `defaultExcludes: false`) plus the caller's `excludeDir` (trailing
    /// separators trimmed), without duplicates.
    pub fn directories(self, exclude_dir: &[String], defaults: bool) -> Vec<String> {
        let mut names = if defaults {
            self.defaults().map(str::to_owned).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for name in exclude_dir {
            let name = name.trim_end_matches(['/', '\\']);
            if !name.is_empty() && !names.iter().any(|known| known == name) {
                names.push(name.to_owned());
            }
        }
        names
    }
}

/// `defaultExcludes` as the generated query types spell it (a plain flag or
/// the shared named type): true unless the caller explicitly turned the
/// default prune off.
pub trait DefaultsFlag {
    fn defaults(&self) -> bool;
}

impl DefaultsFlag for Option<bool> {
    fn defaults(&self) -> bool {
        self.unwrap_or(true)
    }
}

impl DefaultsFlag for Option<crate::contracts::tool_types::DefaultExcludes> {
    fn defaults(&self) -> bool {
        self.as_ref().is_none_or(|flag| **flag)
    }
}

impl DefaultsFlag for Option<crate::contracts::tool_types::ArDefaultExcludes> {
    fn defaults(&self) -> bool {
        self.as_ref().is_none_or(|flag| ***flag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_safe_extends_syntax_visible_with_tool_config_only() {
        let syntax = PruneMode::SyntaxVisible.defaults().collect::<Vec<_>>();
        let search = PruneMode::SearchSafe.defaults().collect::<Vec<_>>();
        assert!(syntax.iter().all(|name| search.contains(name)));
        let search_only = search
            .iter()
            .filter(|name| !syntax.contains(name))
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(search_only, TOOL_CONFIG_DIRECTORY_NAMES);
        for name in [".git", ".ssh", "secrets", "node_modules", "target"] {
            assert!(syntax.contains(&name), "{name}");
        }
        for name in ["vendor", "output", "cache", "release", "tmp"] {
            assert!(!search.contains(&name), "{name} is routinely real source");
        }
    }

    #[test]
    fn exclude_dir_adds_to_the_defaults_without_duplicates() {
        let names = PruneMode::SyntaxVisible.directories(
            &["generated/".to_owned(), "target".to_owned(), String::new()],
            true,
        );
        assert!(names.contains(&"node_modules".to_owned()));
        assert_eq!(names.iter().filter(|name| *name == "target").count(), 1);
        assert_eq!(names.last().map(String::as_str), Some("generated"));
        assert_eq!(names.len(), PruneMode::SyntaxVisible.defaults().count() + 1);
    }
}
