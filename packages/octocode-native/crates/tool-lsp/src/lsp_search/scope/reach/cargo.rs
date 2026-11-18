//! Cargo facts reach reads: a file's package, its target roots, which
//! packages depend on another, and the modules a package places by
//! `#[path]`.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const MANIFEST: &str = "Cargo.toml";
const DEPENDENCY_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

fn manifest(package: &Path) -> Option<std::sync::Arc<toml::Table>> {
    octocode_engine::lsp::workspace::cargo_manifest(package)
}

/// The nearest directory above `file` holding a `Cargo.toml` that names a
/// package (canonical): the package that compiles it.
pub(in crate::lsp_search) fn package_of(file: &Path) -> Option<PathBuf> {
    file.ancestors()
        .skip(1)
        .find(|dir| dir.join(MANIFEST).is_file())
        .filter(|dir| package_name(dir).is_some())
        .map(Path::to_path_buf)
}

pub(in crate::lsp_search) fn package_name(package: &Path) -> Option<String> {
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

/// The local path a dependency entry of the package at `package` names:
/// its `path`, or its workspace entry's; `None` for a registry or git
/// dependency, or a workspace entry this cannot read.
fn dependency_path(package: &Path, key: &str, entry: &toml::Value) -> Option<PathBuf> {
    if let Some(path) = entry.get("path").and_then(toml::Value::as_str) {
        return Some(package.join(path));
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
    Some(root.join(path))
}

/// The local directory a dependency entry resolves to ([`dependency_path`],
/// canonical); `None` for a registry or git dependency, or a path that
/// names no directory.
fn dependency_dir(package: &Path, key: &str, entry: &toml::Value) -> Option<PathBuf> {
    dependency_path(package, key, entry).and_then(|path| std::fs::canonicalize(path).ok())
}

/// Names compare as Cargo normalizes them for code: `-` and `_` alike.
fn same_crate(left: &str, right: &str) -> bool {
    left.replace('-', "_") == right.replace('-', "_")
}

/// Whether the package at `start` depends on crate `name` (at `defining`),
/// directly or through local path dependencies: a re-export can relay an
/// item down a chain. A dependency names the crate its source holds, not any
/// crate of that name: one by local path is the defining crate only when the
/// path is its directory (a snapshot of the workspace depends on its own
/// copy; a path that names no directory compiles nothing). A registry or git
/// dependency of that name may be it: a `[patch]` of the workspace that
/// points at the defining package makes every package of that workspace
/// depend on it, and a patch this does not read may too.
pub(in crate::lsp_search) fn depends_on(start: &Path, name: &str, defining: &Path) -> bool {
    patched(start, defining)
        || local_packages(start).iter().any(|package| {
            manifest(package).is_some_and(|manifest| {
                dependencies(&manifest).into_iter().any(|(key, entry)| {
                    let crate_name = entry
                        .get("package")
                        .and_then(toml::Value::as_str)
                        .unwrap_or(key);
                    same_crate(crate_name, name)
                        && dependency_path(package, key, entry).is_none_or(|path| {
                            std::fs::canonicalize(path).is_ok_and(|dir| dir == defining)
                        })
                })
            })
        })
}

/// The package at `start` first, then every package it reaches through
/// local path dependencies, directly or not (canonical manifest
/// directories): the crates whose sources may define a name it uses.
pub(in crate::lsp_search) fn local_packages(start: &Path) -> Vec<PathBuf> {
    let mut seen = HashSet::from([start.to_path_buf()]);
    let mut found = vec![start.to_path_buf()];
    let mut next = 0;
    while let Some(package) = found.get(next).cloned() {
        next += 1;
        let Some(manifest) = manifest(&package) else {
            continue;
        };
        for (key, entry) in dependencies(&manifest) {
            if let Some(dir) = dependency_dir(&package, key, entry)
                && seen.insert(dir.clone())
            {
                found.push(dir);
            }
        }
    }
    found
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
/// own. Every `.rs` file is read (bounded, never a FIFO); `None` when `stopped` ended the walk
/// first.
pub(super) fn relocated_modules(
    package: &Path,
    stopped: &dyn Fn() -> bool,
) -> Option<Vec<(PathBuf, PathBuf)>> {
    let walker = octocode_tools_shared::package_walk(package).build();
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
        let Ok(bytes) = octocode_tools_shared::source::read_bounded(
            path,
            octocode_runtime_base::security::SourceLimits::REAL.whole_source,
        ) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
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

#[cfg(test)]
mod tests;
