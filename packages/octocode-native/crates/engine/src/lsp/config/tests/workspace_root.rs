use super::*;

#[test]
fn workspace_root_infers_its_server_from_project_markers() {
    let base = std::env::temp_dir().join(format!(
        "octocode-lsp-root-markers-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    ));
    let cargo_workspace = ("Cargo.toml", "[workspace]\nmembers = []\n");
    let cases = [
        ("rust", &[("Cargo.toml", "")][..], "rust"),
        ("ts", &[("tsconfig.json", "")][..], "typescript"),
        ("node", &[("package.json", "")][..], "typescript"),
        // A tsconfig is the strongest TypeScript signal; Cargo beats a bare
        // package.json (npm wrappers around Rust crates).
        (
            "mixed",
            &[("package.json", ""), ("Cargo.toml", "")][..],
            "rust",
        ),
        (
            "ts-over-crate",
            &[("tsconfig.json", ""), ("Cargo.toml", "")][..],
            "typescript",
        ),
        // A Cargo workspace root is Rust's project; its tsconfig serves
        // JS bindings.
        (
            "cargo-workspace",
            &[("tsconfig.json", ""), cargo_workspace][..],
            "rust",
        ),
        // Both declare a workspace: marker order decides.
        (
            "both-workspaces",
            &[
                ("tsconfig.json", ""),
                ("package.json", "{\"workspaces\": []}"),
                cargo_workspace,
            ][..],
            "typescript",
        ),
    ];
    let options = LspDiscoveryOptions::default();
    for (name, markers, expected) in cases {
        let root = base.join(name);
        std::fs::create_dir_all(&root).expect("root");
        for (marker, content) in markers {
            std::fs::write(root.join(marker), content).expect("marker");
        }
        let root = root.to_string_lossy().into_owned();
        let config = default_server_for_workspace_root(&root, &options)
            .unwrap_or_else(|| panic!("{name}: no server inferred"));
        assert_eq!(config.language_id.as_deref(), Some(expected), "{name}");
        assert_eq!(config.workspace_root, root, "{name}");
    }
    let empty = base.join("empty");
    std::fs::create_dir_all(&empty).expect("empty root");
    assert!(default_server_for_workspace_root(&empty.to_string_lossy(), &options).is_none());
}

#[test]
fn workspace_root_representative_source_skips_vendored_dirs() {
    let root = std::env::temp_dir().join(format!(
        "octocode-lsp-root-representative-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(root.join("node_modules/dep")).expect("node_modules");
    std::fs::create_dir_all(root.join("src/nested")).expect("src");
    std::fs::write(root.join("tsconfig.json"), "{}").expect("tsconfig");
    std::fs::write(root.join("node_modules/dep/index.ts"), "").expect("vendored");
    std::fs::write(root.join("README.md"), "").expect("readme");
    std::fs::write(root.join("src/nested/app.ts"), "").expect("source");
    let found = workspace_root_representative_source(&root.to_string_lossy())
        .expect("a representative TypeScript source");
    assert!(found.ends_with("app.ts"), "{found}");

    let empty = root.join("src/nested");
    std::fs::remove_file(empty.join("app.ts")).expect("remove");
    assert!(workspace_root_representative_source(&root.to_string_lossy()).is_none());
}

#[test]
fn workspace_root_representative_source_prefers_project_include_roots() {
    let root = std::env::temp_dir().join(format!(
        "octocode-lsp-root-include-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(root.join("lib/core")).expect("lib");
    std::fs::create_dir_all(root.join("src")).expect("src");
    // A root-level build script sorts first but is outside the project.
    std::fs::write(root.join("build.mjs"), "").expect("build script");
    std::fs::write(root.join("lib/core/index.ts"), "").expect("included source");
    std::fs::write(root.join("src/other.ts"), "").expect("src source");
    std::fs::write(
        root.join("tsconfig.json"),
        r#"{"include": ["lib/**/*.ts"]}"#,
    )
    .expect("tsconfig");
    let found = workspace_root_representative_source(&root.to_string_lossy())
        .expect("an included TypeScript source");
    assert!(found.ends_with("lib/core/index.ts"), "{found}");

    // Without a parseable include list, `src/` still beats root scripts.
    std::fs::write(root.join("tsconfig.json"), "{ // jsonc\n}").expect("jsonc");
    let found = workspace_root_representative_source(&root.to_string_lossy())
        .expect("a src TypeScript source");
    assert!(found.ends_with("src/other.ts"), "{found}");
}

/// A literal include root (`packages` of `packages/*/src/**/*`) also holds
/// files the project does not compile: the representative matches the
/// whole include glob and no exclude, so the server loads the project.
#[test]
fn workspace_root_representative_source_matches_the_full_include_glob() {
    let root = std::env::temp_dir().join(format!(
        "octocode-lsp-root-glob-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(root.join("packages/agent/src/gen")).expect("agent src");
    std::fs::write(root.join("packages/agent/vitest.config.ts"), "").expect("config");
    std::fs::write(root.join("packages/agent/src/gen/a.ts"), "").expect("excluded");
    std::fs::write(root.join("packages/agent/src/loop.ts"), "").expect("source");
    std::fs::write(
        root.join("tsconfig.json"),
        r#"{"include": ["packages/*/src/**/*"], "exclude": ["**/gen/**"]}"#,
    )
    .expect("tsconfig");
    let found = workspace_root_representative_source(&root.to_string_lossy())
        .expect("an included TypeScript source");
    assert!(found.ends_with("packages/agent/src/loop.ts"), "{found}");

    // No compiled source at all: any source still roots the server.
    std::fs::remove_file(root.join("packages/agent/src/loop.ts")).expect("remove");
    std::fs::remove_file(root.join("packages/agent/src/gen/a.ts")).expect("remove");
    let found = workspace_root_representative_source(&root.to_string_lossy())
        .expect("a fallback TypeScript source");
    assert!(found.ends_with("vitest.config.ts"), "{found}");
}
