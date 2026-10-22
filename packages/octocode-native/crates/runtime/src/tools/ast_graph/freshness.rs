//! What a built graph depends on besides the sources it parsed, and how to
//! tell it has not changed: a persisted snapshot (`graph ingest` reuse,
//! `graph query stale`) records digests of the linking configs it read.
//!
//! Linking reads manifests and compiler configs (`tsconfig.json`,
//! `package.json`, `Cargo.toml`, `go.mod`, ...) beside the scanned files and
//! up the root's ancestors inside its repository, so those directories are
//! the scope of every freshness check.
use crate::policy::path::PathPolicy;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Config files hashed in full up to this size; a larger one (a lockfile)
/// is fingerprinted by size and modification time.
const MAX_DIGESTED_CONFIG_BYTES: usize = 16 << 20;

/// Files whose change invalidates everything below their directory: the
/// manifests, lockfiles and compiler configs module resolution reads.
pub(crate) fn is_global_config(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name,
        "package.json"
            | "package-lock.json"
            | "yarn.lock"
            | "pnpm-lock.yaml"
            | "bun.lockb"
            | "bun.lock"
            | "Cargo.toml"
            | "Cargo.lock"
            | "go.mod"
            | "go.sum"
            | "pyproject.toml"
            | "setup.py"
            | "setup.cfg"
            | "poetry.lock"
            | "uv.lock"
            | "Pipfile.lock"
            | "build.gradle"
            | "build.gradle.kts"
            | "pom.xml"
            | "CMakeLists.txt"
            | "compile_commands.json"
    ) || (name.starts_with("tsconfig") && name.ends_with(".json"))
        || name == "jsconfig.json"
        || (name.starts_with("requirements") && name.ends_with(".txt"))
}

/// The directories a build of `root` read: `root`, every directory holding
/// one of `files` (root-relative, `/`-separated) up to `root`, and the
/// root's ancestors inside its repository (outer manifests are read up to
/// the repository root).
pub(super) fn scope_dirs<'a>(
    root: &Path,
    files: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<PathBuf> {
    let mut dirs = BTreeSet::from([root.to_path_buf()]);
    for file in files {
        let mut rest = file;
        while let Some((dir, _)) = rest.rsplit_once('/') {
            if !dirs.insert(root.join(dir)) {
                break;
            }
            rest = dir;
        }
    }
    if let Some(repository) = octocode_engine::search::repository::nearest(root) {
        dirs.extend(
            root.ancestors()
                .skip(1)
                .take_while(|dir| dir.starts_with(repository))
                .map(Path::to_path_buf),
        );
    }
    dirs
}

/// The linking configs in `dirs` the path policy lets a build read, keyed by
/// their path relative to `root` (`../package.json` for an ancestor's), each
/// with a content fingerprint.
pub(crate) fn config_inputs(
    paths: &PathPolicy,
    root: &Path,
    dirs: &BTreeSet<PathBuf>,
) -> BTreeMap<String, String> {
    let mut inputs = BTreeMap::new();
    for dir in dirs {
        let Some(prefix) = relative_dir(root, dir) else {
            continue;
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_global_config(&name) || !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                continue;
            }
            let path = entry.path();
            let Ok(valid) = paths.validate_read(&path) else {
                continue;
            };
            if let Some(fingerprint) = fingerprint(&valid.canonical) {
                inputs.insert(format!("{prefix}{name}"), fingerprint);
            }
        }
    }
    inputs
}

/// `dir` relative to `root` as a key prefix (`""`, `src/`, `../`); `None`
/// for a directory that is neither below nor above `root`.
fn relative_dir(root: &Path, dir: &Path) -> Option<String> {
    if let Ok(below) = dir.strip_prefix(root) {
        let below = octocode_engine::text::slash_path(below);
        return Some(if below.is_empty() {
            below
        } else {
            format!("{below}/")
        });
    }
    let above = root.strip_prefix(dir).ok()?.components().count();
    Some("../".repeat(above))
}

fn fingerprint(path: &Path) -> Option<String> {
    match crate::tools::source::read_bounded(path, MAX_DIGESTED_CONFIG_BYTES) {
        Ok(bytes) => Some(octocode_engine::digest::sha256(&bytes)),
        Err(_) => {
            let meta = std::fs::metadata(path).ok()?;
            let modified = meta
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_nanos();
            Some(format!("size:{}:mtime:{modified}", meta.len()))
        }
    }
}

/// How the recorded config inputs differ from the current ones:
/// `(key, "changed" | "missing" | "added")`, sorted by key.
pub(crate) fn config_changes(
    recorded: &BTreeMap<String, String>,
    current: &BTreeMap<String, String>,
) -> Vec<(String, &'static str)> {
    let mut changes = Vec::new();
    for (key, digest) in recorded {
        match current.get(key) {
            None => changes.push((key.clone(), "missing")),
            Some(now) if now != digest => changes.push((key.clone(), "changed")),
            Some(_) => {}
        }
    }
    for key in current.keys() {
        if !recorded.contains_key(key) {
            changes.push((key.clone(), "added"));
        }
    }
    changes.sort();
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_inputs_cover_scanned_dirs_and_repository_ancestors() {
        let repo = crate::tools::test_support::Fixture::new();
        let base = repo.0.as_path();
        std::fs::create_dir_all(base.join(".git")).expect("git marker");
        repo.write("package.json", "{\"name\":\"repo\"}");
        repo.write("app/tsconfig.json", "{}");
        repo.write("app/src/a.ts", "export const a = 1;\n");
        repo.write("app/src/README.md", "not an input\n");
        repo.write("app/empty/tsconfig.json", "{}");
        let root = base.join("app");
        let dirs = scope_dirs(&root, ["src/a.ts"]);
        let paths = crate::tools::test_support::workspace_policy(base);
        let inputs = config_inputs(&paths, &root, &dirs);
        assert_eq!(
            inputs.keys().collect::<Vec<_>>(),
            ["../package.json", "tsconfig.json"],
            "a directory holding no scanned file is not a build input"
        );

        repo.write("app/tsconfig.json", "{\"compilerOptions\":{}}");
        repo.write("app/src/go.mod", "module x\n");
        std::fs::remove_file(base.join("package.json")).expect("remove");
        let now = config_inputs(&paths, &root, &dirs);
        assert_eq!(
            config_changes(&inputs, &now),
            [
                ("../package.json".to_owned(), "missing"),
                ("src/go.mod".to_owned(), "added"),
                ("tsconfig.json".to_owned(), "changed"),
            ]
        );
    }

    /// A Jujutsu checkout bounds the ancestor inputs like a git one: the
    /// walk applies `.gitignore` files up to either root.
    #[test]
    fn every_repository_marker_bounds_the_ancestor_dirs() {
        for marker in octocode_engine::search::repository::MARKERS {
            let repo = crate::tools::test_support::Fixture::new();
            let base = repo.0.as_path();
            std::fs::create_dir_all(base.join(marker)).expect("repository marker");
            repo.write("packages/app/src/a.ts", "export const a = 1;\n");
            let root = base.join("packages/app");
            let dirs = scope_dirs(&root, ["src/a.ts"]);
            assert!(dirs.contains(base), "{marker}: {dirs:?}");
            assert!(dirs.contains(&base.join("packages")), "{marker}: {dirs:?}");
        }
    }

    #[test]
    fn config_inputs_outside_the_path_policy_are_not_read() {
        let repo = crate::tools::test_support::Fixture::new();
        let base = repo.0.as_path();
        std::fs::create_dir_all(base.join(".git")).expect("git marker");
        repo.write("package.json", "{}");
        repo.write("app/src/a.ts", "export const a = 1;\n");
        let root = base.join("app");
        let paths = crate::tools::test_support::workspace_policy(&root);
        let inputs = config_inputs(&paths, &root, &scope_dirs(&root, ["src/a.ts"]));
        assert!(inputs.is_empty(), "{inputs:?}");
    }
}
