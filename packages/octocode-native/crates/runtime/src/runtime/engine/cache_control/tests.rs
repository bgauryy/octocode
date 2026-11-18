use super::*;

#[test]
fn clearing_removes_every_store_and_checkpoint_but_keeps_user_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let workspace = dir.path().join("work");
    let cache_dirs: Vec<_> = octocode_runtime_base::cache::Directory::ALL
        .into_iter()
        .map(|kind| kind.path(&home))
        .chain(octocode_runtime_base::cache::Directory::retired(&home))
        .chain([
            octocode_tool_github::gh_structure::materialize::materialize_root(&home),
            octocode_tools_shared::source_capture::url::capture_root(&home),
            workspace.join(".octocode/graph/.facts"),
        ])
        .collect();
    for path in &cache_dirs {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("evidence"), "cache").unwrap();
    }
    let kept = [
        home.join("clones/o/r/local-edit"),
        home.join("tmp/clone/o/r/local-edit"),
        home.join("credentials.json"),
        home.join("tmp/ratelimit/budget"),
        workspace.join(".octocode/graph/published/graph.bin"),
    ];
    for path in &kept {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "user").unwrap();
    }
    let removed = clear_disk(&home, &workspace, &workspace.join(".octocode/graph")).unwrap();
    assert_eq!(removed.len(), cache_dirs.len());
    for path in cache_dirs {
        assert!(!path.exists(), "{}", path.display());
    }
    for path in kept {
        assert_eq!(std::fs::read_to_string(path).unwrap(), "user");
    }
    assert!(
        clear_disk(&home, &workspace, &workspace.join(".octocode/graph"))
            .unwrap()
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn clearing_refuses_a_symlinked_cache_parent_without_touching_its_target() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(outside.join("response")).unwrap();
    let file = outside.join("response/user");
    std::fs::write(&file, "keep").unwrap();
    std::os::unix::fs::symlink(&outside, home.join("tmp")).unwrap();
    assert!(clear_disk(&home, dir.path(), &dir.path().join(".octocode/graph")).is_err());
    assert_eq!(std::fs::read_to_string(file).unwrap(), "keep");
}
