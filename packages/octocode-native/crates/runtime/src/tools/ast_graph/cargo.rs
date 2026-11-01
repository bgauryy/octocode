use super::paths::normalize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

mod workspaces;
pub(super) use workspaces::{enclosing_manifests, load_cargo_crates};

/// Cargo names belong to the importing package, not the whole workspace.
#[derive(Default)]
pub(super) struct CargoCrates {
    packages: BTreeMap<String, BTreeMap<String, String>>,
    pub(super) roots: BTreeSet<String>,
    /// The scan root's module path when the root lies inside a crate whose
    /// root (`src/lib.rs`, `src/main.rs`) is above it: `crate::a::b::x`
    /// written under root `src/a/b` names `x` inside the scan.
    enclosing_module: Option<Vec<String>>,
}

impl CargoCrates {
    pub(super) fn scan_is_submodule(&self) -> bool {
        self.enclosing_module.is_some()
    }
    /// These crates, with the scan `root`'s module path inside the crate
    /// that encloses it ([`enclosing_module`]).
    pub(super) fn within(mut self, root: &Path) -> Self {
        self.enclosing_module = enclosing_module(root);
        self
    }

    /// `crate::` path `segments` (after `crate`) as a path from the scan
    /// root, when the scan root lies below its crate's root and the path
    /// names its module or one below it (empty: the root module itself);
    /// `None` otherwise.
    pub(super) fn inside_scan<'s>(&self, segments: &'s [&'s str]) -> Option<&'s [&'s str]> {
        let prefix = self.enclosing_module.as_ref()?;
        (segments.len() >= prefix.len()
            && prefix
                .iter()
                .zip(segments)
                .all(|(module, segment)| module == segment))
        .then(|| &segments[prefix.len()..])
    }

    pub(super) fn resolve(&self, importer: &str, name: &str) -> Option<&String> {
        self.packages
            .iter()
            .filter(|(directory, _)| {
                directory.is_empty()
                    || importer
                        .strip_prefix(directory.as_str())
                        .is_some_and(|tail| tail.starts_with('/'))
            })
            .max_by_key(|(directory, _)| directory.len())
            .and_then(|(_, dependencies)| dependencies.get(name))
    }
}

/// The module path of `root` inside the crate whose source root encloses
/// it: the nearest ancestor holding `lib.rs` or `main.rs` beside a
/// `Cargo.toml` one level up (the `src/` of a package), with `root`'s
/// directories below it as module names. `None` when `root` is itself a
/// crate root, or no crate encloses it.
fn enclosing_module(root: &Path) -> Option<Vec<String>> {
    let root = root.canonicalize().ok()?;
    let source = root.ancestors().skip(1).find(|dir| {
        (dir.join("lib.rs").is_file() || dir.join("main.rs").is_file())
            && dir
                .parent()
                .is_some_and(|package| package.join("Cargo.toml").is_file())
    })?;
    let modules = root
        .strip_prefix(source)
        .ok()?
        .components()
        .map(|part| part.as_os_str().to_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()?;
    (!modules.is_empty()).then_some(modules)
}

pub(super) fn parse_cargo_crates(root: &Path, value: &serde_json::Value) -> CargoCrates {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let packages = value
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    // Collect every library before resolving dependencies: Cargo package order
    // does not imply dependency order. Paths disambiguate versions/renames.
    let libraries = packages
        .iter()
        .filter_map(|package| package_library(package, &root))
        .collect::<Libraries>();
    let mut crates = CargoCrates::default();
    for package in packages {
        for target in package["targets"].as_array().into_iter().flatten() {
            if let Some(source) = target["src_path"].as_str()
                && let Ok(relative) = Path::new(source).strip_prefix(&root)
            {
                crates.roots.insert(normalize(&relative.to_string_lossy()));
            }
        }
        let Some(directory) = package_dir(package) else {
            continue;
        };
        let scope = if let Ok(relative) = directory.strip_prefix(&root) {
            normalize(&relative.to_string_lossy())
        } else if root.starts_with(directory) {
            String::new()
        } else {
            continue;
        };
        crates
            .packages
            .insert(scope, package_dependencies(package, directory, &libraries));
    }
    crates
}

/// Package directory → `(package name, crate import name, root-relative
/// library source)`.
type Libraries = BTreeMap<PathBuf, (String, String, String)>;

fn package_dir(package: &serde_json::Value) -> Option<&Path> {
    Path::new(package["manifest_path"].as_str()?).parent()
}

/// The package's library target, when it has one under `root`.
fn package_library(
    package: &serde_json::Value,
    root: &Path,
) -> Option<(PathBuf, (String, String, String))> {
    let directory = package_dir(package)?;
    let name = package["name"].as_str()?;
    let mut found = None;
    for target in package["targets"].as_array().into_iter().flatten() {
        let library = target["kind"].as_array().into_iter().flatten().any(|kind| {
            matches!(
                kind.as_str(),
                Some("lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro")
            )
        });
        if !library {
            continue;
        }
        let (Some(source), Some(crate_name)) =
            (target["src_path"].as_str(), target["name"].as_str())
        else {
            continue;
        };
        let Ok(relative) = Path::new(source).strip_prefix(root) else {
            continue;
        };
        found = Some((
            directory.to_path_buf(),
            (
                name.to_owned(),
                crate_name.replace('-', "_"),
                normalize(&relative.to_string_lossy()),
            ),
        ));
    }
    found
}

/// Import name → library source for the package itself and its path
/// dependencies on local libraries.
fn package_dependencies(
    package: &serde_json::Value,
    directory: &Path,
    libraries: &Libraries,
) -> BTreeMap<String, String> {
    let mut candidates: BTreeMap<String, BTreeSet<Option<String>>> = BTreeMap::new();
    if let Some((_, crate_name, source)) = libraries.get(directory) {
        candidates
            .entry(crate_name.clone())
            .or_default()
            .insert(Some(source.clone()));
    }
    for dependency in package["dependencies"].as_array().into_iter().flatten() {
        let Some(name) = dependency["name"].as_str() else {
            continue;
        };
        let alias = dependency["rename"].as_str();
        // Only a path dependency identifies one of these local packages.
        // A same-named registry dependency must not link to a workspace file.
        let library = dependency["path"].as_str().and_then(|path| {
            let dependency_path = Path::new(path)
                .canonicalize()
                .unwrap_or_else(|_| PathBuf::from(path));
            libraries
                .get(&dependency_path)
                .filter(|(package_name, _, _)| package_name == name)
        });
        if let Some((_, crate_name, source)) = library {
            let import_name = alias
                .map(|alias| alias.replace('-', "_"))
                .unwrap_or_else(|| crate_name.clone());
            candidates
                .entry(import_name)
                .or_default()
                .insert(Some(source.clone()));
        } else {
            // An external/unindexed target using this same alias is also a
            // competing configuration; do not silently keep the local one.
            candidates
                .entry(alias.unwrap_or(name).replace('-', "_"))
                .or_default()
                .insert(None);
        }
    }
    // Conditional dependencies can reuse an alias for different targets.
    // Without a configured Cargo resolve graph, leave such names unresolved.
    candidates
        .into_iter()
        .filter_map(|(name, sources)| {
            if sources.len() == 1 {
                sources
                    .into_iter()
                    .next()
                    .flatten()
                    .map(|source| (name, source))
            } else {
                None
            }
        })
        .collect()
}
#[cfg(test)]
mod tests {
    #[test]
    fn conflicting_conditional_aliases_and_registry_names_do_not_fabricate_edges() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let package = |name: &str, dependencies: serde_json::Value| {
            serde_json::json!({
                "name":name, "manifest_path":root.join(name).join("Cargo.toml"),
                "targets":[{"name":name, "kind":["lib"], "src_path":root.join(name).join("src/lib.rs")}],
                "dependencies":dependencies,
            })
        };
        let value = serde_json::json!({"packages":[
            package("app", serde_json::json!([
                {"name":"one", "rename":"shared", "path":root.join("one"), "target":"cfg(unix)"},
                {"name":"two", "rename":"shared", "path":root.join("two"), "target":"cfg(windows)"},
                {"name":"one", "rename":"mixed", "path":root.join("one"), "target":"cfg(unix)"},
                {"name":"two", "rename":"mixed", "source":"registry+https://example.invalid/index", "target":"cfg(windows)"},
                {"name":"one", "source":"registry+https://example.invalid/index"},
            ])),
            package("one", serde_json::json!([])),
            package("two", serde_json::json!([])),
        ]});
        let crates = super::parse_cargo_crates(&root, &value);
        assert_eq!(crates.resolve("app/src/lib.rs", "shared"), None);
        assert_eq!(crates.resolve("app/src/lib.rs", "mixed"), None);
        assert_eq!(crates.resolve("app/src/lib.rs", "one"), None);
        assert_eq!(
            crates.resolve("app/src/lib.rs", "app").map(String::as_str),
            Some("app/src/lib.rs")
        );
    }
}
