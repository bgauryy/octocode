//! Cargo workspace membership for language-server root discovery.

use super::{CARGO_MANIFEST, lexical};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

mod targets;
pub(super) use targets::TargetRoots;

pub(super) fn manifest(directory: &Path) -> Option<toml::Table> {
    std::fs::read_to_string(directory.join(CARGO_MANIFEST))
        .ok()?
        .parse()
        .ok()
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

fn matches(root: &Path, pattern: &str, candidate: &Path) -> bool {
    let literal_root = PathBuf::from(globset::escape(&root.to_string_lossy()));
    globset::GlobBuilder::new(&lexical(&literal_root.join(pattern)).to_string_lossy())
        .literal_separator(true)
        .backslash_escape(false)
        .build()
        .is_ok_and(|glob| glob.compile_matcher().is_match(candidate))
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
        let tables = std::iter::once(&member).chain(
            member
                .get("target")
                .and_then(toml::Value::as_table)
                .into_iter()
                .flat_map(|targets| targets.values())
                .filter_map(toml::Value::as_table),
        );
        for table in tables {
            for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
                for (name, dependency) in table
                    .get(key)
                    .and_then(toml::Value::as_table)
                    .into_iter()
                    .flatten()
                {
                    let inherited =
                        dependency.get("workspace").and_then(toml::Value::as_bool) == Some(true);
                    let dependency = if inherited {
                        workspace
                            .get("dependencies")
                            .and_then(|value| value.get(name))
                    } else {
                        Some(dependency)
                    };
                    if let Some(path) = dependency
                        .and_then(|value| value.get("path"))
                        .and_then(toml::Value::as_str)
                    {
                        let base = if inherited { root } else { &directory };
                        let path = lexical(&base.join(path));
                        if path.starts_with(root) {
                            pending.push(path);
                        }
                    }
                }
            }
        }
    }
    false
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
mod tests {
    use super::*;

    fn write(root: &Path, relative: &str, content: &str) -> PathBuf {
        let directory = root.join(relative);
        std::fs::create_dir_all(directory.join("src")).expect("package directory");
        std::fs::write(directory.join(CARGO_MANIFEST), content).expect("manifest");
        directory
    }

    fn resolved(package: &Path) -> PathBuf {
        PathBuf::from(
            super::super::resolve_workspace_root_for_file(
                package.join("src/lib.rs").to_string_lossy().into_owned(),
            )
            .expect("LSP root"),
        )
    }

    #[test]
    fn cargo_membership_respects_globs_exclusions_and_nested_workspaces() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = write(
            temporary.path(),
            "workspace",
            "[workspace]\nmembers = [\n 'crates/*', # members may be multiline\n]\nexclude = ['crates/excluded']\n",
        );
        let package = "[package]\nname = 'member'\nversion = '0.1.0'\n";
        let member = write(&root, "crates/member", package);
        let excluded = write(&root, "crates/excluded", package);
        let unrelated = write(&root, "vendor/unrelated", package);
        let nested = write(&root, "crates/nested", "[workspace]\nmembers = ['inner']\n");
        let inner = write(&nested, "inner", package);
        assert_eq!(resolved(&member), root);
        assert_eq!(resolved(&excluded), excluded);
        assert_eq!(resolved(&unrelated), unrelated);
        assert_eq!(resolved(&inner), nested);
    }

    #[test]
    fn cargo_membership_follows_explicit_workspace_outside_the_package_ancestors() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = write(
            temporary.path(),
            "workspace",
            "[workspace]\nmembers = ['../elsewhere/member']\n",
        );
        let member = write(
            temporary.path(),
            "elsewhere/member",
            "[package]\nname = 'member'\nworkspace = '../../workspace'\n",
        );
        assert_eq!(resolved(&member), root);
    }

    #[test]
    fn cargo_membership_follows_transitive_and_inherited_path_dependencies() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = write(
            temporary.path(),
            "workspace[checkout]",
            "[workspace]\nmembers = ['crates/*']\nexclude = ['support/excluded']\n[workspace.dependencies]\nshared = { path = 'support/shared' }\n",
        );
        write(
            &root,
            "crates/app",
            "[package]\nname = 'app'\n[dependencies]\nshared.workspace = true\n[target.'cfg(unix)'.build-dependencies]\nbridge = { path = '../../support/bridge' }\n",
        );
        let shared = write(&root, "support/shared", "[package]\nname = 'shared'\n");
        write(
            &root,
            "support/bridge",
            "[package]\nname = 'bridge'\n[dev-dependencies]\nleaf = { path = '../leaf' }\nexcluded = { path = '../excluded' }\n",
        );
        let leaf = write(&root, "support/leaf", "[package]\nname = 'leaf'\n");
        let excluded = write(&root, "support/excluded", "[package]\nname = 'excluded'\n");
        assert_eq!(resolved(&shared), root);
        assert_eq!(resolved(&leaf), root);
        assert_eq!(resolved(&excluded), excluded);
    }

    #[test]
    fn cargo_exclusions_are_literal_prefixes_overridden_only_by_exact_members() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = write(
            temporary.path(),
            "workspace",
            "[workspace]\nmembers = ['app', 'vendor/explicit', 'vendor/*']\nexclude = ['vendor', 'support/*']\n",
        );
        write(
            &root,
            "app",
            "[package]\nname = 'app'\n[dependencies]\nleaf = { path = '../support/leaf' }\n",
        );
        let package = "[package]\nname = 'p'\n";
        let explicit = write(&root, "vendor/explicit", package);
        let excluded = write(&root, "vendor/expanded", package);
        let leaf = write(&root, "support/leaf", package);
        assert_eq!(resolved(&explicit), root);
        assert_eq!(resolved(&excluded), excluded);
        assert_eq!(
            resolved(&leaf),
            root,
            "exclude entries are not glob patterns"
        );
    }

    #[test]
    fn cargo_workspace_headers_inside_strings_do_not_declare_a_workspace() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = write(
            temporary.path(),
            "package",
            "[package]\nname = 'p'\ndescription = '''\n[workspace]\n'''\n",
        );
        assert!(!declares_cargo_workspace(&root));
        assert_eq!(resolved(&root), root);
    }
}
