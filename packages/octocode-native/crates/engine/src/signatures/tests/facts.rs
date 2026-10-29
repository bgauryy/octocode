//! Declarations-only facts against the full graph facts, and fact metadata.

use super::*;

/// The declarations-only extraction (outlines, enclosing names) skips
/// calls, argument observations, reference counts and import uses, and keeps every other
/// fact identical to the full graph extraction, in every grammar.
/// A `language` override outlines declarations only: the same
/// declarations as the full facts under that grammar, and no call facts.
#[test]
fn signatures_with_declarations_equal_the_two_separate_extractions() {
    for (path, source) in [
        (
            "lib.rs",
            "use std::fmt;\n\npub struct Point { x: f64 }\n\nimpl Point {\n    pub fn norm(&self) -> f64 {\n        let squared = self.x * self.x;\n        squared.sqrt()\n    }\n}\n\nfn helper() -> u8 {\n    let value = 1;\n    value + 1\n}\n",
        ),
        (
            "app.py",
            "import os\n\nclass Box:\n    def size(self):\n        total = 0\n        for item in self.items:\n            total += 1\n        return total\n",
        ),
        (
            "main.ts",
            "export function run(value: number): number {\n  const doubled = value * 2;\n  return doubled + 1;\n}\nexport class Widget {\n  render() {\n    return 1;\n  }\n}\n",
        ),
        ("notes.txt", "plain text\n"),
    ] {
        let (outline, declarations) = extract_signatures_with_declarations(source, path);
        let alone = extract_signatures_inner(source, path);
        assert_eq!(outline, alone, "{path}");
        assert_eq!(
            declarations.and_then(|facts| serde_json::to_value(facts).ok()),
            extract_declarations_document(source, path)
                .and_then(|facts| serde_json::to_value(facts).ok()),
            "{path}"
        );
    }
}

#[test]
fn an_override_outlines_declarations_without_call_facts() {
    let source = "fn make() -> u8 { helper(); 1 }\nfn helper() {}\n";
    let lean = serde_json::to_value(
        extract_declarations_document_with_extension(source, "script", "rs").expect("facts"),
    )
    .expect("json");
    let full: serde_json::Value = serde_json::from_str(
        &extract_graph_facts_with_extension(source, "script", "rs").expect("facts"),
    )
    .expect("json");
    assert_eq!(lean["declarations"], full["declarations"]);
    assert!(!full["calls"].as_array().expect("calls").is_empty());
    assert!(lean["calls"].as_array().is_none_or(Vec::is_empty), "{lean}");
    assert!(
        !full["argumentSites"]
            .as_array()
            .expect("argument sites")
            .is_empty(),
        "{full}"
    );
    assert!(
        lean["argumentSites"]
            .as_array()
            .expect("argument sites")
            .is_empty(),
        "{lean}"
    );
}

#[test]
fn declaration_facts_equal_the_graph_facts_without_call_or_argument_observations() {
    let cases = [
        (
            "src/lib.rs",
            "mod m {\n    pub struct S;\n    impl Clone for S { fn clone(&self) -> Self { make(S) } }\n}\nuse std::fmt;\n/// doc\npub fn make(s: m::S) -> m::S { helper(); s }\nfn helper() {}\nmacro_rules! mk { () => {} }\n",
        ),
        (
            "app.py",
            "import os\nclass A(Base):\n    def run(self):\n        return os.path.join(self.x)\n\ndef top():\n    A().run()\n",
        ),
        (
            "main.go",
            "package main\nimport \"fmt\"\ntype T struct{}\nfunc (t T) Run() { fmt.Println(1) }\nfunc main() { T{}.Run() }\n",
        ),
        (
            "A.java",
            "class A extends B implements C { void run() { go(); } void go() {} }\n",
        ),
        (
            "x.c",
            "#include <stdio.h>\nstatic int add(int a, int b) { return a + b; }\nint main(void) { printf(\"%d\", add(1, 2)); }\n",
        ),
        // A large real source: the graph-facts module, reassembled from
        // its files.
        (
            "large.rs",
            concat!(
                include_str!("../graph_facts.rs"),
                include_str!("../graph_facts/collect.rs"),
                include_str!("../graph_facts/imports.rs"),
                include_str!("../graph_facts/positions.rs"),
                include_str!("../graph_facts/references.rs"),
                include_str!("../graph_facts/walk.rs"),
            ),
        ),
    ];
    for (path, source) in cases {
        let full = tree_sitter_graph_facts(source, path).expect(path).facts;
        let lean = tree_sitter_declarations(source, path).expect(path);
        assert!(!lean.declarations.is_empty(), "{path}");
        assert!(lean.calls.is_empty(), "{path}");
        assert!(lean.argument_sites.is_empty(), "{path}");
        assert!(!full.calls.is_empty(), "{path}");
        assert!(!full.argument_sites.is_empty(), "{path}");
        use crate::text::analysis::AnalysisOperation;
        assert!(
            full.source_templates
                .units
                .iter()
                .all(|unit| unit.operation == AnalysisOperation::GraphFacts),
            "{path}"
        );
        assert!(
            lean.source_templates
                .units
                .iter()
                .all(|unit| unit.operation == AnalysisOperation::Declarations),
            "{path}"
        );
        let json =
            |value: &crate::graph::GraphFactsDocument| serde_json::to_value(value).expect("json");
        let mut declaration_coordinates = full.clone();
        // Expression-macro bodies are read for calls only, after every item
        // body, so their units come last and a declarations read has none.
        declaration_coordinates
            .source_templates
            .units
            .retain(|unit| unit.adapter.id != "rust-expression-macro-body");
        for unit in &mut declaration_coordinates.source_templates.units {
            unit.operation = AnalysisOperation::Declarations;
        }
        let mut expected = json(&declaration_coordinates);
        expected["calls"] = serde_json::json!([]);
        expected["argumentSites"] = serde_json::json!([]);
        expected.as_object_mut().map(|facts| facts.remove("paths"));
        if let Some(edges) = expected["edges"].as_array_mut() {
            edges.retain(|edge| edge["relation"] != "calls");
        }
        // An edge id ends in its push position, which skipped call
        // edges shift; consumers read from/relation/to.
        let mut got = json(&lean);
        for facts in [&mut got, &mut expected] {
            for edge in facts["edges"].as_array_mut().into_iter().flatten() {
                edge.as_object_mut().map(|edge| edge.remove("id"));
            }
        }
        assert_eq!(got, expected, "{path}");
    }
}

#[test]
fn graph_fact_metadata_preserves_json_and_export_order_across_producers() {
    for (source, path, expected_names, producer_field) in [
        (
            "export function first() { return 1; } export const second = first();",
            "main.ts",
            vec!["first", "second"],
            "commonJs",
        ),
        (
            "pub fn first() {} pub struct Second; fn private() {}",
            "lib.rs",
            vec!["first", "Second", "private"],
            "modules",
        ),
    ] {
        let extraction = extract_graph_facts_with_metadata_inner(source, path)
            .expect("graph facts with metadata");
        let counted: Vec<&str> = extraction
            .reference_counts
            .iter()
            .map(|count| {
                count
                    .declaration_id
                    .split(['#', '@'])
                    .nth(1)
                    .unwrap_or_default()
            })
            .collect();
        assert_eq!(counted, expected_names);
        let facts_json = serde_json::to_string(&extraction.facts).expect("facts JSON");
        assert_eq!(
            facts_json,
            extract_graph_facts(source, path).expect("legacy JSON facts")
        );
        let json: serde_json::Value = serde_json::from_str(&facts_json).expect("valid facts JSON");
        assert!(json.get(producer_field).is_some());
    }
}
