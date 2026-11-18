//! Cargo workspace membership for language-server root discovery.

use super::{CARGO_MANIFEST, lexical};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

mod manifest;
mod targets;
mod unloaded;
pub use manifest::cargo_manifest;
pub(super) use targets::TargetRoots;
pub use unloaded::{CACHEDIR_TAG, UnloadedCargoCaches};

pub(super) fn manifest(directory: &Path) -> Option<Arc<toml::Table>> {
    cargo_manifest(directory)
}

/// Whether the parsed manifest declares a workspace.
pub fn declares_cargo_workspace(directory: &Path) -> bool {
    manifest(directory)
        .is_some_and(|table| table.get("workspace").is_some_and(toml::Value::is_table))
}

/// The workspace that includes this package, respecting exclusions,
/// explicit workspace paths, and implicit members reached by path dependencies.
pub fn cargo_workspace_root(package_dir: &Path) -> Option<PathBuf> {
    let package = lexical(package_dir);
    let package_manifest = manifest(&package)?;
    if package_manifest
        .get("workspace")
        .is_some_and(toml::Value::is_table)
    {
        return Some(package);
    }
    let root = match package_manifest
        .get("package")
        .and_then(|value| value.get("workspace"))
        .and_then(toml::Value::as_str)
    {
        Some(explicit) => lexical(&package.join(explicit)),
        None => package
            .parent()?
            .ancestors()
            .find(|dir| declares_cargo_workspace(dir))?
            .to_path_buf(),
    };
    let root_manifest = manifest(&root)?;
    let workspace = root_manifest.get("workspace")?.as_table()?;
    contains_package(&root, &root_manifest, workspace, &package).then_some(root)
}

fn patterns<'a>(workspace: &'a toml::Table, key: &str) -> impl Iterator<Item = &'a str> {
    workspace
        .get(key)
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
}

/// Whether `candidate` is a directory the members entry `pattern` names
/// below `root`. The pattern's leading glob-free parts are a directory, and
/// only the rest is a glob, matched against the candidate's path below that
/// directory: the root never enters the glob, so no spelling of it (a
/// Windows separator, bytes that are not UTF-8, glob characters) changes
/// what matches.
fn matches(root: &Path, pattern: &str, candidate: &Path) -> bool {
    let mut parts = pattern
        .split('/')
        .filter(|part| !part.is_empty())
        .peekable();
    let mut base = root.to_path_buf();
    while let Some(part) = parts.next_if(|part| !part.contains(crate::search::glob::GLOB_META)) {
        base.push(part);
    }
    let rest = parts.collect::<Vec<_>>().join("/");
    let Ok(below) = candidate.strip_prefix(lexical(&base)) else {
        return false;
    };
    // A literal pattern names its base; a glob part names what lies below it.
    if rest.is_empty() || below.as_os_str().is_empty() {
        return rest.is_empty() && below.as_os_str().is_empty();
    }
    globset::GlobBuilder::new(&rest)
        .literal_separator(true)
        .backslash_escape(false)
        .build()
        .is_ok_and(|glob| glob.compile_matcher().is_match(below))
}

fn contains_package(
    root: &Path,
    root_manifest: &toml::Table,
    workspace: &toml::Table,
    package: &Path,
) -> bool {
    // Cargo's excludes are literal directory prefixes. A literal members
    // entry takes precedence; an expanded glob or implicit dependency does not.
    let explicit = |path: &Path| {
        patterns(workspace, "members").any(|member| lexical(&root.join(member)) == path)
    };
    let excluded = |path: &Path| {
        !explicit(path)
            && patterns(workspace, "exclude")
                .any(|excluded| path.starts_with(lexical(&root.join(excluded))))
    };
    if excluded(package) {
        return false;
    }
    if patterns(workspace, "members").any(|pattern| matches(root, pattern, package)) {
        return true;
    }
    let mut pending = patterns(workspace, "members")
        .flat_map(|pattern| member_dirs(root, pattern))
        .collect::<Vec<_>>();
    if root_manifest.contains_key("package") {
        pending.push(root.to_path_buf());
    }
    let mut visited = HashSet::new();
    while let Some(directory) = pending.pop() {
        if excluded(&directory) || !visited.insert(directory.clone()) {
            continue;
        }
        if directory == package {
            return true;
        }
        let Some(member) = manifest(&directory) else {
            continue;
        };
        // A nested workspace owns its packages independently. An explicit
        // workspace association also takes precedence over ancestry.
        if directory != root && member.contains_key("workspace") {
            continue;
        }
        if let Some(explicit) = member
            .get("package")
            .and_then(|value| value.get("workspace"))
            .and_then(toml::Value::as_str)
            && lexical(&directory.join(explicit)) != root
        {
            continue;
        }
        pending.extend(
            path_dependencies(&member, &directory, root, workspace)
                .filter(|path| path.starts_with(root)),
        );
    }
    false
}

/// The directories of the path dependencies `member` (the manifest at
/// `directory`) declares, in every dependency table, its target-specific
/// ones included; a `workspace = true` dependency resolves through the
/// `[workspace.dependencies]` of the workspace at `root`.
fn path_dependencies<'a>(
    member: &'a toml::Table,
    directory: &'a Path,
    root: &'a Path,
    workspace: &'a toml::Table,
) -> impl Iterator<Item = PathBuf> + 'a {
    let tables = std::iter::once(member).chain(
        member
            .get("target")
            .and_then(toml::Value::as_table)
            .into_iter()
            .flat_map(|targets| targets.values())
            .filter_map(toml::Value::as_table),
    );
    tables
        .flat_map(|table| {
            ["dependencies", "dev-dependencies", "build-dependencies"]
                .into_iter()
                .filter_map(|key| table.get(key).and_then(toml::Value::as_table))
                .flatten()
        })
        .filter_map(move |(name, dependency)| {
            let inherited =
                dependency.get("workspace").and_then(toml::Value::as_bool) == Some(true);
            let dependency = if inherited {
                workspace
                    .get("dependencies")
                    .and_then(|value| value.get(name))?
            } else {
                dependency
            };
            let path = dependency.get("path").and_then(toml::Value::as_str)?;
            let base = if inherited { root } else { directory };
            Some(lexical(&base.join(path)))
        })
}

/// Expand only the directories named by a members pattern. Literal paths
/// need no tree walk; `*` stays within one component, while `**` descends.
fn member_dirs(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut directories = vec![root.to_path_buf()];
    for component in Path::new(pattern).components() {
        let Component::Normal(name) = component else {
            for directory in &mut directories {
                match component {
                    Component::CurDir => {}
                    Component::ParentDir => {
                        directory.pop();
                    }
                    other => directory.push(other),
                }
            }
            continue;
        };
        let name = name.to_string_lossy();
        if name == "**" {
            let mut seen = HashSet::new();
            let mut pending = std::mem::take(&mut directories);
            while let Some(directory) = pending.pop() {
                let identity =
                    std::fs::canonicalize(&directory).unwrap_or_else(|_| directory.clone());
                if !seen.insert(identity) {
                    continue;
                }
                pending.extend(child_dirs(&directory));
                directories.push(directory);
            }
        } else if name.contains(['*', '?', '[']) {
            let Ok(glob) = globset::Glob::new(&name) else {
                return Vec::new();
            };
            let matcher = glob.compile_matcher();
            directories = directories
                .iter()
                .flat_map(|directory| child_dirs(directory))
                .filter(|path| {
                    path.file_name()
                        .is_some_and(|name| matcher.is_match(Path::new(name)))
                })
                .collect();
        } else {
            for directory in &mut directories {
                directory.push(name.as_ref());
            }
        }
    }
    directories
        .into_iter()
        .filter(|directory| directory.join(CARGO_MANIFEST).is_file())
        .collect()
}

fn child_dirs(directory: &Path) -> impl Iterator<Item = PathBuf> {
    std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
}

#[cfg(test)]
mod tests;
