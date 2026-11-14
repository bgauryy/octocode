//! A dependents read outlives its request while what it read holds.
use super::*;
use octocode_engine::stamp::settle_written_so_far;

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    std::fs::write(path, text).expect("write");
}

/// A Go repository: module `example.com/a` holding the anchor, and a
/// project `b` that requires it.
fn repository() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    std::fs::create_dir_all(root.join(".git")).expect("git");
    write(&root.join("a/go.mod"), "module example.com/a\n");
    write(
        &root.join("a/x.go"),
        "package a\n\ntype Querier interface{}\n",
    );
    write(
        &root.join("b/go.mod"),
        "module example.com/b\n\nrequire example.com/a v1.0.0\n",
    );
    (dir, root)
}

fn read(root: &Path) -> Option<Dependents> {
    kept_or_read(
        root,
        &root.join("a"),
        &root.join("a/x.go"),
        "go",
        &octocode_engine::lsp::config::LspDiscoveryOptions::default(),
        &|| false,
    )
}

/// Once the scope is settled, a repeat is served without a walk; a build file
/// added under it is a new input, so the next read walks and finds it.
#[test]
fn a_settled_read_serves_repeats_until_a_build_file_changes() {
    let (_dir, root) = repository();
    settle_written_so_far();
    let first = read(&root).expect("read");
    assert!(first.includes(&root.join("b")));
    assert!(!first.includes(&root.join("c")));
    assert_eq!(reads::of(&root), 1);
    assert_eq!(read(&root), Some(first.clone()));
    assert_eq!(reads::of(&root), 1, "a repeat reads the kept answer");
    write(
        &root.join("c/go.mod"),
        "module example.com/c\n\nrequire example.com/a v1.0.0\n",
    );
    settle_written_so_far();
    let after = read(&root).expect("read again");
    assert_eq!(reads::of(&root), 2);
    assert!(after.includes(&root.join("c")));
    // An edit of a read build file changes its stamp too.
    write(&root.join("b/go.mod"), "module example.com/b\n");
    settle_written_so_far();
    let edited = read(&root).expect("read after the edit");
    assert_eq!(reads::of(&root), 3);
    assert!(!edited.includes(&root.join("b")));
}

/// The dependents walk follows the one walk policy of every local tool: a
/// dependency or build-output directory (`node_modules`, `target`) is never
/// entered, so a package vendored there is not a dependent project.
#[test]
fn the_walk_leaves_out_what_every_local_walk_prunes() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    write(&root.join("a/go.mod"), "module example.com/a\n");
    write(&root.join("a/x.go"), "package a\n");
    let requires = "module example.com/d\n\nrequire example.com/a v1.0.0\n";
    write(&root.join("b/go.mod"), requires);
    write(&root.join("node_modules/d/go.mod"), requires);
    write(&root.join("target/e/go.mod"), requires);
    let found = read(&root).expect("read");
    assert!(found.includes(&root.join("b")), "{found:?}");
    assert!(!found.includes(&root.join("node_modules/d")), "{found:?}");
    assert!(!found.includes(&root.join("target/e")), "{found:?}");
}

/// A read whose inputs were written within the settle window is not kept.
#[test]
fn an_unsettled_read_is_not_kept() {
    let (_dir, root) = repository();
    assert!(read(&root).is_some());
    assert!(read(&root).is_some());
    assert_eq!(reads::of(&root), 2);
}

/// A scope that is its repository's root widens nowhere above it: a write
/// beside the repository leaves the kept read standing.
#[test]
fn a_write_above_the_repository_keeps_the_read() {
    let outer = tempfile::tempdir().expect("dir");
    let outer_path = outer.path().canonicalize().expect("canonical");
    let root = outer_path.join("repo");
    std::fs::create_dir_all(root.join(".git")).expect("git");
    write(&root.join("a/go.mod"), "module example.com/a\n");
    write(&root.join("a/x.go"), "package a\n");
    write(
        &root.join("b/go.mod"),
        "module example.com/b\n\nrequire example.com/a v1.0.0\n",
    );
    settle_written_so_far();
    assert!(read(&root).expect("read").includes(&root.join("b")));
    write(&outer_path.join("beside.txt"), "x");
    settle_written_so_far();
    assert!(read(&root).is_some());
    assert_eq!(reads::of(&root), 1);
}

/// A Cargo project depends on the anchor's package only through a
/// dependency whose source is that package: a snapshot of the workspace,
/// whose crate depends on the snapshot's own same-named package by path, is
/// another project, while one whose path dependency is the anchor's package
/// depends on it, directly or through another local package.
#[test]
fn a_cargo_snapshot_depending_on_its_own_copy_is_no_dependent() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().canonicalize().expect("canonical");
    std::fs::create_dir_all(root.join(".git")).expect("git");
    let package = |name: &str, deps: &str| {
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\n{deps}")
    };
    write(&root.join("a/Cargo.toml"), &package("a", ""));
    write(&root.join("a/src/lib.rs"), "pub fn shared() {}\n");
    write(
        &root.join("b/Cargo.toml"),
        &package("b", "a = { path = \"../a\" }\n"),
    );
    write(
        &root.join("c/Cargo.toml"),
        &package("c", "b = { path = \"../b\" }\n"),
    );
    write(
        &root.join("copy/Cargo.toml"),
        "[workspace]\nmembers = [\"a\", \"b\"]\n",
    );
    write(&root.join("copy/a/Cargo.toml"), &package("a", ""));
    write(
        &root.join("copy/b/Cargo.toml"),
        &package("b", "a = { path = \"../a\" }\n"),
    );
    let found = kept_or_read(
        &root,
        &root.join("a"),
        &root.join("a/src/lib.rs"),
        "rs",
        &octocode_engine::lsp::config::LspDiscoveryOptions::default(),
        &|| false,
    )
    .expect("read");
    assert!(found.includes(&root.join("b")), "{found:?}");
    assert!(found.includes(&root.join("c")), "through b: {found:?}");
    assert!(!found.includes(&root.join("copy")), "{found:?}");
}
