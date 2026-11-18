use super::*;
use crate::lsp::config::workspace_root::{representative_source_for, workspace_root_languages};

/// [`representative_source_for`] in the first language of
/// [`workspace_root_languages`].
fn workspace_root_representative_source(workspace_root: &str) -> Option<String> {
    let extension = workspace_root_languages(workspace_root)
        .into_iter()
        .next()?;
    representative_source_for(workspace_root, extension)
}

/// Every built-in file route must also be discoverable from its containing
/// directory, including type stubs and build-definition dialects.
#[test]
fn every_routed_extension_can_root_a_directory_query() {
    for extension in crate::lsp::config::ROUTED_EXTENSIONS.iter() {
        let directory = tempfile::tempdir().expect("directory");
        let file = directory.path().join(format!("source{extension}"));
        std::fs::write(&file, "").expect("source");
        let root = directory.path().to_string_lossy();
        assert_eq!(
            directory_languages(&root, &root).len(),
            1,
            "{extension}: a routed source must identify its server family"
        );
        assert_eq!(
            directory_representative(&root, &root).as_deref(),
            Some(file.to_string_lossy().as_ref()),
            "{extension}: the directory query must open its source"
        );
    }
}

#[test]
fn declaration_modules_root_directory_queries_with_implementation_preference() {
    for name in ["api.d.ts", "api.d.mts", "api.d.cts"] {
        let directory = tempfile::tempdir().expect("directory");
        let declaration = directory.path().join("src").join(name);
        std::fs::create_dir_all(declaration.parent().expect("parent")).expect("source directory");
        std::fs::write(&declaration, "export declare const target: number;").expect("declaration");
        let root = directory.path().to_string_lossy();
        assert_eq!(directory_languages(&root, &root), vec![".ts"], "{name}");
        assert_eq!(
            directory_representative(&root, &root).as_deref(),
            Some(declaration.to_string_lossy().as_ref()),
            "{name}: declarations are usable when no implementation exists"
        );
        let extension = declaration.extension().expect("extension");
        let implementation = directory
            .path()
            .join("lib/implementation")
            .with_extension(extension);
        std::fs::create_dir_all(implementation.parent().expect("parent")).expect("lib directory");
        std::fs::write(&implementation, "export const target = 1;").expect("implementation");
        assert_eq!(
            directory_representative(&root, &root).as_deref(),
            Some(implementation.to_string_lossy().as_ref()),
            "{name}: implementation outside the preferred source directory still wins"
        );
    }
}

#[test]
fn deeply_nested_sources_can_root_directory_queries() {
    let directory = tempfile::tempdir().expect("directory");
    let mut nested = directory.path().to_path_buf();
    for level in 0..32 {
        nested.push(format!("level{level}"));
    }
    std::fs::create_dir_all(&nested).expect("nested directory");
    let source = nested.join("source.py");
    std::fs::write(&source, "def target(): pass").expect("source");
    let root = directory.path().to_string_lossy();
    assert_eq!(
        directory_representative(&root, &root).as_deref(),
        Some(source.to_string_lossy().as_ref())
    );
}

#[test]
fn workspace_root_infers_its_server_from_project_markers() {
    let dir = tempfile::tempdir().expect("base");
    let base = dir.path().to_path_buf();
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
        let config = default_server_for_directory(&root, &root, &options)
            .unwrap_or_else(|| panic!("{name}: no server inferred"));
        assert_eq!(config.language_id.as_deref(), Some(expected), "{name}");
        assert_eq!(config.workspace_root, root, "{name}");
    }
    let empty = base.join("empty");
    std::fs::create_dir_all(&empty).expect("empty root");
    assert!(
        default_server_for_directory(&empty.to_string_lossy(), &empty.to_string_lossy(), &options)
            .is_none()
    );
}

#[test]
fn workspace_root_representative_source_skips_vendored_dirs() {
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path().to_path_buf();
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
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path().to_path_buf();
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
    let dir = tempfile::tempdir().expect("root");
    let root = dir.path().to_path_buf();
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

/// A directory routes by its own markers, else by the languages of its
/// files (most files first, at the first level that holds any), else by the
/// workspace root's markers: a folder of Python scripts under a TypeScript
/// root is Python's, and a Go package with no manifest of its own is Go's.
#[test]
fn a_directory_routes_by_the_languages_of_its_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let text = |path: &std::path::Path| path.to_string_lossy().into_owned();
    let write = |relative: &str| {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(path, "").expect("file");
    };
    write("tsconfig.json");
    write("scripts/flows.py");
    write("scripts/walk.py");
    write("scripts/stub.mjs");
    write("pkg/csv/reader.go");
    write("pkg/csv/writer.go");
    write("pkg/csv/README.md");
    write("cgo/gcc_a.c");
    write("cgo/gcc_b.c");
    write("cgo/abi.h");
    write("cgo/a.go");
    write("tree/x/deep.rs");
    write("docs/guide.md");
    let options = LspDiscoveryOptions::default();
    let workspace = text(root);
    let route = |relative: &str| {
        let directory = text(&root.join(relative));
        (
            directory_languages(&directory, &workspace),
            default_server_for_directory(&directory, &workspace, &options)
                .and_then(|config| config.language_id),
        )
    };
    assert_eq!(
        route("scripts"),
        (vec![".py", ".ts"], Some("python".to_owned()))
    );
    assert_eq!(route("pkg/csv"), (vec![".go"], Some("go".to_owned())));
    assert_eq!(route("cgo").0, vec![".cpp", ".go"]);
    // A directory with sources only below it routes by the first level
    // that holds any.
    assert_eq!(route("tree"), (vec![".rs"], Some("rust".to_owned())));
    // No source file: the workspace root's markers.
    assert_eq!(route("docs").0, vec![".ts"]);
    // Its own markers first: the directory is a project root.
    write("scripts/package.json");
    assert_eq!(route("scripts").0, vec![".ts"]);
    // Nothing anywhere: no route.
    let empty = tempfile::tempdir().expect("empty");
    assert!(directory_languages(&text(empty.path()), &text(empty.path())).is_empty());
    assert!(
        default_server_for_directory(&text(empty.path()), &text(empty.path()), &options).is_none()
    );
}

/// A C-family directory opens a translation unit, not a header: clangd
/// parses a header opened alone without its includer's context.
#[test]
fn a_c_family_representative_is_a_translation_unit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    for name in ["abi_amd64.h", "gcc_libinit.c", "libcgo.h"] {
        std::fs::write(root.join(name), "").expect("file");
    }
    let text = root.to_string_lossy().into_owned();
    let found = directory_representative(&text, &text).expect("a C source");
    assert!(found.ends_with("gcc_libinit.c"), "{found}");
    std::fs::remove_file(root.join("gcc_libinit.c")).expect("remove");
    let found = directory_representative(&text, &text).expect("a header when nothing else");
    assert!(found.ends_with("abi_amd64.h"), "{found}");
}
