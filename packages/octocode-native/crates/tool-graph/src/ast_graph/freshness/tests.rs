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

/// Only a file some linking step reads is a build input: a lockfile or a
/// build script no resolver parses changes nothing in the graph, while the
/// workspace lists pnpm and Go read are inputs like the manifests.
#[test]
fn only_configs_linking_reads_are_build_inputs() {
    let repo = octocode_runtime_base::test_support::Fixture::new();
    let base = repo.0.as_path();
    std::fs::create_dir_all(base.join(".git")).expect("git marker");
    for unread in [
        "yarn.lock",
        "package-lock.json",
        "pnpm-lock.yaml",
        "bun.lock",
        "Cargo.lock",
        "go.sum",
        "poetry.lock",
        "uv.lock",
        "Pipfile.lock",
        "pom.xml",
        "build.gradle",
        "CMakeLists.txt",
        "requirements-docs.txt",
    ] {
        repo.write(unread, "unread\n");
    }
    for read in [
        "package.json",
        "pnpm-workspace.yaml",
        "go.work",
        "Cargo.toml",
        "dev-requirements.txt",
    ] {
        repo.write(read, "read\n");
    }
    repo.write("app/src/a.rs", "pub fn a() {}\n");
    let root = base.join("app");
    let paths = octocode_runtime_base::test_support::workspace_policy(base);
    let inputs = config_inputs(&paths, &root, &scope_dirs(&root, ["src/a.rs"]));
    assert_eq!(
        inputs.keys().collect::<Vec<_>>(),
        [
            "../Cargo.toml",
            "../dev-requirements.txt",
            "../go.work",
            "../package.json",
            "../pnpm-workspace.yaml"
        ]
    );
}
