//! The build's identity: a digest of every input that shapes a cursor's pages,
//! so a cursor minted by another build is never replayed against pages this
//! build cuts differently. It only has to differ between builds: a toolchain
//! change that changes the hash just invalidates saved cursors.

use super::inputs;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

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
        for file in inputs::crate_sources(&dir)? {
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
    // Each hashed file is watched one by one, never its directory: a directory
    // watch would rerun this script, and rebuild every crate linking the
    // runtime, on an edit to a test-only file. A new source file reaches the
    // build only through a declaration or `include*!` in a file watched here.
    for (label, path) in &inputs_hashed {
        label.hash(&mut hasher);
        std::fs::read(path)?.hash(&mut hasher);
        println!("cargo:rerun-if-changed={}", path.display());
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
