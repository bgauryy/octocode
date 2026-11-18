//! A kept cursor serves only its own walk, at its own position, while every
//! directory and rule it holds is unchanged.
use super::*;
use octocode_engine::types::FileSystemQueryOptions;
use octocode_test_support::json_fixture;
use std::time::Duration;

/// The cursor of a walk of `root` cut by time just after its first entry.
fn cut_after_first_entry(root: &std::path::Path) -> WalkCursor {
    let result = octocode_engine::search::fs_query::query_file_system(
        FileSystemQueryOptions {
            path: root.to_string_lossy().into_owned(),
            recursive: Some(true),
            skip_entries: Some(1),
            time_budget: Some(Duration::ZERO),
            ..Default::default()
        },
        &|_, _, _| true,
        &|| false,
    )
    .expect("walk");
    result.cursor.expect("a walk cut by time keeps its cursor")
}

/// Keep [`cut_after_first_entry`] of `root`, under policy `p`.
fn keep_cut(
    identity: &Value,
    paths: &octocode_runtime_base::policy::path::PathPolicy,
    root: &std::path::Path,
) {
    let query = serde_json::from_value::<octocode_contract_types::StructureSearchQueryFiles>(
        json!({"operation":"files","path":root}),
    )
    .expect("query");
    keep(
        identity,
        ("p", paths),
        root,
        cut_after_first_entry(root),
        (ListingDigest::new(&query, root), Proof::default()),
    );
}

fn walk() -> (
    tempfile::TempDir,
    octocode_runtime_base::policy::path::PathPolicy,
) {
    let root = tempfile::tempdir().expect("root");
    for file in ["a/1.txt", "a/2.txt", "b/3.txt"] {
        let path = root.path().join(file);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(path, "x").expect("file");
    }
    let paths = octocode_runtime_base::test_support::workspace_policy(root.path());
    (root, paths)
}

#[test]
fn a_kept_cursor_serves_its_walk_at_its_position_until_a_held_directory_changes() {
    let (root, paths) = walk();
    let identity = json_fixture(r#"["walk"]"#);
    keep_cut(&identity, &paths, root.path());
    assert!(take(&identity, "other policy", 1).is_none());
    assert!(take(&json_fixture(r#"["another walk"]"#), "p", 1).is_none());
    assert!(take(&identity, "p", 2).is_none(), "another position");
    assert_eq!(
        take(&identity, "p", 1).map(|kept| kept.cursor.position()),
        Some(1)
    );
    // An entry created in a directory the cursor holds moves its time on.
    std::thread::sleep(Duration::from_millis(30));
    std::fs::write(root.path().join("a").join("0.txt"), "y").expect("newcomer");
    assert!(take(&identity, "p", 1).is_none(), "the listing is stale");
    assert!(take(&identity, "p", 1).is_none(), "and dropped");
}

#[test]
fn an_edited_ignore_rule_of_the_directory_the_walk_is_inside_drops_the_cursor() {
    let (root, paths) = walk();
    let rules = root.path().join("a").join(".gitignore");
    std::fs::write(&rules, "2.txt\n").expect("rules");
    let identity = json_fixture(r#"["rules"]"#);
    keep_cut(&identity, &paths, root.path());
    assert!(take(&identity, "p", 1).is_some());
    std::thread::sleep(Duration::from_millis(30));
    std::fs::OpenOptions::new()
        .append(true)
        .open(&rules)
        .and_then(|mut rules| std::io::Write::write_all(&mut rules, b"3.txt\n"))
        .expect("edit in place");
    assert!(take(&identity, "p", 1).is_none());
}
