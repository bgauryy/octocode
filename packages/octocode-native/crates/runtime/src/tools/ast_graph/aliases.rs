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

use super::graph::{dirname, join, join_within_root, normalize};
use crate::{policy::path::PathPolicy, security::ContentSecurity};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

/// Largest project manifest or config the graph reads.
const MAX_CONFIG_BYTES: usize = 1_000_000;
const MAX_EXTENDS_DEPTH: usize = 8;
const MAX_CONDITION_DEPTH: usize = 8;
const MAX_OUTER_CONFIG_LEVELS: usize = 6;
const MAX_PYTHON_ROOTS: usize = 64;
const MAX_INCLUDE_DIRS: usize = 64;
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
/// Package-relative build-output roots mapped back to sources.
const BUILD_DIRS: &[&str] = &[
    "dist",
    "build",
    "out",
    "lib",
    "esm",
    "cjs",
    "es",
    "types",
    "declarations",
];
/// Output flavor directories nested under a build root (`dist/esm/…`).
const FLAVOR_DIRS: &[&str] = &[
    "esm",
    "cjs",
    "es",
    "mjs",
    "lib",
    "types",
    "prod",
    "dev",
    "production",
    "development",
    "node",
    "browser",
    "module",
    "commonjs",
    "src",
];
const JS_EXTENSIONS: &[&str] = &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts"];
const DECLARATION_EXTENSIONS: &[&str] = &[".d.ts", ".d.mts", ".d.cts"];

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
    /// Substituted, scan-root-relative candidates of the best-matching pattern
    /// (exact match first, else the longest wildcard prefix — TypeScript's rule).
    fn candidates(&self, spec: &str) -> Vec<String> {
        if let Some(exact) = self
            .patterns
            .iter()
            .find(|pattern| !pattern.wildcard && pattern.prefix == spec)
        {
            return exact.targets.clone();
        }
        let best = self
            .patterns
            .iter()
            .filter(|pattern| {
                pattern.wildcard
                    && spec.len() >= pattern.prefix.len() + pattern.suffix.len()
                    && spec.starts_with(&pattern.prefix)
                    && spec.ends_with(&pattern.suffix)
            })
            .max_by_key(|pattern| pattern.prefix.len());
        let Some(best) = best else {
            return Vec::new();
        };
        let matched = &spec[best.prefix.len()..spec.len() - best.suffix.len()];
        best.targets
            .iter()
            .filter_map(|target| join_within_root(".", &target.replacen('*', matched, 1)))
            .collect()
    }

    /// Whether `spec` is claimed by a configured alias. A bare `*` pattern
    /// (empty prefix) claims everything, so it does not mark a specifier
    /// internal on its own.
    fn claims(&self, spec: &str) -> bool {
        self.patterns.iter().any(|pattern| {
            if pattern.wildcard {
                !pattern.prefix.is_empty() && spec.starts_with(&pattern.prefix)
            } else {
                pattern.prefix == spec
            }
        })
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
    pub(crate) fn load(
        root: &Path,
        known: &BTreeSet<String>,
        paths: &PathPolicy,
        security: &ContentSecurity,
    ) -> Self {
        let mut context = Self::default();
        let mut js_dirs = BTreeSet::from([".".to_owned()]);
        let mut py_dirs = BTreeSet::new();
        let mut all_dirs = BTreeSet::from([".".to_owned()]);
        let mut has_c = false;
        for file in known {
            let ext = file.rsplit_once('.').map_or("", |x| x.1);
            let is_js = matches!(
                ext,
                "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"
            );
            let is_py = matches!(ext, "py" | "pyi");
            has_c |= matches!(
                ext,
                "c" | "h" | "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" | "cu" | "cuh"
            );
            let mut dir = dirname(file);
            loop {
                if !all_dirs.insert(dir.to_owned()) && !is_js && !is_py {
                    break;
                }
                if is_js {
                    js_dirs.insert(dir.to_owned());
                }
                if is_py {
                    py_dirs.insert(dir.to_owned());
                }
                if dir == "." {
                    break;
                }
                dir = dirname(dir);
            }
        }
        let mut reader = ConfigReader {
            root,
            paths,
            security,
            cache: HashMap::new(),
        };
        context.load_ts_configs(&mut reader, &js_dirs);
        context.load_manifests(&reader, &all_dirs);
        context.load_outer_manifest(&reader);
        if !py_dirs.is_empty() {
            context.load_python_roots(root, &py_dirs);
        }
        if has_c {
            context.load_include_dirs(root, paths);
            for file in known {
                let ext = file.rsplit_once('.').map_or("", |x| x.1);
                if matches!(
                    ext,
                    "h" | "hh" | "hpp" | "hxx" | "cuh" | "inc" | "c" | "cc" | "cpp" | "cxx"
                ) {
                    let name = file.rsplit('/').next().unwrap_or(file);
                    context
                        .headers_by_name
                        .entry(name.to_owned())
                        .or_default()
                        .push(file.clone());
                }
            }
        }
        context
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

    fn load_ts_configs(&mut self, reader: &mut ConfigReader<'_>, js_dirs: &BTreeSet<String>) {
        for dir in js_dirs {
            let directory = reader.root.join(dir);
            if let Some(config) = config_file_in(&directory) {
                self.ts_configs
                    .insert(dir.clone(), Arc::new(reader.effective(&config)));
            }
        }
        if self.ts_configs.contains_key(".") {
            return;
        }
        // A scan of a package subtree (`packages/app/src`) still honors the
        // package's own tsconfig above the root; stop at the project boundary.
        for directory in reader
            .root
            .ancestors()
            .skip(1)
            .take(MAX_OUTER_CONFIG_LEVELS)
        {
            if let Some(config) = config_file_in(directory) {
                self.outer_ts_config = Some(Arc::new(reader.effective(&config)));
                return;
            }
            if directory.join("package.json").is_file() || directory.join(".git").exists() {
                return;
            }
        }
    }

    fn load_manifests(&mut self, reader: &ConfigReader<'_>, dirs: &BTreeSet<String>) {
        for dir in dirs {
            let Some(value) = reader.read_json(&reader.root.join(dir).join("package.json")) else {
                continue;
            };
            let manifest = Arc::new(Manifest::parse(dir, &value));
            if let Some(name) = value.get("name").and_then(Value::as_str) {
                self.packages.insert(name.to_owned(), Arc::clone(&manifest));
            }
            self.manifests.insert(dir.clone(), manifest);
        }
    }

    /// A scan of a package subtree (`packages/app/src`) still honors the
    /// package's own `package.json` above the root (`#` subpath imports,
    /// self-referencing exports), mirroring the outer tsconfig lookup: the
    /// first manifest within the bounded ancestor walk governs the scan root,
    /// its targets rebased onto the root; the walk stops at a `.git` boundary.
    fn load_outer_manifest(&mut self, reader: &ConfigReader<'_>) {
        if self.manifests.contains_key(".") {
            return;
        }
        for directory in reader
            .root
            .ancestors()
            .skip(1)
            .take(MAX_OUTER_CONFIG_LEVELS)
        {
            let file = directory.join("package.json");
            if file.is_file() {
                let (Some(value), Ok(offset)) =
                    (reader.read_json(&file), reader.root.strip_prefix(directory))
                else {
                    return;
                };
                let offset = offset.to_string_lossy().replace('\\', "/");
                let manifest = Arc::new(Manifest::parse(".", &value).rebased(&offset));
                if let Some(name) = value.get("name").and_then(Value::as_str) {
                    self.packages
                        .entry(name.to_owned())
                        .or_insert_with(|| Arc::clone(&manifest));
                }
                self.manifests.insert(".".to_owned(), manifest);
                return;
            }
            if directory.join(".git").exists() {
                return;
            }
        }
    }

    fn load_python_roots(&mut self, root: &Path, py_dirs: &BTreeSet<String>) {
        let mut roots = vec![".".to_owned()];
        if root.join("src").is_dir() {
            roots.push("src".to_owned());
        }
        for dir in py_dirs {
            if roots.len() >= MAX_PYTHON_ROOTS {
                break;
            }
            if dir == "." {
                continue;
            }
            let directory = root.join(dir);
            if ["pyproject.toml", "setup.py", "setup.cfg"]
                .iter()
                .any(|name| directory.join(name).is_file())
            {
                roots.push(dir.clone());
                let src = join(dir, "src");
                if directory.join("src").is_dir() && !roots.contains(&src) {
                    roots.push(src);
                }
            }
        }
        roots.truncate(MAX_PYTHON_ROOTS);
        self.python_roots = roots;
        self.python_package = enclosing_python_package(root);
    }

    fn load_include_dirs(&mut self, root: &Path, paths: &PathPolicy) {
        self.include_dirs = [".", "include", "src"]
            .into_iter()
            .filter(|dir| *dir == "." || root.join(dir).is_dir())
            .map(str::to_owned)
            .collect();
        let mut system = Vec::new();
        for candidate in ["compile_commands.json", "build/compile_commands.json"] {
            let file = root.join(candidate);
            if !file.is_file() {
                continue;
            }
            for dir in compile_command_include_dirs(root, &file, paths) {
                if system.len() >= MAX_INCLUDE_DIRS {
                    break;
                }
                if !system.contains(&dir) {
                    system.push(dir);
                }
            }
            break;
        }
        if root.join("include").is_dir() && !system.iter().any(|dir| dir == "include") {
            system.push("include".to_owned());
        }
        for dir in &system {
            if self.include_dirs.len() >= MAX_INCLUDE_DIRS {
                break;
            }
            if !self.include_dirs.contains(dir) {
                self.include_dirs.push(dir.clone());
            }
        }
        self.system_include_dirs = system;
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
        if spec.starts_with('#') || spec.starts_with("@/") || spec.starts_with("~/") {
            return true;
        }
        if self
            .ts_config_for(importer)
            .is_some_and(|config| config.claims(spec))
        {
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
            .filter(|root| root.as_str() != "." && importer.starts_with(&format!("{root}/")))
            .map(String::as_str)
            .collect::<Vec<_>>();
        containing.sort_by_key(|root| std::cmp::Reverse(root.len()));
        let mut ordered = containing;
        ordered.push(".");
        for root in &self.python_roots {
            if !ordered.contains(&root.as_str()) {
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

impl Manifest {
    /// Re-anchor package-relative targets of a manifest that sits `offset`
    /// above the scan root (`./src/x.ts` with offset `src` → `./x.ts`);
    /// targets outside the root are dropped.
    fn rebased(mut self, offset: &str) -> Self {
        let prefix = format!("{}/", offset.trim_matches('/'));
        let rebase = |targets: &mut Vec<String>| {
            targets.retain_mut(|target| {
                let relative = target.strip_prefix("./").unwrap_or(target);
                let Some(rest) = relative.strip_prefix(&prefix) else {
                    return false;
                };
                *target = format!("./{rest}");
                true
            });
        };
        rebase(&mut self.entries);
        for (_, targets) in self.exports.iter_mut().chain(self.imports.iter_mut()) {
            rebase(targets);
        }
        self.exports.retain(|(_, targets)| !targets.is_empty());
        self.imports.retain(|(_, targets)| !targets.is_empty());
        self
    }

    fn parse(dir: &str, value: &Value) -> Self {
        let mut manifest = Self {
            dir: dir.to_owned(),
            ..Default::default()
        };
        if let Some(exports) = value.get("exports") {
            match exports {
                Value::Object(map) if map.keys().any(|key| key.starts_with('.')) => {
                    for (key, target) in map {
                        if !key.starts_with('.') {
                            continue;
                        }
                        let targets = relative_targets(target);
                        if key == "." {
                            manifest.entries.extend(targets);
                        } else {
                            manifest.exports.push((key.clone(), targets));
                        }
                    }
                }
                other => manifest.entries.extend(relative_targets(other)),
            }
        }
        for field in ["source", "module", "main", "types", "typings"] {
            if let Some(target) = value.get(field).and_then(Value::as_str)
                && !manifest.entries.iter().any(|entry| entry == target)
            {
                manifest.entries.push(target.to_owned());
            }
        }
        if let Some(Value::Object(map)) = value.get("imports") {
            for (key, target) in map {
                if key.starts_with('#') {
                    manifest
                        .imports
                        .push((key.clone(), relative_targets(target)));
                }
            }
        }
        manifest
    }
}

/// `./`-relative leaves of an export/import target in condition preference
/// order (bare package targets are not files of this package).
fn relative_targets(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    condition_targets(value, &mut out, 0);
    out.retain(|target| target.starts_with("./"));
    let mut seen = BTreeSet::new();
    out.retain(|target| seen.insert(target.clone()));
    out
}

fn condition_targets(value: &Value, out: &mut Vec<String>, depth: usize) {
    if depth > MAX_CONDITION_DEPTH {
        return;
    }
    match value {
        Value::String(target) => out.push(target.clone()),
        Value::Array(items) => {
            for item in items {
                condition_targets(item, out, depth + 1);
            }
        }
        Value::Object(map) => {
            for condition in CONDITIONS {
                if let Some(nested) = map.get(*condition) {
                    condition_targets(nested, out, depth + 1);
                }
            }
            for (condition, nested) in map {
                if !CONDITIONS.contains(&condition.as_str()) {
                    condition_targets(nested, out, depth + 1);
                }
            }
        }
        _ => {}
    }
}

/// Node subpath-pattern matching: exact key, else the longest `*` pattern
/// prefix, else a legacy `./dir/` folder mapping. Returns substituted targets.
fn match_subpath(patterns: &[(String, Vec<String>)], key: &str) -> Option<Vec<String>> {
    if let Some((_, targets)) = patterns.iter().find(|(pattern, _)| pattern == key) {
        return Some(targets.clone());
    }
    let wildcard = patterns
        .iter()
        .filter_map(|(pattern, targets)| {
            let (prefix, suffix) = pattern.split_once('*')?;
            (key.len() >= prefix.len() + suffix.len()
                && key.starts_with(prefix)
                && key.ends_with(suffix))
            .then_some((prefix, suffix, targets))
        })
        .max_by_key(|(prefix, _, _)| prefix.len());
    if let Some((prefix, suffix, targets)) = wildcard {
        let matched = &key[prefix.len()..key.len() - suffix.len()];
        return Some(
            targets
                .iter()
                .map(|target| target.replace('*', matched))
                .collect(),
        );
    }
    patterns
        .iter()
        .filter(|(pattern, _)| pattern.ends_with('/') && key.starts_with(pattern.as_str()))
        .max_by_key(|(pattern, _)| pattern.len())
        .map(|(pattern, targets)| {
            let rest = &key[pattern.len()..];
            targets
                .iter()
                .filter(|target| target.ends_with('/'))
                .map(|target| format!("{target}{rest}"))
                .collect()
        })
}

fn split_package(spec: &str) -> Option<(&str, &str)> {
    if spec.is_empty() {
        return None;
    }
    if spec.starts_with('@') {
        let mut slashes = spec.match_indices('/').map(|(index, _)| index);
        let _scope_end = slashes.next()?;
        return Some(match slashes.next() {
            Some(end) => (&spec[..end], &spec[end + 1..]),
            None => (spec, ""),
        });
    }
    Some(spec.split_once('/').unwrap_or((spec, "")))
}

fn resolve_package(manifest: &Manifest, subpath: &str, known: &BTreeSet<String>) -> Option<String> {
    let entry = || {
        resolve_targets(&manifest.dir, &manifest.entries, known)
            .or_else(|| probe_js(&join(&manifest.dir, "src/index"), known))
            .or_else(|| probe_js(&join(&manifest.dir, "index"), known))
    };
    if subpath.is_empty() {
        return entry();
    }
    if let Some(targets) = match_subpath(&manifest.exports, &format!("./{subpath}"))
        && let Some(found) = resolve_targets(&manifest.dir, &targets, known)
    {
        return Some(found);
    }
    // Deep imports without (or outside) an exports map: the package file,
    // its build-output source equivalent, `src/<subpath>`, then relative to
    // the resolved entry's directory.
    join_within_root(&manifest.dir, subpath)
        .and_then(|direct| probe_js(&direct, known))
        .or_else(|| source_equivalent(&manifest.dir, subpath, known))
        .or_else(|| {
            join_within_root(&join(&manifest.dir, "src"), subpath)
                .and_then(|candidate| probe_js(&candidate, known))
        })
        .or_else(|| {
            let entry = entry()?;
            probe_js(&join_within_root(dirname(&entry), subpath)?, known)
        })
}

fn is_declaration(path: &str) -> bool {
    DECLARATION_EXTENSIONS.iter().any(|ext| path.ends_with(ext))
}

/// Link the first target that names a parsed file: direct source hits
/// first, then build-output targets mapped back to sources, then
/// declaration files as a last resort.
fn resolve_targets(dir: &str, targets: &[String], known: &BTreeSet<String>) -> Option<String> {
    let joined = targets
        .iter()
        .filter_map(|target| Some((target, join_within_root(dir, target)?)))
        .collect::<Vec<_>>();
    joined
        .iter()
        .filter(|(_, path)| !is_declaration(path))
        .find_map(|(_, path)| probe_js(path, known))
        .or_else(|| {
            joined
                .iter()
                .find_map(|(target, _)| source_equivalent(dir, target, known))
        })
        .or_else(|| {
            joined
                .iter()
                .find(|(_, path)| is_declaration(path) && known.contains(path))
                .map(|(_, path)| path.clone())
        })
}

/// Strip a JavaScript/TypeScript or declaration extension.
fn strip_code_extension(path: &str) -> &str {
    DECLARATION_EXTENSIONS
        .iter()
        .chain(JS_EXTENSIONS)
        .find_map(|ext| path.strip_suffix(ext))
        .unwrap_or(path)
}

/// Map a package-relative build-output target (`dist/esm/x.js`,
/// `lib/x.d.ts`) back to its source (`src/x.ts`, `x.ts`).
fn source_equivalent(dir: &str, target: &str, known: &BTreeSet<String>) -> Option<String> {
    let target = normalize(target);
    let segments = target.split('/').collect::<Vec<_>>();
    if !segments
        .first()
        .is_some_and(|first| BUILD_DIRS.contains(first))
    {
        return None;
    }
    let mut rests = vec![segments[1..].join("/")];
    let mut index = 1;
    while index + 1 < segments.len() && FLAVOR_DIRS.contains(&segments[index]) {
        index += 1;
        rests.push(segments[index..].join("/"));
    }
    rests.iter().find_map(|rest| {
        let stem = strip_code_extension(rest);
        ["src", "."].iter().find_map(|base| {
            let candidate = join_within_root(&join(dir, base), stem)?;
            probe_js(&candidate, known)
        })
    })
}

/// Probe a module path the way relative imports are linked: exact file,
/// `.js`→`.ts` style swaps, extension probing, and `index` files.
pub(crate) fn probe_js(stem: &str, known: &BTreeSet<String>) -> Option<String> {
    if stem.is_empty() || stem == "." {
        return JS_EXTENSIONS
            .iter()
            .map(|ext| format!("index{ext}"))
            .find(|candidate| known.contains(candidate));
    }
    if JS_EXTENSIONS.iter().any(|ext| stem.ends_with(ext)) {
        if known.contains(stem) {
            return Some(stem.to_owned());
        }
        for (from, to) in [
            (".js", ".ts"),
            (".jsx", ".tsx"),
            (".mjs", ".mts"),
            (".cjs", ".cts"),
        ] {
            if let Some(base) = stem.strip_suffix(from) {
                let candidate = format!("{base}{to}");
                return known.contains(&candidate).then_some(candidate);
            }
        }
        return None;
    }
    std::iter::once(stem.to_owned())
        .chain(JS_EXTENSIONS.iter().map(|ext| format!("{stem}{ext}")))
        .chain(
            JS_EXTENSIONS
                .iter()
                .map(|ext| join(stem, &format!("index{ext}"))),
        )
        .find(|candidate| known.contains(candidate))
}

fn probe_js_or_source(candidate: &str, known: &BTreeSet<String>) -> Option<String> {
    probe_js(candidate, known).or_else(|| {
        // An alias into a package's build output (`packages/x/dist/*`).
        let segments = candidate.split('/').collect::<Vec<_>>();
        let build = segments
            .iter()
            .position(|segment| BUILD_DIRS.contains(segment))?;
        let dir = if build == 0 {
            ".".to_owned()
        } else {
            segments[..build].join("/")
        };
        source_equivalent(&dir, &segments[build..].join("/"), known)
    })
}

fn config_file_in(directory: &Path) -> Option<PathBuf> {
    ["tsconfig.json", "jsconfig.json"]
        .iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
}

/// A tsconfig's own options merged over its `extends` chain.
#[derive(Clone, Debug, Default)]
struct RawTsConfig {
    base_url: Option<PathBuf>,
    /// `paths` entries plus the directory of the config that declared them
    /// (the substitution base when no `baseUrl` is in effect).
    paths: Option<(PatternTargets, PathBuf)>,
    references: Vec<PathBuf>,
}

/// A project manifest or config the graph reads beside its sources: inside
/// the path policy, a regular file of at most [`MAX_CONFIG_BYTES`], decoded
/// and scanned by the content policy.
pub(super) fn read_config_text(
    paths: &PathPolicy,
    security: &ContentSecurity,
    path: &Path,
) -> Option<String> {
    let validated = paths.validate_read(path).ok()?;
    let bytes = crate::tools::source::read_bounded(&validated.canonical, MAX_CONFIG_BYTES).ok()?;
    security
        .validate_text_bytes(&bytes, Some(&validated.canonical), MAX_CONFIG_BYTES)
        .ok()
        .map(|safe| safe.content)
}

struct ConfigReader<'a> {
    root: &'a Path,
    paths: &'a PathPolicy,
    security: &'a ContentSecurity,
    cache: HashMap<PathBuf, Option<Arc<RawTsConfig>>>,
}

impl ConfigReader<'_> {
    fn read_json(&self, path: &Path) -> Option<Value> {
        parse_jsonc(&read_config_text(self.paths, self.security, path)?)
    }

    fn raw(&mut self, path: &Path, depth: usize) -> Option<Arc<RawTsConfig>> {
        if depth > MAX_EXTENDS_DEPTH {
            return None;
        }
        if let Some(cached) = self.cache.get(path) {
            return cached.clone();
        }
        // Mark in-progress so an `extends` cycle terminates.
        self.cache.insert(path.to_path_buf(), None);
        let parsed = self.read_json(path).map(|value| {
            let directory = path.parent().unwrap_or(self.root).to_path_buf();
            let mut merged = RawTsConfig::default();
            let extends = match value.get("extends") {
                Some(Value::String(one)) => vec![one.clone()],
                Some(Value::Array(many)) => many
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                _ => Vec::new(),
            };
            for base in extends {
                let Some(base_path) = resolve_extends(&directory, &base) else {
                    continue;
                };
                if let Some(parent) = self.raw(&base_path, depth + 1) {
                    if parent.base_url.is_some() {
                        merged.base_url.clone_from(&parent.base_url);
                    }
                    if parent.paths.is_some() {
                        merged.paths.clone_from(&parent.paths);
                    }
                }
            }
            let options = value.get("compilerOptions");
            if let Some(base_url) = options
                .and_then(|options| options.get("baseUrl"))
                .and_then(Value::as_str)
            {
                merged.base_url = Some(lexical(&directory.join(base_url)));
            }
            if let Some(Value::Object(map)) = options.and_then(|options| options.get("paths")) {
                let entries = map
                    .iter()
                    .map(|(pattern, targets)| {
                        let targets = match targets {
                            Value::Array(items) => items
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect(),
                            Value::String(one) => vec![one.clone()],
                            _ => Vec::new(),
                        };
                        (pattern.clone(), targets)
                    })
                    .collect();
                merged.paths = Some((entries, directory.clone()));
            }
            if let Some(Value::Array(references)) = value.get("references") {
                for reference in references {
                    let Some(target) = reference.get("path").and_then(Value::as_str) else {
                        continue;
                    };
                    let target = lexical(&directory.join(target));
                    merged.references.push(if target.is_dir() {
                        target.join("tsconfig.json")
                    } else {
                        target
                    });
                }
            }
            Arc::new(merged)
        });
        self.cache.insert(path.to_path_buf(), parsed.clone());
        parsed
    }

    fn compile(&self, raw: &RawTsConfig) -> TsPaths {
        let mut compiled = TsPaths {
            base_url: raw
                .base_url
                .as_deref()
                .and_then(|base| relative_to_root(self.root, base)),
            patterns: Vec::new(),
        };
        if let Some((entries, declared_in)) = &raw.paths {
            let base = raw.base_url.as_deref().unwrap_or(declared_in);
            for (pattern, targets) in entries {
                if pattern.matches('*').count() > 1 {
                    continue;
                }
                let (prefix, suffix, wildcard) = match pattern.split_once('*') {
                    Some((prefix, suffix)) => (prefix.to_owned(), suffix.to_owned(), true),
                    None => (pattern.clone(), String::new(), false),
                };
                let targets = targets
                    .iter()
                    .filter(|target| target.matches('*').count() <= 1)
                    .filter_map(|target| relative_to_root(self.root, &lexical(&base.join(target))))
                    .collect::<Vec<_>>();
                if !targets.is_empty() {
                    compiled.patterns.push(PathPattern {
                        prefix,
                        suffix,
                        wildcard,
                        targets,
                    });
                }
            }
        }
        compiled
    }

    fn effective(&mut self, config: &Path) -> TsPaths {
        let Some(raw) = self.raw(config, 0) else {
            return TsPaths::default();
        };
        let mut compiled = self.compile(&raw);
        if compiled.patterns.is_empty() && compiled.base_url.is_none() {
            // Solution-style config (`files: []` + references, e.g. Vite's
            // tsconfig.json → tsconfig.app.json): adopt the referenced
            // projects' aliases.
            for reference in &raw.references {
                if let Some(referenced) = self.raw(reference, 1) {
                    let referenced = self.compile(&referenced);
                    compiled.patterns.extend(referenced.patterns);
                    if compiled.base_url.is_none() {
                        compiled.base_url = referenced.base_url;
                    }
                }
            }
        }
        compiled
    }
}

/// Resolve an `extends` value: relative/absolute files (with implied
/// `.json`), then `node_modules` packages up the tree. Unfound → `None`.
fn resolve_extends(directory: &Path, value: &str) -> Option<PathBuf> {
    let probe = |base: PathBuf| {
        let with_json = PathBuf::from(format!("{}.json", base.to_string_lossy()));
        [base.clone(), with_json, base.join("tsconfig.json")]
            .into_iter()
            .find(|candidate| candidate.is_file())
    };
    if value.starts_with("./") || value.starts_with("../") || Path::new(value).is_absolute() {
        return probe(lexical(&directory.join(value)));
    }
    directory
        .ancestors()
        .take(12)
        .find_map(|ancestor| probe(ancestor.join("node_modules").join(value)))
}

/// Lexically normalize `..`/`.` without touching the file system.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Scan-root-relative slash path (`.` for the root); `None` outside it.
fn relative_to_root(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let normalized = normalize(&relative.to_string_lossy());
    Some(if normalized.is_empty() {
        ".".to_owned()
    } else {
        normalized
    })
}

/// Parse JSON with comments and trailing commas (tsconfig's JSONC dialect).
fn parse_jsonc(text: &str) -> Option<Value> {
    serde_json::from_str(&strip_jsonc(text)).ok()
}

fn strip_jsonc(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    let mut chunk_start = 0;
    let mut in_string = false;
    // Index in `out` of a pending comma that may turn out to be trailing.
    let mut pending_comma: Option<usize> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_string {
            if byte == b'\\' {
                index += 2;
                continue;
            }
            if byte == b'"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' => {
                pending_comma = None;
                in_string = true;
                index += 1;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                out.push_str(&text[chunk_start..index]);
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                chunk_start = index;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                out.push_str(&text[chunk_start..index]);
                index += 2;
                while index < bytes.len()
                    && !(bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/'))
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
                out.push(' ');
                chunk_start = index;
            }
            b',' => {
                out.push_str(&text[chunk_start..index]);
                pending_comma = Some(out.len());
                out.push(',');
                index += 1;
                chunk_start = index;
            }
            b'}' | b']' => {
                out.push_str(&text[chunk_start..index]);
                if let Some(comma) = pending_comma.take() {
                    out.replace_range(comma..comma + 1, " ");
                }
                chunk_start = index;
                index += 1;
            }
            b' ' | b'\t' | b'\r' | b'\n' => index += 1,
            _ => {
                pending_comma = None;
                index += 1;
            }
        }
    }
    out.push_str(&text[chunk_start.min(text.len())..]);
    out
}

/// `-I` / `-iquote` directories named by compile_commands.json, rebased to
/// the scan root; directories outside the root are dropped.
fn compile_command_include_dirs(root: &Path, file: &Path, paths: &PathPolicy) -> Vec<String> {
    let Ok(validated) = paths.validate_read(file) else {
        return Vec::new();
    };
    let Ok(bytes) =
        crate::tools::source::read_bounded(&validated.canonical, MAX_COMPILE_COMMANDS_BYTES)
    else {
        return Vec::new();
    };
    let Ok(Value::Array(commands)) = serde_json::from_slice::<Value>(&bytes) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    for command in commands {
        let directory = command
            .get("directory")
            .and_then(Value::as_str)
            .map_or_else(|| root.to_path_buf(), PathBuf::from);
        let arguments: Vec<String> = match command.get("arguments") {
            Some(Value::Array(items)) => items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            _ => command
                .get("command")
                .and_then(Value::as_str)
                .map(|line| line.split_whitespace().map(str::to_owned).collect())
                .unwrap_or_default(),
        };
        let mut iter = arguments.iter();
        while let Some(argument) = iter.next() {
            let value = if argument == "-I" || argument == "-iquote" {
                iter.next().cloned()
            } else if let Some(rest) = argument.strip_prefix("-I") {
                Some(rest.to_owned())
            } else {
                argument.strip_prefix("-iquote").map(str::to_owned)
            };
            let Some(value) = value.filter(|value| !value.is_empty()) else {
                continue;
            };
            let value = value.trim_matches('"');
            let Some(relative) = relative_to_root(root, &lexical(&directory.join(value))) else {
                continue;
            };
            if !dirs.contains(&relative) {
                dirs.push(relative);
                if dirs.len() >= MAX_INCLUDE_DIRS {
                    return dirs;
                }
            }
        }
    }
    dirs
}

/// Package directories above a scan root searched for the root's name.
const MAX_PYTHON_PACKAGE_DEPTH: usize = 16;

/// The dotted package name of `root` when it is a Python package (holds
/// `__init__.py`): its directory name, prefixed by every enclosing package.
fn enclosing_python_package(root: &Path) -> Option<String> {
    let is_package =
        |dir: &Path| dir.join("__init__.py").is_file() || dir.join("__init__.pyi").is_file();
    let mut names = Vec::new();
    let mut dir = root;
    while is_package(dir) && names.len() < MAX_PYTHON_PACKAGE_DEPTH {
        names.push(dir.file_name()?.to_string_lossy().into_owned());
        dir = dir.parent()?;
    }
    names.reverse();
    (!names.is_empty()).then(|| names.join("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ast_graph::test_support::{known, write_file};

    fn load(root: &Path, files: &BTreeSet<String>) -> ResolveContext {
        let policy = crate::tools::test_support::workspace_policy(root);
        ResolveContext::load(root, files, &policy, &ContentSecurity::new())
    }

    #[test]
    fn config_reads_stay_inside_the_policy_and_the_size_bound() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let policy = crate::tools::test_support::workspace_policy(root.path());
        let security = ContentSecurity::new();
        write_file(root.path(), "package.json", r#"{"name":"inside"}"#);
        write_file(outside.path(), "package.json", r#"{"name":"outside"}"#);
        assert_eq!(
            read_config_text(&policy, &security, &root.path().join("package.json")).as_deref(),
            Some(r#"{"name":"inside"}"#)
        );
        assert_eq!(
            read_config_text(&policy, &security, &outside.path().join("package.json")),
            None
        );
        write_file(root.path(), "big.json", &" ".repeat(MAX_CONFIG_BYTES + 1));
        assert_eq!(
            read_config_text(&policy, &security, &root.path().join("big.json")),
            None
        );
    }

    #[test]
    fn c_includes_fall_back_to_a_unique_path_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let files = [
            "deps/jemalloc/include/jemalloc/internal/stats.h",
            "deps/jemalloc/src/stats.c",
            "src/server.c",
            "a/util.h",
            "b/util.h",
            "b/main.c",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
        let context = load(dir.path(), &files);
        assert_eq!(
            context
                .header_by_suffix("jemalloc/internal/stats.h", "deps/jemalloc/src/stats.c")
                .as_deref(),
            Some("deps/jemalloc/include/jemalloc/internal/stats.h")
        );
        // Ambiguous basename: the includer's own subtree wins; otherwise none.
        assert_eq!(
            context.header_by_suffix("util.h", "b/main.c").as_deref(),
            Some("b/util.h")
        );
        assert_eq!(context.header_by_suffix("util.h", "src/server.c"), None);
        assert_eq!(context.header_by_suffix("missing.h", "src/server.c"), None);
    }

    #[test]
    fn jsonc_comments_and_trailing_commas_are_tolerated() {
        let value = parse_jsonc(
            "{\n // c\n \"a\": \"x//y/*z*/\", /* b */\n \"b\": [1, 2,],\n \"c\": {\"d\": 1,},\n}",
        )
        .unwrap();
        assert_eq!(value["a"], "x//y/*z*/");
        assert_eq!(value["b"], serde_json::json!([1, 2]));
        assert_eq!(value["c"]["d"], 1);
    }

    #[test]
    fn tsconfig_paths_resolve_through_relative_extends_chains() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        write_file(
            &root,
            "tsconfig.base.json",
            "{\n  // shared\n  \"compilerOptions\": {\n    \"baseUrl\": \".\",\n    \"paths\": {\n      \"@lib/*\": [\"missing/*\", \"packages/lib/src/*\"],\n      \"@lib\": [\"packages/lib/src/index.ts\"],\n      \"@/*\": [\"packages/app/src/*\"],\n    },\n  },\n}\n",
        );
        write_file(
            &root,
            "packages/app/tsconfig.json",
            "{ \"extends\": \"../../tsconfig.base\", \"compilerOptions\": {} }",
        );
        write_file(
            &root,
            "packages/other/tsconfig.json",
            "{ \"compilerOptions\": { \"paths\": { \"~x/*\": [\"./src/x/*\"] } } }",
        );
        let files = known(&[
            "packages/app/src/main.ts",
            "packages/app/src/util/helpers.ts",
            "packages/lib/src/index.ts",
            "packages/lib/src/deep/thing.tsx",
            "packages/other/src/main.ts",
            "packages/other/src/x/y.ts",
        ]);
        let context = load(&root, &files);
        let importer = "packages/app/src/main.ts";
        for (spec, expected) in [
            ("@lib/deep/thing", Some("packages/lib/src/deep/thing.tsx")),
            ("@lib", Some("packages/lib/src/index.ts")),
            (
                "@/util/helpers.js",
                Some("packages/app/src/util/helpers.ts"),
            ),
            ("@lib/none", None),
            ("react", None),
        ] {
            assert_eq!(
                context.resolve_bare_js(spec, importer, &files).as_deref(),
                expected,
                "{spec}"
            );
        }
        // Nearest config governs: `other` does not see the base aliases, and
        // its own `paths` are relative to its config without a baseUrl.
        assert_eq!(
            context.resolve_bare_js("@lib", "packages/other/src/main.ts", &files),
            None
        );
        assert_eq!(
            context
                .resolve_bare_js("~x/y", "packages/other/src/main.ts", &files)
                .as_deref(),
            Some("packages/other/src/x/y.ts")
        );
        assert!(context.is_internal_bare_js("@lib/none", importer));
        assert!(!context.is_internal_bare_js("react", importer));
        assert!(context.is_internal_bare_js("~/anything", importer));
    }

    #[test]
    fn base_url_resolves_non_relative_modules_and_references_are_followed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        write_file(
            &root,
            "tsconfig.json",
            "{ \"files\": [], \"references\": [{ \"path\": \"./tsconfig.app.json\" }] }",
        );
        write_file(
            &root,
            "tsconfig.app.json",
            "{ \"compilerOptions\": { \"baseUrl\": \"src\", \"paths\": { \"@/*\": [\"./*\"] } } }",
        );
        let files = known(&["src/main.ts", "src/components/Button.tsx"]);
        let context = load(&root, &files);
        assert_eq!(
            context
                .resolve_bare_js("@/components/Button", "src/main.ts", &files)
                .as_deref(),
            Some("src/components/Button.tsx")
        );
        assert_eq!(
            context
                .resolve_bare_js("components/Button", "src/main.ts", &files)
                .as_deref(),
            Some("src/components/Button.tsx")
        );
    }

    #[test]
    fn workspace_packages_prefer_sources_over_build_output() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        write_file(
            &root,
            "packages/core/package.json",
            r##"{
              "name": "@acme/core",
              "main": "./dist/cjs/index.js",
              "exports": {
                ".": { "types": "./dist/index.d.ts", "import": { "default": "./dist/esm/index.mjs" }, "require": "./dist/cjs/index.js" },
                "./feature": { "import": "./dist/esm/feature/index.js" },
                "./utils/*": "./dist/utils/*.js",
                "./package.json": "./package.json"
              },
              "imports": { "#internal/*": { "source": "./src/internal/*.ts", "default": "./dist/internal/*.js" } }
            }"##,
        );
        write_file(
            &root,
            "packages/legacy/package.json",
            r#"{ "name": "legacy", "module": "lib/index.js", "types": "lib/index.d.ts" }"#,
        );
        let files = known(&[
            "packages/core/src/index.mts",
            "packages/core/src/feature/index.ts",
            "packages/core/src/utils/strings.ts",
            "packages/core/src/internal/secret.ts",
            "packages/core/src/deep/file.ts",
            "packages/legacy/src/index.ts",
            "packages/app/main.ts",
        ]);
        let context = load(&root, &files);
        let importer = "packages/app/main.ts";
        for (spec, expected) in [
            ("@acme/core", Some("packages/core/src/index.mts")),
            (
                "@acme/core/feature",
                Some("packages/core/src/feature/index.ts"),
            ),
            (
                "@acme/core/utils/strings",
                Some("packages/core/src/utils/strings.ts"),
            ),
            (
                "@acme/core/deep/file",
                Some("packages/core/src/deep/file.ts"),
            ),
            ("legacy", Some("packages/legacy/src/index.ts")),
            ("@acme/core/nope", None),
        ] {
            assert_eq!(
                context.resolve_bare_js(spec, importer, &files).as_deref(),
                expected,
                "{spec}"
            );
        }
        assert_eq!(
            context
                .resolve_bare_js("#internal/secret", "packages/core/src/index.mts", &files)
                .as_deref(),
            Some("packages/core/src/internal/secret.ts")
        );
        assert!(context.is_internal_bare_js("@acme/core/nope", importer));
        assert!(context.is_internal_bare_js("#internal/x", importer));
    }

    #[test]
    fn python_roots_include_src_layout_project_roots() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        write_file(
            &root,
            "libs/tool/pyproject.toml",
            "[project]\nname='tool'\n",
        );
        write_file(&root, "libs/tool/src/tool/__init__.py", "");
        let files = known(&["libs/tool/src/tool/__init__.py", "app/main.py"]);
        let context = load(&root, &files);
        assert_eq!(
            context.python_roots_for("libs/tool/src/tool/__init__.py"),
            vec!["libs/tool/src", "libs/tool", "."]
        );
        assert_eq!(
            context.python_roots_for("app/main.py"),
            vec![".", "libs/tool", "libs/tool/src"]
        );
    }

    #[test]
    fn subtree_scans_use_the_package_tsconfig_above_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        write_file(
            &workspace,
            "app/tsconfig.json",
            "{ \"compilerOptions\": { \"paths\": { \"@ui/*\": [\"./src/ui/*\"] } } }",
        );
        write_file(&workspace, "app/package.json", "{}");
        let root = workspace.join("app/src");
        let files = known(&["main.ts", "ui/button.ts"]);
        let policy = crate::tools::test_support::workspace_policy(&workspace);
        let context = ResolveContext::load(&root, &files, &policy, &ContentSecurity::new());
        assert_eq!(
            context
                .resolve_bare_js("@ui/button", "main.ts", &files)
                .as_deref(),
            Some("ui/button.ts")
        );
    }

    #[test]
    fn compile_commands_include_dirs_are_rebased_to_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let commands = serde_json::json!([
            {"directory": root.join("build"), "command": "c++ -I../third/include -I /outside -iquote ../quoted -c ../src/a.cpp", "file": "../src/a.cpp"},
            {"directory": root, "arguments": ["cc", "-I", "gen", "-c", "b.c"], "file": "b.c"}
        ]);
        write_file(&root, "compile_commands.json", &commands.to_string());
        write_file(&root, "src/a.cpp", "");
        let files = known(&["src/a.cpp"]);
        let context = load(&root, &files);
        assert_eq!(
            context.include_dirs(false),
            ["third/include", "quoted", "gen"].map(str::to_owned)
        );
        assert_eq!(
            context.include_dirs(true),
            [".", "src", "third/include", "quoted", "gen"].map(str::to_owned)
        );
    }
}
