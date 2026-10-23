//! Cargo workspaces: dependency aliases, manifest refresh and cross-crate
//! Rust paths.
use super::*;

fn cargo_fixture() -> Fixture {
    let root = Fixture::new();
    std::fs::write(
        root.0.as_path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"a\", \"b\", \"z_one\", \"z_two\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    for (name, dependency) in [
        ("a", "shared = { package = \"z_one\", path = \"../z_one\" }"),
        ("b", "shared = { package = \"z_two\", path = \"../z_two\" }"),
        ("z_one", ""),
        ("z_two", ""),
    ] {
        let directory = root.0.as_path().join(name);
        std::fs::create_dir_all(directory.join("src")).unwrap();
        std::fs::write(directory.join("src/lib.rs"), "pub fn item() {}\n").unwrap();
        std::fs::write(directory.join("Cargo.toml"), format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[dependencies]\n{dependency}\n"
        )).unwrap();
    }
    root
}

#[test]
fn cargo_aliases_are_resolved_in_the_importing_package() {
    let fixture = cargo_fixture();
    let crates = super::super::load_cargo_crates(
        &fixture.0.as_path().canonicalize().unwrap(),
        None,
        &crate::execution::cancel::NeverCancel,
    )
    .unwrap();
    let known = [
        "a/src/lib.rs",
        "b/src/lib.rs",
        "z_one/src/lib.rs",
        "z_two/src/lib.rs",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for (importer, expected) in [
        ("a/src/lib.rs", "z_one/src/lib.rs"),
        ("b/src/lib.rs", "z_two/src/lib.rs"),
    ] {
        assert_eq!(
            super::super::resolve_rust("shared::item", importer, &known, &crates),
            Some(expected.to_owned())
        );
    }
    assert_eq!(
        super::super::resolve_rust("shared::item", "z_one/src/lib.rs", &known, &crates),
        None
    );
    assert_eq!(
        super::super::resolve_rust("z_two::item", "a/src/lib.rs", &known, &crates),
        None
    );
}

#[test]
fn cargo_member_manifest_changes_refresh_dependency_aliases() {
    let fixture = cargo_fixture();
    let manifest = fixture.0.as_path().join("z_two/Cargo.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(
        &manifest,
        format!("{text}shared = {{ package = \"z_one\", path = \"../z_one\" }}\n"),
    )
    .unwrap();
    // Warm up after any first-run lockfile creation, so only the member
    // manifest changes between the two observations below.
    super::super::load_cargo_crates(
        &fixture.0.as_path().canonicalize().unwrap(),
        None,
        &crate::execution::cancel::NeverCancel,
    )
    .unwrap();
    let before = super::super::load_cargo_crates(
        &fixture.0.as_path().canonicalize().unwrap(),
        None,
        &crate::execution::cancel::NeverCancel,
    )
    .unwrap();
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, text.replace("shared =", "renamed =")).unwrap();
    let after = super::super::load_cargo_crates(
        &fixture.0.as_path().canonicalize().unwrap(),
        None,
        &crate::execution::cancel::NeverCancel,
    )
    .unwrap();
    let known = ["z_two/src/lib.rs", "z_one/src/lib.rs"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(
        super::super::resolve_rust("shared::item", "z_two/src/lib.rs", &known, &before),
        Some("z_one/src/lib.rs".to_owned())
    );
    assert_eq!(
        super::super::resolve_rust("renamed::item", "z_two/src/lib.rs", &known, &before),
        None
    );
    assert_eq!(
        super::super::resolve_rust("renamed::item", "z_two/src/lib.rs", &known, &after),
        Some("z_one/src/lib.rs".to_owned())
    );
}

#[test]
fn rust_internal_paths_resolve_across_workspace_crates() {
    let known = [
        "crates/runtime/src/lib.rs",
        "crates/runtime/src/config/mod.rs",
        "crates/runtime/src/config/types.rs",
        "crates/runtime/src/tools/mod.rs",
        "crates/runtime/src/tools/local_fetch.rs",
        "crates/engine/src/lib.rs",
        "crates/engine/src/lsp.rs",
    ]
    .iter()
    .map(|path| (*path).to_owned())
    .collect::<std::collections::BTreeSet<_>>();
    let crates = super::super::CargoCrates::default();
    // `crate::` anchors to the importer's own crate root, not a global
    // `src/` — the multi-crate workspace case.
    assert_eq!(
        super::super::resolve_rust(
            "crate::config::types::ResolvedConfig;",
            "crates/runtime/src/tools/local_fetch.rs",
            &known,
            &crates
        ),
        Some("crates/runtime/src/config/types.rs".to_owned())
    );
    // Trailing item segments fall back to the longest module prefix.
    assert_eq!(
        super::super::resolve_rust(
            "crate::config::PROTECTED_KEYS;",
            "crates/runtime/src/tools/local_fetch.rs",
            &known,
            &crates
        ),
        Some("crates/runtime/src/config/mod.rs".to_owned())
    );
    // `super::` climbs one module level per segment; an item-only tail
    // resolves to the parent module file.
    assert_eq!(
        super::super::resolve_rust(
            "super::PROTECTED_KEYS;",
            "crates/runtime/src/config/types.rs",
            &known,
            &crates
        ),
        Some("crates/runtime/src/config/mod.rs".to_owned())
    );
    assert_eq!(
        super::super::resolve_rust(
            "super::super::tools::local_fetch::Row;",
            "crates/runtime/src/config/types.rs",
            &known,
            &crates
        ),
        Some("crates/runtime/src/tools/local_fetch.rs".to_owned())
    );
    // `use super::*;` denotes the parent module file itself.
    assert_eq!(
        super::super::resolve_rust(
            "super::*;",
            "crates/runtime/src/config/types.rs",
            &known,
            &crates
        ),
        Some("crates/runtime/src/config/mod.rs".to_owned())
    );
    // Grouped imports resolve to the deepest real module prefix.
    assert_eq!(
        super::super::resolve_rust(
            "crate::config::{types, validation};",
            "crates/runtime/src/tools/local_fetch.rs",
            &known,
            &crates
        ),
        Some("crates/runtime/src/config/mod.rs".to_owned())
    );
    // Importers in a different crate anchor to their own root.
    assert_eq!(
        super::super::resolve_rust(
            "crate::lsp::Pool;",
            "crates/engine/src/lsp.rs",
            &known,
            &crates
        ),
        Some("crates/engine/src/lsp.rs".to_owned())
    );
}

#[test]
fn rust_uniform_path_fallback_only_links_the_importers_child_modules() {
    let known = ["src/a.rs", "src/thing.rs", "src/b.rs", "src/b/child.rs"]
        .iter()
        .map(|path| (*path).to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    let crates = super::super::CargoCrates::default();
    // A same-named file elsewhere in the tree must not become an edge for a
    // path that is not one of the importer's own child modules.
    assert_eq!(
        super::super::resolve_rust("unrelated::thing;", "src/a.rs", &known, &crates),
        None
    );
    // A module the importer declares (`mod child;`) lives in its child
    // directory and resolves.
    assert_eq!(
        super::super::resolve_rust("child::item;", "src/b.rs", &known, &crates),
        Some("src/b/child.rs".to_owned())
    );
    // Root-style importers (lib.rs/main.rs/mod.rs) keep sibling resolution.
    assert_eq!(
        super::super::resolve_rust("thing::item;", "src/lib.rs", &known, &crates),
        Some("src/thing.rs".to_owned())
    );
}

#[test]
fn cargo_metadata_stdout_is_drained_while_the_child_runs() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let crates =
        super::super::load_cargo_crates(root, None, &crate::execution::cancel::NeverCancel)
            .expect("cargo metadata");
    assert_eq!(
        crates
            .resolve("src/lib.rs", "octocode_native")
            .map(String::as_str),
        Some("src/lib.rs")
    );
}
