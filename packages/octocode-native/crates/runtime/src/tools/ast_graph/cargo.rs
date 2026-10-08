use super::graph::normalize;
use crate::tools::{bounded_process::run_bounded, cancel::CancellationCheck};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Duration,
};

/// Cargo names belong to the importing package, not the whole workspace.
#[derive(Default)]
pub(super) struct CargoCrates {
    packages: BTreeMap<String, BTreeMap<String, String>>,
}

impl CargoCrates {
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

// Refresh metadata on every graph build. A root manifest/lock stamp cannot
// account for member manifests, glob membership, Cargo config, or environment.
// Keep the bounded offline process; do not cache an incomplete input fingerprint.
/// `cargo` is the configured `OCTOCODE_CARGO` path; `None` runs `cargo`
/// from PATH.
pub(super) fn load_cargo_crates(
    root: &Path,
    cargo: Option<&str>,
    cancel: &dyn CancellationCheck,
) -> Result<CargoCrates, String> {
    const MAX_METADATA_BYTES: usize = 32 * 1024 * 1024;
    // `--no-deps` keeps metadata to the workspace's own crates, cutting work
    // and attack surface.
    let mut command = std::process::Command::new(cargo.unwrap_or("cargo"));
    command
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(root);
    let output = run_bounded(
        command,
        "cargo metadata",
        Duration::from_secs(5),
        MAX_METADATA_BYTES,
        cancel,
    )?;
    if !output.status.success() {
        return Err("cargo metadata exited unsuccessfully".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
    Ok(parse_cargo_crates(root, &value))
}

fn parse_cargo_crates(root: &Path, value: &serde_json::Value) -> CargoCrates {
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
    fn cargo_metadata_runs_the_configured_cargo_only() {
        let root = tempfile::tempdir().expect("fixture");
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )
        .expect("manifest");
        // The configured path is used as-is: no ambient `CARGO` fallback.
        let missing = root.path().join("no-such-cargo");
        let result = super::load_cargo_crates(
            root.path(),
            missing.to_str(),
            &crate::tools::cancel::NeverCancel,
        );
        assert!(result.is_err(), "a missing configured cargo cannot run");
    }

    /// L2: a cancelled build stops `cargo metadata` as cancelled (not at the
    /// 5 s limit), and its process group dies with it.
    #[cfg(unix)]
    #[test]
    fn cancellation_stops_cargo_metadata_and_its_children() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::Duration;
        /// Cancels once the fake cargo has started its child.
        struct CancelOnceStarted(std::path::PathBuf);
        impl crate::tools::cancel::CancellationCheck for CancelOnceStarted {
            fn check(&self) -> Result<(), String> {
                let started = std::fs::read_to_string(&self.0).is_ok_and(|pid| pid.ends_with('\n'));
                if started {
                    Err("cancelled".into())
                } else {
                    Ok(())
                }
            }
        }
        let root = tempfile::tempdir().expect("fixture");
        let pidfile = root.path().join("child.pid");
        let cargo = root.path().join("fake-cargo");
        std::fs::write(
            &cargo,
            format!(
                "#!/bin/sh\nsleep 30 & echo $! > '{}'\nwait\n",
                pidfile.display()
            ),
        )
        .expect("fake cargo");
        std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let cancel = CancelOnceStarted(pidfile.clone());
        let result = super::load_cargo_crates(root.path(), cargo.to_str(), &cancel);
        assert_eq!(result.err().as_deref(), Some("cancelled"));
        let pid: u32 = std::fs::read_to_string(&pidfile)
            .expect("pid")
            .trim()
            .parse()
            .expect("pid number");
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            !crate::process_status::is_alive(pid),
            "child {pid} survived"
        );
    }

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
