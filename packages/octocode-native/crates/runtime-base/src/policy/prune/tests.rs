#[test]
fn credential_stores_left_to_the_policy_are_all_withheld_by_it() {
    for name in super::CREDENTIAL_DIRECTORY_NAMES {
        assert!(
            crate::policy::discovery::SENSITIVE_DIRECTORY_NAMES.contains(name),
            "{name} would be walked"
        );
        assert!(!super::walk_names(true, &[]).iter().any(|kept| kept == name));
    }
}

use super::*;

#[test]
fn the_prune_list_keeps_tool_config_and_source_like_names_walkable() {
    let names = defaults().collect::<Vec<_>>();
    for name in [".git", ".ssh", "node_modules", "target"] {
        assert!(names.contains(&name), "{name}");
    }
    // Tool config is real text and syntax: `includeHidden` alone reaches it.
    for name in [".github", ".vscode", ".config", ".cargo", ".yarn", ".idea"] {
        assert!(
            !names.contains(&name),
            "{name} is walked with includeHidden"
        );
    }
    for name in [
        "vendor", "output", "cache", "release", "tmp", "secrets", "private",
    ] {
        assert!(!names.contains(&name), "{name} is routinely real source");
    }
}

#[test]
fn an_include_naming_a_pruned_directory_lifts_only_that_prune() {
    let include = [
        "build/*.rs".to_owned(),
        "src/**".to_owned(),
        "*.ts".to_owned(),
        ".ssh/*".to_owned(),
    ];
    let names = walk_names(true, &include);
    assert!(!names.iter().any(|name| name == "build"), "{names:?}");
    assert!(names.iter().any(|name| name == "dist"), "{names:?}");
    // Credential stores are the read policy's, included or not.
    assert!(!names.iter().any(|name| name == ".ssh"), "{names:?}");
    // An explicit directory glob lifts only the directory it names.
    let bare = ["dist/**".to_owned()];
    let names = walk_names(true, &bare);
    assert!(!names.iter().any(|name| name == "dist"), "{names:?}");
    assert!(names.iter().any(|name| name == "build"), "{names:?}");
    assert_eq!(walk_names(false, &bare), Vec::<String>::new());
}

#[test]
fn only_the_graph_store_under_octocode_is_an_artifact_unless_the_root_is_in_it() {
    use std::path::Path;
    let root = Path::new("/w");
    assert!(is_octocode_artifact(
        root,
        Path::new("/w/.octocode/graph"),
        true
    ));
    assert!(is_octocode_artifact(
        root,
        Path::new("/w/sub/.octocode/graph"),
        true
    ));
    // Skill outputs and a file named graph stay walkable.
    for (path, is_dir) in [
        ("/w/.octocode/octocode-dev", true),
        ("/w/.octocode/graph-view", true),
        ("/w/.octocode/graph", false),
        ("/w/graph", true),
    ] {
        assert!(
            !is_octocode_artifact(root, Path::new(path), is_dir),
            "{path}"
        );
    }
    // A root at or inside the store searches it.
    for inside in ["/w/.octocode/graph", "/w/.octocode/graph/snapshots"] {
        assert!(!is_octocode_artifact(
            Path::new(inside),
            Path::new("/w/.octocode/graph"),
            true
        ));
    }
}

#[test]
fn default_excludes_false_prunes_nothing() {
    assert!(directories(false).is_empty());
    assert_eq!(directories(true).len(), defaults().count());
}
