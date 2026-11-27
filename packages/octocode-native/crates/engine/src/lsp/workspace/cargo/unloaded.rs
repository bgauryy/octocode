//! The Cargo cache directories rust-analyzer leaves out of its file set.
use super::{cargo_workspace_root, lexical, manifest, member_dirs, path_dependencies, patterns};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The first bytes of a valid `CACHEDIR.TAG` (the Cache Directory Tagging
/// Specification); Cargo writes the file at the root of its target directory.
const CACHEDIR_SIGNATURE: &[u8] = b"Signature: 8a477f597d28d172789f06886806bc55";

/// The tag file that marks a cache directory.
pub const CACHEDIR_TAG: &str = "CACHEDIR.TAG";

/// What rust-analyzer loads of the Cargo workspace it analyzes, while build
/// scripts are off: the directory of every local package (the workspace's
/// members and the path dependencies they reach) except its `target` and
/// `.git`, and the directory of each target a package declares outside its
/// own. Build scripts on, it also loads their `OUT_DIR`s, which lie under the
/// target directory, so this model does not hold then (rust-analyzer's
/// `ProjectWorkspace::to_roots`).
#[derive(Debug, Default)]
pub struct UnloadedCargoCaches {
    /// Local package directories (lexical).
    packages: Vec<PathBuf>,
    /// Directories of targets declared outside their package.
    includes: Vec<PathBuf>,
}

impl UnloadedCargoCaches {
    /// The model for a rust-analyzer rooted at `root`, which analyzes the
    /// Cargo workspace including it. `None` when no Cargo manifest there says
    /// which packages it loads (a `rust-project.json`, or no manifest).
    pub fn of(root: &Path) -> Option<Self> {
        let root = lexical(root);
        let workspace_root = cargo_workspace_root(&root).unwrap_or(root);
        let root_manifest = manifest(&workspace_root)?;
        let empty = toml::Table::new();
        let workspace = root_manifest
            .get("workspace")
            .and_then(toml::Value::as_table)
            .unwrap_or(&empty);
        let mut pending = patterns(workspace, "members")
            .flat_map(|pattern| member_dirs(&workspace_root, pattern))
            .chain(patched(&root_manifest, &workspace_root))
            .collect::<Vec<_>>();
        pending.push(workspace_root.clone());
        let mut found = Self::default();
        let mut visited = HashSet::new();
        while let Some(directory) = pending.pop() {
            if !visited.insert(directory.clone()) {
                continue;
            }
            let Some(member) = manifest(&directory) else {
                continue;
            };
            if member.contains_key("package") {
                found.includes.extend(outside_targets(&member, &directory));
                found.packages.push(directory.clone());
            }
            // Every path dependency is a local package, whichever workspace
            // owns it: rust-analyzer loads it too.
            pending.extend(path_dependencies(
                &member,
                &directory,
                &workspace_root,
                workspace,
            ));
        }
        Some(found)
    }

    /// Whether `dir` is a cache directory (a valid [`CACHEDIR_TAG`]) none of
    /// whose files rust-analyzer loads: no local package and no declared
    /// target lies in it, and every package that holds it holds it in its
    /// `target` or `.git`.
    pub fn contains(&self, dir: &Path) -> bool {
        if !tagged(dir) {
            return false;
        }
        if self
            .packages
            .iter()
            .chain(&self.includes)
            .any(|loaded| loaded.starts_with(dir))
            || self.includes.iter().any(|include| dir.starts_with(include))
        {
            return false;
        }
        self.packages
            .iter()
            .filter(|package| dir.starts_with(package))
            .all(|package| {
                dir.starts_with(package.join("target")) || dir.starts_with(package.join(".git"))
            })
    }
}

/// Whether `dir` holds a [`CACHEDIR_TAG`] that begins with the signature.
fn tagged(dir: &Path) -> bool {
    use std::io::Read;
    let Ok((file, _)) = crate::text::open_regular(&dir.join(CACHEDIR_TAG)) else {
        return false;
    };
    let mut head = Vec::with_capacity(CACHEDIR_SIGNATURE.len());
    file.take(CACHEDIR_SIGNATURE.len() as u64)
        .read_to_end(&mut head)
        .is_ok_and(|_| head == CACHEDIR_SIGNATURE)
}

/// The path packages a workspace root's `[patch]` tables put in the graph.
fn patched(root_manifest: &toml::Table, root: &Path) -> Vec<PathBuf> {
    root_manifest
        .get("patch")
        .and_then(toml::Value::as_table)
        .into_iter()
        .flat_map(|sources| sources.values())
        .filter_map(toml::Value::as_table)
        .flat_map(|patches| patches.values())
        .filter_map(|patch| patch.get("path").and_then(toml::Value::as_str))
        .map(|path| lexical(&root.join(path)))
        .collect()
}

/// The directories of the targets `manifest` (at `package`) declares outside
/// `package`: rust-analyzer loads each target's directory too.
fn outside_targets(manifest: &toml::Table, package: &Path) -> Vec<PathBuf> {
    let single = ["lib"]
        .into_iter()
        .filter_map(|key| manifest.get(key))
        .chain(manifest.get("package").and_then(|p| p.get("build")));
    let listed = ["bin", "test", "bench", "example"]
        .into_iter()
        .filter_map(|key| manifest.get(key).and_then(toml::Value::as_array))
        .flatten();
    single
        .chain(listed)
        .filter_map(|target| match target {
            toml::Value::String(path) => Some(path.as_str()),
            table => table.get("path").and_then(toml::Value::as_str),
        })
        .filter_map(|path| lexical(&package.join(path)).parent().map(Path::to_path_buf))
        .filter(|directory| !directory.starts_with(package))
        .collect()
}
