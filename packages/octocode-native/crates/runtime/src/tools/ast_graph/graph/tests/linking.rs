//! The per-file link pass: ignore rules, heritage and call kinds, fact
//! schemas and diagnostics.
use super::*;

/// A graph scan walks under the one ignore policy (it has no
/// `includeIgnored`): `.gitignore`, nested negations and
/// `.git/info/exclude` leave files out of every scan.
#[test]
fn ignore_rules_always_apply() {
    let dir = Fixture::new();
    let root = dir.0.as_path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join(".git/info")).unwrap();
    dir.write(".git/info/exclude", "local-only.ts\n");
    dir.write(".gitignore", "generated/\n*.gen.ts\n");
    dir.write("pkg/.gitignore", "scratch/\n!keep.gen.ts\n");
    for file in [
        "src/main.ts",
        "generated/api.ts",
        "src/model.gen.ts",
        "pkg/keep.gen.ts",
        "pkg/scratch/tmp.ts",
        "local-only.ts",
    ] {
        dir.write(file, "export const x = 1;\n");
    }
    let filtered = fixture_graph(&root, &["pkg"]);
    assert_eq!(filtered.facts.keys().collect::<Vec<_>>(), ["src/main.ts"],);
    let nested = fixture_graph(&root, &[]);
    assert_eq!(
        nested.facts.keys().collect::<Vec<_>>(),
        ["pkg/keep.gen.ts", "src/main.ts"],
        "nested negation re-includes; nested dir rule prunes"
    );
}

#[test]
fn link_file_keeps_heritage_and_call_kinds() {
    use octocode_engine::graph::{GraphFactCall, GraphFactEdge, GraphFactsDocument};
    let document = GraphFactsDocument {
        schema_version: octocode_engine::signatures::GRAPH_FACTS_SCHEMA_VERSION,
        language: "typescript".into(),
        edges: vec![
            GraphFactEdge {
                from: "decl:A".into(),
                to: "ns.Base".into(),
                relation: "extends".into(),
                line: 3,
                ..Default::default()
            },
            GraphFactEdge {
                from: "decl:A".into(),
                to: "Shape".into(),
                relation: "implements".into(),
                line: 3,
                ..Default::default()
            },
            GraphFactEdge {
                from: "decl:A".into(),
                to: "x".into(),
                relation: "contains".into(),
                line: 4,
                ..Default::default()
            },
        ],
        calls: vec![
            GraphFactCall {
                callee: "Widget".into(),
                kind: "renders".into(),
                line: 5,
                ..Default::default()
            },
            GraphFactCall {
                callee: "Injectable".into(),
                kind: "decorates".into(),
                caller_id: Some("decl:A".into()),
                line: 2,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let mut built = super::super::BuiltGraph::default();
    let known = known(&["a.ts"]);
    let security = crate::security::ContentSecurity::new();
    let inputs = super::super::LinkInputs {
        known: &known,
        security: &security,
        rust_cargo_unavailable: false,
        cargo_crates: &super::super::CargoCrates::default(),
        ctx: &super::super::ResolveContext::default(),
        packages: &Default::default(),
    };
    super::super::link_file(
        &mut built,
        &inputs,
        "a.ts".into(),
        document,
        Default::default(),
    );
    let facts = &built.facts["a.ts"];
    assert_eq!(
        facts
            .heritage
            .iter()
            .map(|h| (
                h.decl_id.as_str(),
                h.relation.as_str(),
                h.target.as_str(),
                h.line
            ))
            .collect::<Vec<_>>(),
        [
            ("decl:A", "extends", "ns.Base", 3),
            ("decl:A", "implements", "Shape", 3)
        ]
    );
    assert_eq!(
        facts
            .calls
            .iter()
            .map(|c| (c.callee.as_str(), c.kind.as_str(), c.caller_id.is_some()))
            .collect::<Vec<_>>(),
        [
            ("Widget", "renders", false),
            ("Injectable", "decorates", true)
        ]
    );
}

/// Facts of another schema version (v1 carried text diagnostics) are
/// never linked: the file is skipped and says why.
#[test]
fn facts_of_an_older_schema_are_skipped_not_linked() {
    let mut built = super::super::BuiltGraph::default();
    let known = known(&["a.ts"]);
    let security = crate::security::ContentSecurity::new();
    let inputs = super::super::LinkInputs {
        known: &known,
        security: &security,
        rust_cargo_unavailable: false,
        cargo_crates: &super::super::CargoCrates::default(),
        ctx: &super::super::ResolveContext::default(),
        packages: &Default::default(),
    };
    let entry = octocode_engine::graph::GraphFactsTypedEntry {
        relative_path: "a.ts".into(),
        content_digest: "digest".into(),
        source_stamp: None,
        facts: octocode_engine::graph::GraphFactsDocument {
            schema_version: octocode_engine::signatures::GRAPH_FACTS_SCHEMA_VERSION - 1,
            ..Default::default()
        },
        reference_counts: Vec::new(),
    };
    super::super::link_entry(&mut built, &inputs, entry, Default::default());
    assert!(built.nodes.is_empty());
    assert_eq!(built.files_skipped, 1);
    assert_eq!(
        built.diagnostics[0].code,
        DiagnosticCode::FactsSchemaUnsupported
    );
}

/// A Rust module-attribute note keeps its line in the diagnostic's
/// `line`, and the message carries no line text.
#[test]
fn a_module_attribute_note_names_its_line() {
    let dir = crate::tools::test_support::Fixture::new();
    dir.write("lib.rs", "fn a() {}\n#[custom]\nmod t;\n");
    dir.write("t.rs", "fn b() {}\n");
    let built = fixture_graph(&dir.0, &[]);
    let note = built
        .diagnostics
        .iter()
        .find(|d| d.file == "lib.rs" && d.code == DiagnosticCode::UnsupportedLinking)
        .unwrap_or_else(|| panic!("{:?}", built.diagnostics));
    assert_eq!(note.line, Some(3), "the `mod` line: {note:?}");
    assert_eq!(
        note.message,
        "unsupported Rust conditional or custom module attributes"
    );
}
