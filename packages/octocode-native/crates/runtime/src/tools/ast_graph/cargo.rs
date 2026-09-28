use super::graph::normalize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
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
pub(super) fn load_cargo_crates(root: &Path) -> Result<CargoCrates, String> {
    const MAX_METADATA_BYTES: usize = 32 * 1024 * 1024;
    // Resolve cargo from an explicit env-provided path when available rather than
    // trusting the ambient PATH against an untrusted working directory. `--no-deps`
    // keeps metadata to the workspace's own crates, cutting work and attack surface.
    let cargo = std::env::var_os("OCTOCODE_CARGO")
        .or_else(|| std::env::var_os("CARGO"))
        .unwrap_or_else(|| std::ffi::OsString::from("cargo"));
    let mut child = std::process::Command::new(&cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .current_dir(root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "cargo metadata stdout was unavailable".to_owned())?;
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut stdout = stdout;
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 16 * 1024];
        let mut exceeded = false;
        loop {
            let read = stdout.read(&mut chunk).map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            if bytes.len().saturating_add(read) <= MAX_METADATA_BYTES {
                bytes.extend_from_slice(&chunk[..read]);
            } else {
                exceeded = true;
            }
        }
        if exceeded {
            Err("cargo metadata exceeded the 32 MiB output limit".to_owned())
        } else {
            Ok(bytes)
        }
    });
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err(error.to_string());
            }
            Ok(None) if started.elapsed() > std::time::Duration::from_secs(5) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("cargo metadata timed out".into());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| "cargo metadata output reader failed".to_owned())??;
    if !status.success() {
        return Err("cargo metadata exited unsuccessfully".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
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
    let mut libraries: BTreeMap<PathBuf, (String, String, String)> = BTreeMap::new();
    for package in packages {
        let Some(manifest) = package["manifest_path"].as_str() else {
            continue;
        };
        let Some(directory) = Path::new(manifest).parent() else {
            continue;
        };
        let Some(name) = package["name"].as_str() else {
            continue;
        };
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
            let Ok(relative) = Path::new(source).strip_prefix(&root) else {
                continue;
            };
            libraries.insert(
                directory.to_path_buf(),
                (
                    name.to_owned(),
                    crate_name.replace('-', "_"),
                    normalize(&relative.to_string_lossy()),
                ),
            );
        }
    }
    let mut crates = CargoCrates::default();
    for package in packages {
        let Some(manifest) = package["manifest_path"].as_str() else {
            continue;
        };
        let Some(directory) = Path::new(manifest).parent() else {
            continue;
        };
        let scope = if let Ok(relative) = directory.strip_prefix(&root) {
            normalize(&relative.to_string_lossy())
        } else if root.starts_with(directory) {
            String::new()
        } else {
            continue;
        };
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
        let dependencies = candidates
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
            .collect();
        crates.packages.insert(scope, dependencies);
    }
    crates
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
