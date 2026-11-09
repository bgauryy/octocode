//! The freshness check reads a snapshot's files only through the path
//! policy: its keys are data, and one that leaves the allowed roots names
//! nothing the check may look at.
use super::*;
use crate::tools::test_support::{Fixture, workspace_policy};
use format::{GraphTables, NONE, NodeKind, NodeRec};

fn file_node(key: u32) -> NodeRec {
    NodeRec {
        kind: NodeKind::File,
        flags: 0,
        key,
        name: key,
        detail: NONE,
        file: key,
        parent: NONE,
        line: NONE,
        end_line: NONE,
    }
}

/// Keys that climb out of the scanned root to files outside the allowed
/// roots are `missing`, whether the file there matches the recorded digest
/// or exists at all: the check is no oracle for what lies outside.
#[test]
fn snapshot_keys_outside_the_roots_are_missing_whatever_is_there() {
    let dir = Fixture::new();
    dir.write("app/kept.ts", "export const kept = 1;\n");
    let root = dir.0.join("app");
    std::fs::write(dir.0.join("secret.txt"), "outside").expect("outside");
    std::fs::write(dir.0.join("other.txt"), "outside").expect("outside");
    let mut tables = GraphTables {
        strings: vec![
            "../secret.txt".into(),
            octocode_engine::digest::sha256(b"outside"),
            "../other.txt".into(),
        ]
        .into(),
        nodes: vec![file_node(0), file_node(2)],
        // The first carries the outside file's digest; the second no digest.
        digests: vec![(0, 1)],
        ..GraphTables::default()
    };
    tables.index();
    let manifest = serde_json::json!({"scan": {}, "registryExtensions": ["ts"], "inputs": {}});
    let changes = snapshot_changes(&workspace_policy(&root), &root, &tables, &manifest, false);
    let status = |file: &str| {
        changes
            .changes
            .iter()
            .find(|change| change.file == file)
            .map(|change| change.status)
    };
    assert_eq!(status("../secret.txt"), Some("missing"));
    assert_eq!(status("../other.txt"), Some("missing"));
}

/// Inside the root, a file is read where it is; a symlink is followed only
/// where the policy admits its target, a symlinked directory only where it
/// stays inside the roots, and a withheld name is never read.
#[cfg(unix)]
#[test]
fn readable_files_follow_the_policy_for_symlinks_and_withheld_names() {
    let dir = Fixture::new();
    dir.write("app/a.ts", "a");
    dir.write("app/inner/b.ts", "b");
    dir.write("app/.env", "SECRET=1");
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(outside.path().join("c.ts"), "c").expect("c");
    let root = dir.0.join("app");
    std::os::unix::fs::symlink(root.join("a.ts"), root.join("in_link.ts")).expect("link");
    std::os::unix::fs::symlink(outside.path().join("c.ts"), root.join("out_link.ts"))
        .expect("link");
    std::os::unix::fs::symlink(outside.path(), root.join("out_dir")).expect("dir link");
    let paths = workspace_policy(&root);
    let mut readable = Readable::new(&paths, &root);
    let real = std::fs::canonicalize(&root).expect("root");
    assert_eq!(readable.path("a.ts"), Some(real.join("a.ts")));
    assert_eq!(readable.path("inner/b.ts"), Some(real.join("inner/b.ts")));
    assert_eq!(readable.path("in_link.ts"), Some(real.join("a.ts")));
    for refused in [
        "out_link.ts",
        "out_dir/c.ts",
        ".env",
        "missing.ts",
        "/etc/hosts",
        "inner/../a.ts",
    ] {
        assert_eq!(readable.path(refused), None, "{refused}");
    }
}
