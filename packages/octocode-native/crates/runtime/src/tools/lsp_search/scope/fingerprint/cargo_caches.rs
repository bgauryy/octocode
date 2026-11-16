//! Which Cargo cache directories rust-analyzer leaves out
//! ([`UnloadedCargoCaches`]).
use octocode_engine::lsp::workspace::{CACHEDIR_TAG, UnloadedCargoCaches};
use std::path::Path;

fn write(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
    std::fs::write(path, content).expect("file");
}

fn tag(root: &Path, dir: &str) {
    write(
        root,
        &format!("{dir}/{CACHEDIR_TAG}"),
        "Signature: 8a477f597d28d172789f06886806bc55\n# cargo\n",
    );
}

const PACKAGE: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

/// A package's tagged `target` is left out; a tagged directory elsewhere in
/// it, an untagged `target`, or a forged tag is loaded.
#[test]
fn a_package_target_is_unloaded_only_when_tagged() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    write(root, "Cargo.toml", PACKAGE);
    tag(root, "target");
    tag(root, "build");
    write(root, "other/CACHEDIR.TAG", "Signature: not cargo\n");
    std::fs::create_dir_all(root.join("plain")).expect("plain");
    let caches = UnloadedCargoCaches::of(root).expect("cargo workspace");
    assert!(caches.contains(&root.join("target")));
    assert!(
        !caches.contains(&root.join("build")),
        "a target directory inside the package is loaded"
    );
    assert!(!caches.contains(&root.join("other")), "an invalid tag");
    assert!(!caches.contains(&root.join("plain")), "no tag");
}

/// A cache directory holding a workspace package, or below a package that
/// does not hold it in its `target`, is loaded.
#[test]
fn a_cache_holding_or_inside_a_loaded_package_is_loaded() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    write(
        root,
        "Cargo.toml",
        &format!("{PACKAGE}[workspace]\nmembers = [\"target/member\"]\n"),
    );
    write(root, "target/member/Cargo.toml", PACKAGE);
    tag(root, "target");
    write(root, "fixtures/crate/Cargo.toml", PACKAGE);
    tag(root, "fixtures/crate/target");
    let caches = UnloadedCargoCaches::of(root).expect("cargo workspace");
    assert!(
        !caches.contains(&root.join("target")),
        "it holds a member package"
    );
    assert!(
        !caches.contains(&root.join("fixtures/crate/target")),
        "the root package loads a non-member crate's directory, its target too"
    );
}

/// In a virtual workspace, the root's and each member's tagged target are
/// unloaded; a path dependency's directory counts as a package.
#[test]
fn a_virtual_workspace_leaves_out_every_package_target() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path();
    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/*\"]\n",
    );
    write(
        root,
        "crates/a/Cargo.toml",
        &format!("{PACKAGE}[dependencies]\ndep = {{ path = \"../../vendor/dep\" }}\n"),
    );
    write(root, "vendor/dep/Cargo.toml", PACKAGE);
    tag(root, "target");
    tag(root, "crates/a/target");
    tag(root, "vendor/dep/cache");
    let caches = UnloadedCargoCaches::of(&root.join("crates/a")).expect("cargo workspace");
    assert!(caches.contains(&root.join("target")));
    assert!(caches.contains(&root.join("crates/a/target")));
    assert!(
        !caches.contains(&root.join("vendor/dep/cache")),
        "a path dependency's own directory is loaded"
    );
}

/// No Cargo manifest: no model, nothing is left out.
#[test]
fn no_manifest_no_model() {
    let dir = tempfile::tempdir().expect("dir");
    assert!(UnloadedCargoCaches::of(dir.path()).is_none());
}
