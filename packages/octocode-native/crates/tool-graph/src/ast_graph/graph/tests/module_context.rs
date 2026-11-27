use super::*;
use octocode_runtime_base::test_support::Fixture;
use std::collections::BTreeSet;

#[test]
fn shared_source_with_different_child_directories_stays_ambiguous() {
    let root = Fixture::new();
    for (path, source) in [
        (
            "entry.rs",
            "mod shared;\n#[path = \"shared.rs\"] mod relocated;\n",
        ),
        ("shared.rs", "mod child;\nuse self::child::leaf;\n"),
        ("shared/child.rs", "pub fn leaf() {}\n"),
        ("child.rs", "pub fn leaf() {}\n"),
    ] {
        root.write(path, source);
    }
    let graph = fixture_graph(&root.0, &[]);
    assert!(
        graph.facts["shared.rs"]
            .imports
            .iter()
            .all(|import| import.target.is_none()),
        "{:?}",
        graph.facts["shared.rs"].imports
    );
}

#[test]
fn relocated_module_ancestry_resolves_parents_roots_and_siblings() {
    let root = Fixture::new();
    for (path, source) in [
        (
            "entry.rs",
            "#[path = \"elsewhere/parent.rs\"] mod parent;\nfn root_fn() {}\n",
        ),
        (
            "elsewhere/parent.rs",
            "mod child;\n#[path = \"sibling.rs\"] mod sibling;\nfn parent_fn() {}\n",
        ),
        (
            "elsewhere/child.rs",
            "use super::parent_fn;\nuse super::super::root_fn;\nuse crate::root_fn as from_root;\nuse super::sibling::sibling_fn;\n",
        ),
        ("elsewhere/sibling.rs", "pub fn sibling_fn() {}\n"),
    ] {
        root.write(path, source);
    }
    let graph = fixture_graph(&root.0, &[]);
    let imports = &graph.facts["elsewhere/child.rs"].imports;
    for (name, target) in [
        ("parent_fn", "elsewhere/parent.rs"),
        ("root_fn", "entry.rs"),
        ("sibling_fn", "elsewhere/sibling.rs"),
    ] {
        assert!(
            imports
                .iter()
                .any(|i| i.imported_name == name && i.target.as_deref() == Some(target)),
            "{name}: {imports:?}"
        );
    }
    assert!(
        graph
            .diagnostics
            .iter()
            .all(|d| d.code != DiagnosticCode::UnresolvedInternal),
        "{:?}",
        graph.diagnostics
    );
}

/// `#[path]` on an inline module names the directory its file modules
/// live in, from the declaring file's directory (a non-`mod.rs` file's
/// stem is not added), as rustc mounts them; nested inline modules below
/// it add their names.
#[test]
fn path_on_an_inline_module_roots_its_nested_file_modules() {
    let root = Fixture::new();
    for (path, source) in [
        (
            "lib.rs",
            "#[path = \"extra\"]\nmod inl {\n    pub mod deep;\n    mod plain {\n        pub mod leaf;\n    }\n}\nmod other;\n",
        ),
        ("extra/deep.rs", "pub fn deep_fn() {}\n"),
        ("extra/plain/leaf.rs", "pub fn leaf_fn() {}\n"),
        (
            "other.rs",
            "#[path = \"gen\"]\nmod gen_mod {\n    pub mod made;\n}\n",
        ),
        ("gen/made.rs", "pub fn made_fn() {}\n"),
    ] {
        root.write(path, source);
    }
    let graph = fixture_graph(&root.0, &[]);
    let module_targets = |file: &str| {
        graph.facts[file]
            .imports
            .iter()
            .filter(|import| import.id.starts_with("module:"))
            .filter_map(|import| import.target.clone())
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        module_targets("lib.rs"),
        BTreeSet::from([
            "extra/deep.rs".to_owned(),
            "extra/plain/leaf.rs".to_owned(),
            "other.rs".to_owned(),
        ]),
        "{:?}",
        graph.facts["lib.rs"].imports
    );
    assert_eq!(
        module_targets("other.rs"),
        BTreeSet::from(["gen/made.rs".to_owned()]),
        "{:?}",
        graph.facts["other.rs"].imports
    );
}

#[test]
fn shared_source_with_different_module_parents_stays_ambiguous() {
    let root = Fixture::new();
    for (path, source) in [
        (
            "left.rs",
            "#[path = \"shared.rs\"] mod shared;\nfn owner() {}\n",
        ),
        (
            "right.rs",
            "#[path = \"shared.rs\"] mod shared;\nfn owner() {}\n",
        ),
        ("shared.rs", "use super::owner;\n"),
    ] {
        root.write(path, source);
    }
    let graph = fixture_graph(&root.0, &[]);
    assert!(
        graph.facts["shared.rs"]
            .imports
            .iter()
            .all(|i| i.target.is_none())
    );
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|d| d.file == "shared.rs" && d.code == DiagnosticCode::UnresolvedInternal)
    );
}
