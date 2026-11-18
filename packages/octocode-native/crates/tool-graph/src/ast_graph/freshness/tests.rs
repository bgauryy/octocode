use super::*;

#[test]
fn config_inputs_cover_scanned_dirs_and_repository_ancestors() {
    let repo = octocode_runtime_base::test_support::Fixture::new();
    let base = repo.0.as_path();
    std::fs::create_dir_all(base.join(".git")).expect("git marker");
    repo.write("package.json", "{\"name\":\"repo\"}");
    repo.write("app/tsconfig.json", "{}");
    repo.write("app/src/a.ts", "export const a = 1;\n");
    repo.write("app/src/README.md", "not an input\n");
    repo.write("app/empty/tsconfig.json", "{}");
    let root = base.join("app");
    let dirs = scope_dirs(&root, ["src/a.ts"]);
    let paths = octocode_runtime_base::test_support::workspace_policy(base);
    let inputs = config_inputs(&paths, &root, &dirs);
    assert_eq!(
        inputs.keys().collect::<Vec<_>>(),
        ["../package.json", "tsconfig.json"],
        "a directory holding no scanned file is not a build input"
    );

    repo.write("app/tsconfig.json", "{\"compilerOptions\":{}}");
    repo.write("app/src/go.mod", "module x\n");
    std::fs::remove_file(base.join("package.json")).expect("remove");
    let now = config_inputs(&paths, &root, &dirs);
    assert_eq!(
        config_changes(&inputs, &now),
        [
            ("../package.json".to_owned(), "missing"),
            ("src/go.mod".to_owned(), "added"),
            ("tsconfig.json".to_owned(), "changed"),
        ]
    );
}

/// A Jujutsu checkout bounds the ancestor inputs like a git one: the
/// walk applies `.gitignore` files up to either root.
#[test]
fn every_repository_marker_bounds_the_ancestor_dirs() {
    for marker in octocode_engine::search::repository::MARKERS {
        let repo = octocode_runtime_base::test_support::Fixture::new();
        let base = repo.0.as_path();
        std::fs::create_dir_all(base.join(marker)).expect("repository marker");
        repo.write("packages/app/src/a.ts", "export const a = 1;\n");
        let root = base.join("packages/app");
        let dirs = scope_dirs(&root, ["src/a.ts"]);
        assert!(dirs.contains(base), "{marker}: {dirs:?}");
        assert!(dirs.contains(&base.join("packages")), "{marker}: {dirs:?}");
    }
}

#[test]
fn config_inputs_outside_the_path_policy_are_not_read() {
    let repo = octocode_runtime_base::test_support::Fixture::new();
    let base = repo.0.as_path();
    std::fs::create_dir_all(base.join(".git")).expect("git marker");
    repo.write("package.json", "{}");
    repo.write("app/src/a.ts", "export const a = 1;\n");
    let root = base.join("app");
    let paths = octocode_runtime_base::test_support::workspace_policy(&root);
    let inputs = config_inputs(&paths, &root, &scope_dirs(&root, ["src/a.ts"]));
    assert!(inputs.is_empty(), "{inputs:?}");
}
