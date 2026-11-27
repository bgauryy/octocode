//! Which Cargo workspaces enclose the scanned Rust files, and one
//! `cargo metadata` per workspace, merged into one crate map.
use super::{CargoCrates, parse_cargo_crates};
use octocode_runtime_base::execution::cancel::CancellationCheck;
use octocode_tools_shared::bounded_process::run_bounded;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Duration,
};

/// The nearest `Cargo.toml` enclosing each Rust file of the scan, inside or
/// above `root` (a scan of a crate's subdirectory belongs to that crate), and
/// how many Rust files no manifest encloses.
pub(in crate::ast_graph) fn enclosing_manifests<'f>(
    root: &Path,
    rust_files: impl Iterator<Item = &'f str>,
) -> (BTreeSet<PathBuf>, usize) {
    let mut nearest = BTreeMap::new();
    let mut manifests = BTreeSet::new();
    let mut loose = 0;
    for file in rust_files {
        match root
            .join(file)
            .parent()
            .and_then(|dir| nearest_manifest(dir, &mut nearest))
        {
            Some(manifest) => {
                manifests.insert(manifest);
            }
            None => loose += 1,
        }
    }
    (manifests, loose)
}

/// The first `Cargo.toml` at or above `dir`, memoized per visited directory.
fn nearest_manifest(dir: &Path, memo: &mut BTreeMap<PathBuf, Option<PathBuf>>) -> Option<PathBuf> {
    let mut walked = Vec::new();
    let mut at = Some(dir);
    let found = loop {
        let Some(current) = at else { break None };
        if let Some(known) = memo.get(current) {
            break known.clone();
        }
        walked.push(current.to_path_buf());
        let manifest = current.join("Cargo.toml");
        if manifest.is_file() {
            break Some(manifest);
        }
        at = current.parent();
    };
    for directory in walked {
        memo.insert(directory, found.clone());
    }
    found
}

// Refresh metadata on every graph build. A root manifest/lock stamp cannot
// account for member manifests, glob membership, Cargo config, or environment.
// Do not cache an incomplete input fingerprint.
/// The crates of every Cargo workspace that encloses one of `manifests`.
/// One `cargo metadata` runs per workspace (a manifest an earlier run listed
/// is covered), and the packages of all runs resolve together, so a scan of a
/// folder of crates, of several workspaces or of a crate's subdirectory links
/// its crate names. Each failure names its manifest; the other workspaces
/// still link. `cargo` is the configured `OCTOCODE_CARGO` path; `None` runs
/// `cargo` from PATH. Only the request's cancellation bounds the runs.
pub(in crate::ast_graph) fn load_cargo_crates(
    root: &Path,
    manifests: &BTreeSet<PathBuf>,
    cargo: Option<&str>,
    cancel: &dyn CancellationCheck,
) -> (CargoCrates, Vec<String>) {
    let mut packages = Vec::new();
    let mut listed = BTreeSet::new();
    let mut failures = Vec::new();
    for manifest in manifests {
        let manifest = manifest.canonicalize().unwrap_or_else(|_| manifest.clone());
        if listed.contains(&manifest) {
            continue;
        }
        if let Err(error) = cancel.check() {
            failures.push(error);
            break;
        }
        match metadata(&manifest, cargo, cancel, Duration::MAX) {
            Ok(value) => {
                for package in value["packages"].as_array().into_iter().flatten() {
                    let Some(path) = package["manifest_path"].as_str() else {
                        continue;
                    };
                    let path = Path::new(path)
                        .canonicalize()
                        .unwrap_or_else(|_| PathBuf::from(path));
                    if listed.insert(path) {
                        packages.push(package.clone());
                    }
                }
            }
            Err(error) => {
                let shown = manifest.strip_prefix(root).unwrap_or(&manifest);
                failures.push(format!("{}: {error}", shown.display()));
            }
        }
        listed.insert(manifest);
    }
    let packages = serde_json::Value::Array(packages);
    (
        parse_cargo_crates(root, &serde_json::json!({ "packages": packages })),
        failures,
    )
}

/// `cargo metadata` for the workspace of `manifest`, bounded by `limit`.
fn metadata(
    manifest: &Path,
    cargo: Option<&str>,
    cancel: &dyn CancellationCheck,
    limit: Duration,
) -> Result<serde_json::Value, String> {
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
            "--manifest-path",
        ])
        .arg(manifest);
    if let Some(directory) = manifest.parent() {
        command.current_dir(directory);
    }
    let output = run_bounded(command, "cargo metadata", limit, usize::MAX, cancel)?;
    if !output.status.success() {
        let reason = String::from_utf8_lossy(&output.stderr);
        let reason = reason
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        return Err(format!("cargo metadata exited unsuccessfully: {reason}"));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
