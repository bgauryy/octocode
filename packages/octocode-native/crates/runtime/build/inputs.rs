//! The files outside a crate's own sources that its build reads: the contract
//! `@octocodeai/config` generates, and the crates it links. [`embed`],
//! [`config_gen`] and [`identity`] all take their lists from here, so a file
//! one of them watches is never missing from another.

use std::error::Error;
use std::path::{Path, PathBuf};

/// Files of the generated tool contract, relative to its `contract/` directory.
pub(super) const CONTRACT_FILES: [&str; 5] = [
    "tool-contract.json",
    "contract-fixtures.json",
    "provenance.json",
    "tool_types.rs",
    "cli-presentation.json",
];

/// The generated configuration contract, relative to the config package.
pub(super) const CONFIG_CONTRACT: &str = "config-contract.json";

/// A file of the `@octocodeai/config` package.
pub(super) fn config_path(manifest_dir: &Path, name: &str) -> PathBuf {
    manifest_dir.join(format!("../../../../packages/octocode-config/{name}"))
}

/// Every contract input, as `(label, path)`; the label does not depend on where
/// the checkout lives.
pub(super) fn contract_inputs(manifest_dir: &Path) -> Vec<(String, PathBuf)> {
    let contract = CONTRACT_FILES.iter().map(|name| format!("contract/{name}"));
    std::iter::once(CONFIG_CONTRACT.to_string())
        .chain(contract)
        .map(|relative| {
            (
                format!("config/{relative}"),
                config_path(manifest_dir, &relative),
            )
        })
        .collect()
}

/// `crate_dir` and every crate directory it reaches through `path = ...`
/// entries of the build and normal dependency tables, transitively, so a crate
/// added to a manifest is covered without a list to update. Canonical paths.
pub(super) fn path_dependency_dirs(crate_dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut found = Vec::new();
    collect_path_dependencies(crate_dir, &mut found)?;
    Ok(found)
}

fn collect_path_dependencies(dir: &Path, found: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    let dir = dir.canonicalize()?;
    if found.contains(&dir) {
        return Ok(());
    }
    found.push(dir.clone());
    let manifest: toml::Table = toml::from_str(&std::fs::read_to_string(dir.join("Cargo.toml"))?)?;
    let mut scopes = vec![&manifest];
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        scopes.extend(targets.values().filter_map(toml::Value::as_table));
    }
    for scope in scopes {
        for key in ["dependencies", "build-dependencies"] {
            let Some(table) = scope.get(key).and_then(toml::Value::as_table) else {
                continue;
            };
            for path in table
                .values()
                .filter_map(|dependency| dependency.get("path")?.as_str())
            {
                collect_path_dependencies(&dir.join(path), found)?;
            }
        }
    }
    Ok(())
}
