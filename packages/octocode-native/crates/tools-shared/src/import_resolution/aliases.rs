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

use super::js_paths::{BUILD_DIRS, is_declaration, probe_js, source_equivalent};
use super::paths::{
    climb, dirname, dirs_above_within_repository, join, join_within_root, normalize,
};
use octocode_engine::text::lexical;
use octocode_runtime_base::{policy::path::PathPolicy, security::ContentSecurity};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

pub mod config;
pub mod loading;
mod manifests;
pub mod python;
mod python_paths;
mod workspaces;
pub use config::read_config_text;
use config::*;
use manifests::*;
use python::PythonModules;

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
    /// A target of the pattern lies above the scan root: what it matches is
    /// project code the scan does not hold.
    outside_root: bool,
}

/// A `baseUrl` outside the scan root (`app/` for a scan of `app/src`).
#[derive(Clone, Debug)]
struct OuterBase {
    dir: PathBuf,
    /// The scan root below it, when it lies above the root (`src`).
    root_below: Option<String>,
}

/// Effective aliasing of one tsconfig/jsconfig after `extends`, and what
/// its emit does with an import read only as a type.
#[derive(Clone, Debug, Default)]
pub(crate) struct TsPaths {
    /// Scan-root-relative `baseUrl`; `None` when unset or outside the root.
    base_url: Option<String>,
    /// The `baseUrl` when it lies outside the root.
    outer_base: Option<OuterBase>,
    patterns: Vec<PathPattern>,
    type_use_imports: TypeUseImports,
}

/// What a project's emit does with a TypeScript value import whose bindings
/// the file reads only as types, as the tsconfig governing the file says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TypeUseImports {
    /// Not settled: no tsconfig governs the file, decorator metadata
    /// (`emitDecoratorMetadata`) may keep it, or a solution's references
    /// disagree.
    #[default]
    Unknown,
    /// Kept: the project keeps value imports (`verbatimModuleSyntax`,
    /// `preserveValueImports`, `importsNotUsedAsValues` `preserve` or
    /// `error`), so the import loads with the file.
    Kept,
    /// Elided by TypeScript's and esbuild's emit: the import loads nothing.
    Elided,
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

    /// Explicit aliases are internal unless every target names node_modules;
    /// one with a target above the scan root names code beyond it. A bare `*`
    /// fallback alone does not classify an import.
    fn claims(&self, spec: &str) -> Option<BareTarget> {
        let pattern = self.pattern(spec)?;
        if pattern.wildcard && pattern.prefix.is_empty() {
            return None;
        }
        Some(if pattern.outside_root {
            BareTarget::OutsideRoot
        } else if pattern.targets.iter().any(|target| {
            !target
                .split('/')
                .any(|component| component == "node_modules")
        }) {
            BareTarget::Internal
        } else {
            BareTarget::External
        })
    }
}

/// What a bare (non-relative) specifier the resolution did not link names,
/// as the project's configuration places it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BareTarget {
    /// Nothing of the project: a third-party package or a builtin.
    External,
    /// Project code the scan root holds, or no configuration says where.
    Internal,
    /// Project code beyond the scan root: a workspace sibling or path
    /// dependency outside it, an alias target or `baseUrl` module above it.
    OutsideRoot,
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
    /// The manifest has an `exports` field: Node lets the package's own
    /// files import it by its name (self-reference).
    self_reference: bool,
}

/// Configuration-derived resolution state for one graph build.
#[derive(Debug, Default)]
pub struct ResolveContext {
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
    /// Package directories a scanned workspace root lists: the workspace
    /// links each into `node_modules`, so its own files reach it by name.
    workspace_members: HashSet<String>,
    /// Package names local to the repository though not scanned: a sibling in
    /// an enclosing workspace, or a `workspace:`, `file:` or `link:` dependency.
    local_packages: HashSet<String>,
    /// Of `local_packages`, those whose directory lies outside the scan root.
    outside_packages: HashSet<String>,
    /// Package directories whose own name Node resolves to themselves: the
    /// first `node_modules/<name>` above is a link to the package (a
    /// workspace's or `npm link`'s).
    self_linked: HashSet<String>,
    /// A manifest above the scan that may list a workspace was not read (the
    /// path policy keeps it out): whether a package is a member of it is
    /// unknown.
    outer_workspace_unread: bool,
    /// `(baseUrl outside the scan, specifier)` → it names a file on disk.
    outer_base_hits: std::sync::Mutex<HashMap<(PathBuf, String), bool>>,
    /// Python import roots and what could fill them.
    python: PythonModules,
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
    pub fn join_relative(&self, dir: &str, spec: &str) -> Option<String> {
        join_within_root(dir, spec).or_else(|| {
            let parts = climb(self.root_names.clone(), &format!("{dir}/{spec}"))?;
            let inside = parts.strip_prefix(self.root_names.as_slice())?;
            Some(inside.join("/"))
        })
    }

    /// The one known file whose path ends with `spec` (`a/b.h` matches
    /// `deps/x/include/a/b.h`); among several, the one sharing the longest
    /// directory prefix with `importer`, when that choice is unique.
    pub fn header_by_suffix(&self, spec: &str, importer: &str) -> Option<String> {
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

    /// What emit does with a value import of `importer` read only as a
    /// type, by the tsconfig that governs it (the nearest one, or the one
    /// above a subtree scan).
    pub fn type_use_imports(&self, importer: &str) -> TypeUseImports {
        self.ts_config_for(importer)
            .map_or(TypeUseImports::Unknown, |config| config.type_use_imports)
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
    pub fn resolve_bare_js(
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
            && self.named_from(manifest, package, importer)
            && let Some(found) = resolve_package(manifest, subpath, known)
        {
            return Some(found);
        }
        let config = config?;
        if let Some(below) = config
            .outer_base
            .as_ref()
            .and_then(|base| base.root_below.as_deref())
            && let Some(local) = spec
                .strip_prefix(below)
                .and_then(|rest| rest.strip_prefix('/'))
            && let Some(found) = probe_js(local, known)
        {
            return Some(found);
        }
        probe_js(&join_within_root(config.base_url.as_deref()?, spec)?, known)
    }

    /// What a bare specifier names: project code (alias prefix, workspace
    /// package, `#` import, `@/` or `~/` convention, a module under a
    /// `baseUrl` above the scan) or a third-party package. Failing to link
    /// project code is an internal coverage gap, never a dependency; when the
    /// configuration places that code beyond the scan root, the scope is the
    /// gap ([`BareTarget::OutsideRoot`]).
    pub fn bare_target_js(&self, spec: &str, importer: &str) -> BareTarget {
        let config = self.ts_config_for(importer);
        if let Some(target) = config.and_then(|config| config.claims(spec)) {
            return target;
        }
        if spec.starts_with('#') || spec.starts_with("@/") || spec.starts_with("~/") {
            return BareTarget::Internal;
        }
        if let Some((package, _)) = split_package(spec) {
            if self.packages.get(package).is_some_and(|manifest| {
                self.named_from(manifest, package, importer) || self.outer_workspace_unread
            }) {
                return BareTarget::Internal;
            }
            if self.local_packages.contains(package) {
                return if self.outside_packages.contains(package) {
                    BareTarget::OutsideRoot
                } else {
                    BareTarget::Internal
                };
            }
        }
        match config.and_then(|config| config.outer_base.as_ref()) {
            Some(base) if self.outer_base_holds(&base.dir, spec) => {
                // Spelled through the scan root's own directory, the module
                // lies inside it; any other lies beyond.
                let inside = base.root_below.as_deref().is_some_and(|below| {
                    spec.strip_prefix(below)
                        .is_some_and(|rest| rest.starts_with('/'))
                });
                if inside {
                    BareTarget::Internal
                } else {
                    BareTarget::OutsideRoot
                }
            }
            _ => BareTarget::External,
        }
    }

    /// Whether `importer` reaches the package `name` (declared by `manifest`)
    /// by that name, as Node resolves it. A file of another package does,
    /// through `node_modules`. A file of the package itself does only through
    /// self-reference, which needs `exports`, or through the link a workspace
    /// (or `npm link`) makes for it in `node_modules`; otherwise its
    /// `node_modules` holds whatever copy of the name is installed, which is
    /// not this source. A workspace proves the link by listing the package,
    /// an install by the link itself.
    fn named_from(&self, manifest: &Manifest, name: &str, importer: &str) -> bool {
        let own = self
            .manifest_for(importer)
            .is_some_and(|nearest| nearest.dir == manifest.dir);
        !own || manifest.self_reference
            || self.workspace_members.contains(&manifest.dir)
            || self.local_packages.contains(name)
            || self.self_linked.contains(&manifest.dir)
    }

    /// Whether `spec` spelled from the `baseUrl` `dir` outside the scan names a
    /// module file on disk.
    fn outer_base_holds(&self, dir: &Path, spec: &str) -> bool {
        let mut hits = self
            .outer_base_hits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *hits
            .entry((dir.to_path_buf(), spec.to_owned()))
            .or_insert_with(|| super::js_paths::module_on_disk(&dir.join(spec)))
    }

    /// Search directories for a C/C++ include after the importer's own
    /// directory: quoted includes try the project root, `include/`, `src/`
    /// and compile_commands `-I` dirs; angle includes only the `-I` dirs.
    pub fn include_dirs(&self, quoted: bool) -> &[String] {
        if quoted {
            &self.include_dirs
        } else {
            &self.system_include_dirs
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn with_package(mut self, name: &str, dir: &str, entry: &str) -> Self {
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
