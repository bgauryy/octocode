//! The build's identity: a digest of every input that shapes a cursor's pages,
//! so a cursor minted by another build is never replayed against pages this
//! build cuts differently. It only has to differ between builds: a toolchain
//! change that changes the hash just invalidates saved cursors.

use super::inputs;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Every file under `dir`, recursively; a missing `dir` has none.
fn files_under(dir: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            files_under(&path, found)?;
        } else {
            found.push(path);
        }
    }
    Ok(())
}

/// What one crate contributes: its sources, manifest and build script.
fn crate_inputs(crate_dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for source in ["src", "build"] {
        files_under(&crate_dir.join(source), &mut found)?;
    }
    for file in ["build.rs", "Cargo.toml"] {
        let path = crate_dir.join(file);
        if path.is_file() {
            found.push(path);
        }
    }
    Ok(found)
}

pub(super) fn emit_build_identity() -> Result<(), Box<dyn Error>> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let workspace = manifest_dir.join("../..").canonicalize()?;
    let relative = |path: &Path| -> String {
        path.strip_prefix(&workspace)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    };
    // (label, path) of every hashed file; labels never hold the checkout path.
    let mut inputs_hashed: Vec<(String, PathBuf)> = Vec::new();
    let mut roots = Vec::new();
    for dir in inputs::path_dependency_dirs(&manifest_dir)? {
        roots.push(relative(&dir));
        for file in crate_inputs(&dir)? {
            inputs_hashed.push((relative(&file), file));
        }
    }
    let lock = workspace.join("Cargo.lock");
    roots.push(relative(&lock));
    inputs_hashed.push((relative(&lock), lock));
    for (label, path) in inputs::contract_inputs(&manifest_dir) {
        roots.push(label.clone());
        inputs_hashed.push((label, path.canonicalize()?));
    }
    inputs_hashed.sort();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (label, path) in &inputs_hashed {
        label.hash(&mut hasher);
        std::fs::read(path)?.hash(&mut hasher);
        println!("cargo:rerun-if-changed={}", path.display());
    }
    // A file added later changes its directory's mtime, which cargo scans.
    for dir in inputs::path_dependency_dirs(&manifest_dir)? {
        for source in ["src", "build"] {
            println!("cargo:rerun-if-changed={}", dir.join(source).display());
        }
    }
    println!(
        "cargo:rustc-env=OCTOCODE_BUILD_IDENTITY={:016x}",
        hasher.finish()
    );
    println!(
        "cargo:rustc-env=OCTOCODE_BUILD_IDENTITY_ROOTS={}",
        roots.join(";")
    );
    Ok(())
}
