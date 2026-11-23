use super::*;
use serde_json::json;
use std::fs;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
const OTHER: &str = "89abcdef0123456789abcdef0123456789abcdef";

/// A work tree at `dir` whose `HEAD` holds `head`, with one source file.
fn work_tree(dir: &Path, head: &str) -> String {
    fs::create_dir_all(dir.join(".git")).expect("git dir");
    fs::write(dir.join(".git/HEAD"), format!("{head}\n")).expect("HEAD");
    fs::create_dir_all(dir.join("src")).expect("src");
    let file = dir.join("src/lib.rs");
    fs::write(&file, "fn main() {}\n").expect("source");
    file.to_string_lossy().into_owned()
}

fn workspace() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("workspace");
    let root = dir.path().canonicalize().expect("canonical workspace");
    (dir, root)
}

#[test]
fn a_row_in_one_work_tree_states_its_checked_out_commit() {
    let (_dir, root) = workspace();
    let file = work_tree(&root.join("repo"), SHA);
    let paths = crate::test_support::workspace_policy(&root);
    let row = json!({"files":[{"path":file,"matches":[{"line":1}]}]});
    assert_eq!(
        checkout_commit(&row, &paths, &Value::Null).as_deref(),
        Some(SHA)
    );
    // A row naming no path takes the asked one; an error row states none.
    let asked = json!({"path": root.join("repo/src").to_string_lossy()});
    assert_eq!(
        checkout_commit(&json!({"matchCount":0}), &paths, &asked).as_deref(),
        Some(SHA)
    );
    let refused = json!({"errorCode":"pathPolicyDenied","path":file});
    assert_eq!(checkout_commit(&refused, &paths, &asked), None);
}

#[test]
fn a_branch_resolves_from_its_loose_ref_or_packed_refs() {
    let (_dir, root) = workspace();
    let loose = work_tree(&root.join("loose"), "ref: refs/heads/main");
    fs::create_dir_all(root.join("loose/.git/refs/heads")).expect("refs");
    fs::write(root.join("loose/.git/refs/heads/main"), format!("{SHA}\n")).expect("ref");
    let packed = work_tree(&root.join("packed"), "ref: refs/heads/dev");
    fs::write(
        root.join("packed/.git/packed-refs"),
        format!(
            "# pack-refs with: peeled\n{SHA} refs/heads/main\n{OTHER} refs/heads/dev\n^{SHA}\n"
        ),
    )
    .expect("packed-refs");
    let paths = crate::test_support::workspace_policy(&root);
    let commit = |file: &str| checkout_commit(&json!({"path":file}), &paths, &Value::Null);
    assert_eq!(commit(&loose).as_deref(), Some(SHA));
    assert_eq!(commit(&packed).as_deref(), Some(OTHER));
}

#[test]
fn a_linked_work_tree_reads_its_own_head_and_the_shared_refs() {
    let (_dir, root) = workspace();
    work_tree(&root.join("main"), "ref: refs/heads/main");
    let linked_git = root.join("main/.git/worktrees/feature");
    fs::create_dir_all(&linked_git).expect("linked git dir");
    fs::write(linked_git.join("HEAD"), "ref: refs/heads/feature\n").expect("HEAD");
    fs::write(linked_git.join("commondir"), "../..\n").expect("commondir");
    fs::create_dir_all(root.join("main/.git/refs/heads")).expect("refs");
    fs::write(
        root.join("main/.git/refs/heads/feature"),
        format!("{OTHER}\n"),
    )
    .expect("ref");
    let tree = root.join("feature");
    fs::create_dir_all(tree.join("src")).expect("src");
    fs::write(
        tree.join(".git"),
        format!("gitdir: {}\n", linked_git.display()),
    )
    .expect(".git file");
    fs::write(tree.join("src/lib.rs"), "\n").expect("source");
    let paths = crate::test_support::workspace_policy(&root);
    let row = json!({"path": tree.join("src/lib.rs").to_string_lossy()});
    assert_eq!(
        checkout_commit(&row, &paths, &Value::Null).as_deref(),
        Some(OTHER)
    );
}

#[test]
fn paths_in_several_work_trees_or_none_state_no_commit() {
    let (_dir, root) = workspace();
    let one = work_tree(&root.join("one"), SHA);
    let two = work_tree(&root.join("two"), OTHER);
    let loose = root.join("loose.rs");
    fs::write(&loose, "\n").expect("untracked file");
    let paths = crate::test_support::workspace_policy(&root);
    let row = json!({"files":[{"path":one},{"path":two}]});
    assert_eq!(checkout_commit(&row, &paths, &Value::Null), None);
    let outside = json!({"files":[{"path":one},{"path":loose.to_string_lossy()}]});
    assert_eq!(checkout_commit(&outside, &paths, &Value::Null), None);
    // What a walk skipped elsewhere is not evidence of the row.
    let skipped = json!({"files":[{"path":one}],"skippedPaths":{"entries":[{"path":two}]}});
    assert_eq!(
        checkout_commit(&skipped, &paths, &Value::Null).as_deref(),
        Some(SHA)
    );
}

#[test]
fn a_work_tree_above_the_allowed_roots_is_never_read() {
    let (_dir, outer) = workspace();
    let file = work_tree(&outer, SHA);
    // The workspace is a folder inside the outer work tree, without its own.
    let paths = crate::test_support::workspace_policy(&outer.join("src"));
    assert_eq!(
        checkout_commit(&json!({"path":file}), &paths, &Value::Null),
        None
    );
}

#[test]
fn a_head_that_climbs_out_of_refs_or_holds_no_object_id_states_nothing() {
    let (_dir, root) = workspace();
    let climbs = work_tree(&root.join("climbs"), "ref: refs/../../secret");
    fs::write(root.join("climbs/secret"), format!("{SHA}\n")).expect("file");
    let unborn = work_tree(&root.join("unborn"), "ref: refs/heads/main");
    let garbage = work_tree(&root.join("garbage"), "not an id");
    let paths = crate::test_support::workspace_policy(&root);
    for file in [climbs, unborn, garbage] {
        assert_eq!(
            checkout_commit(&json!({"path":file}), &paths, &Value::Null),
            None,
            "{file}"
        );
    }
}
