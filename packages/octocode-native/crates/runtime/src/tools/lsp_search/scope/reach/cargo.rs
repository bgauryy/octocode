//! Cargo facts reach reads: a file's package, its target roots, which
//! packages depend on another, and the modules a package places by
//! `#[path]`.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const MANIFEST: &str = "Cargo.toml";
const DEPENDENCY_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

fn manifest(package: &Path) -> Option<toml::Table> {
    std::fs::read_to_string(package.join(MANIFEST))
        .ok()?
        .parse()
        .ok()
}

/// The nearest directory above `file` holding a `Cargo.toml` that names a
/// package (canonical): the package that compiles it.
pub(super) fn package_of(file: &Path) -> Option<PathBuf> {
    file.ancestors()
        .skip(1)
        .find(|dir| dir.join(MANIFEST).is_file())
        .filter(|dir| package_name(dir).is_some())
        .map(Path::to_path_buf)
}

pub(super) fn package_name(package: &Path) -> Option<String> {
    Some(
        manifest(package)?
            .get("package")?
            .get("name")?
            .as_str()?
            .to_owned(),
    )
}

/// The root files the manifest names for its targets (`[lib] path`,
/// `[[bin]] path`, …): crate roots whatever their name.
pub(super) fn target_roots(package: &Path) -> Vec<PathBuf> {
    let Some(manifest) = manifest(package) else {
        return Vec::new();
    };
    let path = |target: &toml::Value| {
        target
            .get("path")
            .and_then(toml::Value::as_str)
            .map(|path| octocode_engine::text::lexical(&package.join(path)))
    };
    let mut roots = manifest
        .get("lib")
        .and_then(path)
        .into_iter()
        .collect::<Vec<_>>();
    for kind in ["bin", "test", "bench", "example"] {
        roots.extend(
            manifest
                .get(kind)
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(path),
        );
    }
    roots
}

/// The dependency entries of `manifest`: every dependency table, target
/// specific ones included.
fn dependencies(manifest: &toml::Table) -> Vec<(&str, &toml::Value)> {
    fn tables(owner: &toml::Table) -> Vec<(&str, &toml::Value)> {
        DEPENDENCY_TABLES
            .iter()
            .filter_map(|table| owner.get(*table).and_then(toml::Value::as_table))
            .flat_map(|table| table.iter().map(|(key, value)| (key.as_str(), value)))
            .collect()
    }
    let mut found = tables(manifest);
    for target in manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(toml::Table::values)
        .filter_map(toml::Value::as_table)
    {
        found.extend(tables(target));
    }
    found
}

/// The local directory a dependency entry of the package at `package`
/// resolves to: its `path`, or its workspace entry's; `None` for a
/// registry or git dependency.
fn dependency_dir(package: &Path, key: &str, entry: &toml::Value) -> Option<PathBuf> {
    if let Some(path) = entry.get("path").and_then(toml::Value::as_str) {
        return std::fs::canonicalize(package.join(path)).ok();
    }
    if entry.get("workspace").and_then(toml::Value::as_bool) != Some(true) {
        return None;
    }
    let root = octocode_engine::lsp::workspace::cargo_workspace_root(package)?;
    let workspace = manifest(&root)?;
    let path = workspace
        .get("workspace")?
        .get("dependencies")?
        .get(key)?
        .get("path")?
        .as_str()?
        .to_owned();
    std::fs::canonicalize(root.join(path)).ok()
}

/// Names compare as Cargo normalizes them for code: `-` and `_` alike.
fn same_crate(left: &str, right: &str) -> bool {
    left.replace('-', "_") == right.replace('-', "_")
}

/// Whether the package at `start` depends on crate `name` (at `defining`),
/// directly or through local path dependencies: a re-export can relay an
/// item down a chain. A `[patch]` of the workspace that points at the
/// defining package can make any registry dependency it, so every package
/// of that workspace depends on it.
pub(super) fn depends_on(start: &Path, name: &str, defining: &Path) -> bool {
    if patched(start, defining) {
        return true;
    }
    let mut seen = HashSet::from([start.to_path_buf()]);
    let mut queue = vec![start.to_path_buf()];
    while let Some(package) = queue.pop() {
        let Some(manifest) = manifest(&package) else {
            continue;
        };
        for (key, entry) in dependencies(&manifest) {
            let crate_name = entry
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(key);
            if same_crate(crate_name, name) {
                return true;
            }
            if let Some(dir) = dependency_dir(&package, key, entry)
                && seen.insert(dir.clone())
            {
                queue.push(dir);
            }
        }
    }
    false
}

/// Whether the workspace of `package` patches a dependency to `defining`.
fn patched(package: &Path, defining: &Path) -> bool {
    let Some(root) = octocode_engine::lsp::workspace::cargo_workspace_root(package) else {
        return false;
    };
    manifest(&root)
        .and_then(|manifest| {
            manifest
                .get("patch")
                .and_then(toml::Value::as_table)
                .cloned()
        })
        .into_iter()
        .flat_map(toml::Table::into_iter)
        .filter_map(|(_, registry)| registry.as_table().cloned())
        .flat_map(toml::Table::into_iter)
        .filter_map(|(_, entry)| entry.get("path")?.as_str().map(str::to_owned))
        .any(|path| std::fs::canonicalize(root.join(path)).is_ok_and(|dir| dir == defining))
}

/// The modules the package at `package` places by `#[path = "…"]`, each
/// as (declaring file, module file), canonical; nested packages are their
/// own. Every `.rs` file is read; `None` when `stopped` ended the walk
/// first.
pub(super) fn relocated_modules(
    package: &Path,
    stopped: &dyn Fn() -> bool,
) -> Option<Vec<(PathBuf, PathBuf)>> {
    let root = package.to_path_buf();
    let walker = ignore::WalkBuilder::new(package)
        .filter_entry(move |entry| {
            entry.path() == root
                || !entry.file_type().is_some_and(|kind| kind.is_dir())
                || !entry.path().join(MANIFEST).is_file()
        })
        .build();
    let mut found = Vec::new();
    for entry in walker.flatten() {
        if stopped() {
            return None;
        }
        let path = entry.path();
        if !entry.file_type().is_some_and(|kind| kind.is_file())
            || path.extension().is_none_or(|ext| ext != "rs")
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let dir = path.parent().unwrap_or(path);
        for target in path_attributes(&text) {
            let target = octocode_engine::text::lexical(&dir.join(target));
            let target = std::fs::canonicalize(&target).unwrap_or(target);
            found.push((path.to_path_buf(), target));
        }
    }
    Some(found)
}

/// The targets of the `#[path = "…"]` attributes in `text`.
fn path_attributes(text: &str) -> Vec<&str> {
    text.match_indices("#[path")
        .filter_map(|(at, _)| {
            let value = text[at + "#[path".len()..].trim_start().strip_prefix('=')?;
            value.trim_start().strip_prefix('"')?.split('"').next()
        })
        .collect()
}
