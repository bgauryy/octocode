//! Import-resolution context for the file graph: tsconfig/jsconfig path
//! aliases (`paths`, `baseUrl`, `extends`, solution `references`), workspace
//! package manifests (`exports`/`imports` subpath maps with condition
//! preference and build-output → source mapping), Python package roots, and
//! C/C++ include directories.
//!
//! Everything here is loaded once per graph build from configuration files
//! next to the scanned sources; resolution itself only probes the `known`
//! (parsed) file set, so a configured alias can never fabricate an edge to a
//! file the scan did not read.

use super::graph::{climb, dirname, join, join_within_root, normalize};
use super::js_paths::{BUILD_DIRS, is_declaration, probe_js, source_equivalent};
use crate::{policy::path::PathPolicy, security::ContentSecurity};
use octocode_engine::text::lexical;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

mod config;
mod loading;
mod manifests;
pub(super) use config::read_config_text;
use config::*;
use manifests::*;

/// Largest project manifest or config the graph reads.
const MAX_CONFIG_BYTES: usize = 1_000_000;
const MAX_EXTENDS_DEPTH: usize = 8;
const MAX_CONDITION_DEPTH: usize = 8;
const MAX_OUTER_CONFIG_LEVELS: usize = 6;
const MAX_COMPILE_COMMANDS_BYTES: usize = 32 * 1024 * 1024;

/// Export/import conditions in preference order: source-first so a workspace
/// package links to its sources instead of its build output.
const CONDITIONS: &[&str] = &[
    "source",
    "development",
    "import",
    "module",
    "node",
    "default",
    "require",
    "browser",
    "types",
    "typings",
];

/// Pattern keys (`./sub/*`, `#internal/*`, `@app/*`) with ordered targets.
type PatternTargets = Vec<(String, Vec<String>)>;

/// One compiled `compilerOptions.paths` entry, targets already rebased to
/// scan-root-relative form (a target may still hold the `*` placeholder).
#[derive(Clone, Debug)]
struct PathPattern {
    prefix: String,
    suffix: String,
    wildcard: bool,
    targets: Vec<String>,
}

/// Effective aliasing of one tsconfig/jsconfig after `extends`.
#[derive(Clone, Debug, Default)]
pub(crate) struct TsPaths {
    /// Scan-root-relative `baseUrl`; `None` when unset or outside the root.
    base_url: Option<String>,
    patterns: Vec<PathPattern>,
}

impl TsPaths {
    /// Exact match first, else the longest matching wildcard prefix.
    fn pattern(&self, spec: &str) -> Option<&PathPattern> {
        if let Some(exact) = self
            .patterns
            .iter()
            .find(|pattern| !pattern.wildcard && pattern.prefix == spec)
        {
            return Some(exact);
        }
        self.patterns
            .iter()
            .filter(|pattern| {
                pattern.wildcard
                    && spec.len() >= pattern.prefix.len() + pattern.suffix.len()
                    && spec.starts_with(&pattern.prefix)
                    && spec.ends_with(&pattern.suffix)
            })
            .max_by_key(|pattern| pattern.prefix.len())
    }

    /// Substituted, scan-root-relative candidates of the best-matching pattern.
    fn candidates(&self, spec: &str) -> Vec<String> {
        let Some(best) = self.pattern(spec) else {
            return Vec::new();
        };
        if !best.wildcard {
            return best.targets.clone();
        }
        let matched = &spec[best.prefix.len()..spec.len() - best.suffix.len()];
        best.targets
            .iter()
            .filter_map(|target| join_within_root(".", &target.replacen('*', matched, 1)))
            .collect()
    }

    /// Explicit aliases are internal unless every target names node_modules.
    /// A bare `*` fallback alone does not classify an import.
    fn claims(&self, spec: &str) -> Option<bool> {
        let pattern = self.pattern(spec)?;
        if pattern.wildcard && pattern.prefix.is_empty() {
            return None;
        }
        Some(pattern.targets.iter().any(|target| {
            !target
                .split('/')
                .any(|component| component == "node_modules")
        }))
    }
}

/// A `package.json` next to scanned sources.
#[derive(Clone, Debug, Default)]
struct Manifest {
    /// Scan-root-relative package directory (`.` for the root).
    dir: String,
    /// Root entry candidates (package-relative) in preference order.
    entries: Vec<String>,
    /// `exports` subpath keys other than `.` (`./sub`, `./*`, `./dir/`).
    exports: PatternTargets,
    /// `imports` keys (`#internal/*`).
    imports: PatternTargets,
}

/// Configuration-derived resolution state for one graph build.
#[derive(Debug, Default)]
pub(crate) struct ResolveContext {
    /// Named components of the canonical scan root, so a relative specifier
    /// that leaves the root and enters it again by name still normalises to a
    /// root-relative path.
    root_names: Vec<String>,
    /// Directory (scan-root-relative, `.` for the root) holding a
    /// tsconfig/jsconfig → its effective aliases. The nearest one governs.
    ts_configs: BTreeMap<String, Arc<TsPaths>>,
    /// Config above the scan root (bounded search) for scans of a subtree.
    outer_ts_config: Option<Arc<TsPaths>>,
    /// Workspace package name → manifest.
    packages: BTreeMap<String, Arc<Manifest>>,
    /// Package directory → manifest (for `#` imports of the importer).
    manifests: BTreeMap<String, Arc<Manifest>>,
    /// Python import roots: `.`, `src`, and project roots (pyproject/setup).
    python_roots: Vec<String>,
    /// Dotted package name of the scan root itself when the root is a
    /// package (`django` for a scan of `python/django/`), so absolute
    /// imports naming it resolve inside the scan.
    python_package: Option<String>,
    /// C/C++ quoted-include search directories after the importer's own.
    include_dirs: Vec<String>,
    /// `-I` directories from compile_commands.json (also for `<...>`).
    system_include_dirs: Vec<String>,
    /// C-family file basename → root-relative paths, for includes written
    /// relative to an include root the build system passes (`-I deps/x`).
    headers_by_name: HashMap<String, Vec<String>>,
}

impl ResolveContext {
    /// `spec` joined onto the root-relative `dir`, normalised. A specifier
    /// that climbs above the scan root is followed on the absolute path: it
    /// counts when it re-enters the root, and not when it lands elsewhere.
    pub(crate) fn join_relative(&self, dir: &str, spec: &str) -> Option<String> {
        join_within_root(dir, spec).or_else(|| {
            let parts = climb(self.root_names.clone(), &format!("{dir}/{spec}"))?;
            let inside = parts.strip_prefix(self.root_names.as_slice())?;
            Some(inside.join("/"))
        })
    }

    /// The one known file whose path ends with `spec` (`a/b.h` matches
    /// `deps/x/include/a/b.h`); among several, the one sharing the longest
    /// directory prefix with `importer`, when that choice is unique.
    pub(crate) fn header_by_suffix(&self, spec: &str, importer: &str) -> Option<String> {
        let spec = spec.trim_start_matches("./");
        let name = spec.rsplit('/').next()?;
        let suffix = format!("/{spec}");
        let candidates = self
            .headers_by_name
            .get(name)?
            .iter()
            .filter(|path| path.as_str() == spec || path.ends_with(&suffix))
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [] => None,
            [only] => Some((*only).clone()),
            many => {
                let shared = |path: &str| {
                    path.split('/')
                        .zip(importer.split('/'))
                        .take_while(|(a, b)| a == b)
                        .count()
                };
                let best = many.iter().map(|p| shared(p)).max()?;
                let mut top = many.iter().filter(|p| shared(p) == best);
                let first = top.next()?;
                top.next().is_none().then(|| (*first).clone())
            }
        }
    }

    fn ts_config_for(&self, importer: &str) -> Option<&TsPaths> {
        let mut dir = dirname(importer);
        loop {
            if let Some(config) = self.ts_configs.get(dir) {
                return Some(config);
            }
            if dir == "." {
                return self.outer_ts_config.as_deref();
            }
            dir = dirname(dir);
        }
    }

    fn manifest_for(&self, importer: &str) -> Option<&Manifest> {
        let mut dir = dirname(importer);
        loop {
            if let Some(manifest) = self.manifests.get(dir) {
                return Some(manifest);
            }
            if dir == "." {
                return None;
            }
            dir = dirname(dir);
        }
    }

    /// Resolve a bare (non-relative) JavaScript/TypeScript specifier:
    /// package `imports` (`#…`), tsconfig `paths`, workspace packages, then
    /// `baseUrl`-relative modules.
    pub(crate) fn resolve_bare_js(
        &self,
        spec: &str,
        importer: &str,
        known: &BTreeSet<String>,
    ) -> Option<String> {
        if spec.starts_with('#')
            && let Some(manifest) = self.manifest_for(importer)
            && let Some(targets) = match_subpath(&manifest.imports, spec)
            && let Some(found) = resolve_targets(&manifest.dir, &targets, known)
        {
            return Some(found);
        }
        let config = self.ts_config_for(importer);
        if let Some(config) = config
            && let Some(found) = config
                .candidates(spec)
                .iter()
                .find_map(|candidate| probe_js_or_source(candidate, known))
        {
            return Some(found);
        }
        let (package, subpath) = split_package(spec)?;
        if let Some(manifest) = self.packages.get(package)
            && let Some(found) = resolve_package(manifest, subpath, known)
        {
            return Some(found);
        }
        let base_url = config.and_then(|config| config.base_url.as_deref())?;
        probe_js(&join_within_root(base_url, spec)?, known)
    }

    /// A bare specifier that names project code (alias prefix, workspace
    /// package, `#` import, `@/` or `~/` convention). Failing to link one is
    /// an internal coverage gap, never a third-party dependency.
    pub(crate) fn is_internal_bare_js(&self, spec: &str, importer: &str) -> bool {
        if let Some(internal) = self
            .ts_config_for(importer)
            .and_then(|config| config.claims(spec))
        {
            return internal;
        }
        if spec.starts_with('#') || spec.starts_with("@/") || spec.starts_with("~/") {
            return true;
        }
        split_package(spec).is_some_and(|(package, _)| self.packages.contains_key(package))
    }

    /// The scan-root-relative module for an absolute import that names the
    /// scan root's own package: `django.utils.text` → `utils.text` under a
    /// scan of `django/`, and `""` for the package itself.
    pub(crate) fn python_package_local<'s>(&self, spec: &'s str) -> Option<&'s str> {
        let package = self.python_package.as_deref()?;
        if spec == package {
            return Some("");
        }
        spec.strip_prefix(package)?.strip_prefix('.')
    }

    /// Python import roots ordered for `importer`: roots containing the
    /// importer (deepest first), then the scan root, then the rest.
    pub(crate) fn python_roots_for(&self, importer: &str) -> Vec<&str> {
        if self.python_roots.is_empty() {
            return vec!["."];
        }
        let mut containing = self
            .python_roots
            .iter()
            .filter(|root| {
                root.as_str() != "."
                    && importer
                        .strip_prefix(root.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            })
            .map(String::as_str)
            .collect::<Vec<_>>();
        containing.sort_by_key(|root| std::cmp::Reverse(root.len()));
        let mut ordered = containing;
        let mut included = ordered.iter().copied().collect::<HashSet<_>>();
        ordered.push(".");
        included.insert(".");
        for root in &self.python_roots {
            if included.insert(root.as_str()) {
                ordered.push(root);
            }
        }
        ordered
    }

    /// Search directories for a C/C++ include after the importer's own
    /// directory: quoted includes try the project root, `include/`, `src/`
    /// and compile_commands `-I` dirs; angle includes only the `-I` dirs.
    pub(crate) fn include_dirs(&self, quoted: bool) -> &[String] {
        if quoted {
            &self.include_dirs
        } else {
            &self.system_include_dirs
        }
    }

    #[cfg(test)]
    pub(crate) fn with_package(mut self, name: &str, dir: &str, entry: &str) -> Self {
        let manifest = Arc::new(Manifest {
            dir: dir.to_owned(),
            entries: vec![entry.to_owned()],
            ..Default::default()
        });
        self.packages.insert(name.to_owned(), Arc::clone(&manifest));
        self.manifests.insert(dir.to_owned(), manifest);
        self
    }
}

#[cfg(test)]
mod tests;
