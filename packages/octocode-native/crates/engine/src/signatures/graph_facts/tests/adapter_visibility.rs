use super::adapter_repairs::typed;
use super::*;

#[test]
fn header_modifiers_determine_visibility() {
    for (path, source) in [
        (
            "a.c",
            "inline static void hidden(void) {}
void visible(void) {}",
        ),
        (
            "a.scala",
            "private class hidden
class visible",
        ),
    ] {
        if languages::find_entry(&extension_of(path, true, "")).is_none() {
            continue;
        }
        let facts = extract_graph_facts_with_metadata(source, path)
            .unwrap()
            .facts;
        for (name, expected) in [("hidden", false), ("visible", true)] {
            assert_eq!(
                facts
                    .declarations
                    .iter()
                    .find(|d| d.name == name)
                    .unwrap()
                    .exported,
                expected,
                "{path}: {name}"
            );
        }
    }
}

#[test]
fn header_visibility_ignores_order_comments_annotations_and_nested_bodies() {
    for (path, source, expected) in [
        (
            "a.c",
            "static inline void hidden1(void) {}\ninline /* static is real below */ static void hidden2(void) {}\n/* static */ inline void visible(void) {}",
            vec![("hidden1", false), ("hidden2", false), ("visible", true)],
        ),
        (
            "a.cpp",
            "template<class T> inline static void hidden() {}\nclass visible { private: void inner() {} };",
            vec![("hidden", false), ("visible", true)],
        ),
        (
            "a.java",
            "@Deprecated public class Visible { private class Hidden {} static public void visible() {} private static void hidden() {} }",
            vec![
                ("Visible", true),
                ("Hidden", false),
                ("visible", true),
                ("hidden", false),
            ],
        ),
        (
            "a.cs",
            "[Mark] public class Visible { private class Hidden {} static public void visible() {} internal static void hidden() {} }",
            vec![
                ("Visible", true),
                ("Hidden", false),
                ("visible", true),
                ("hidden", false),
            ],
        ),
        (
            "a.scala",
            "private class Hidden\nprotected class Guarded\nprivate[pkg] class Scoped\nclass Visible { private def inner() = () }",
            vec![
                ("Hidden", false),
                ("Guarded", false),
                ("Scoped", false),
                ("Visible", true),
            ],
        ),
    ] {
        if languages::find_entry(&extension_of(path, true, "")).is_none() {
            continue;
        }
        let facts = typed(source, path);
        assert!(
            facts.error_lines.is_empty(),
            "{path}: {:?}",
            facts.error_lines
        );
        for (name, exported) in expected {
            let declaration = facts
                .declarations
                .iter()
                .find(|d| d.name == name)
                .unwrap_or_else(|| panic!("{path} {name}: {:?}", facts.declarations));
            assert_eq!(declaration.exported, exported, "{path}: {name}");
            assert_eq!(
                facts.exports.iter().any(|e| e.name == name),
                exported,
                "{path}: {name} export facts"
            );
        }
        let mut full = facts;
        let lean = crate::signatures::extract_declarations_document(source, path).unwrap();
        full.calls.clear();
        full.argument_sites.clear();
        full.edges.retain(|edge| edge.relation != "calls");
        use crate::text::analysis::AnalysisOperation;
        assert!(
            lean.source_templates
                .units
                .iter()
                .all(|unit| unit.operation == AnalysisOperation::Declarations)
        );
        for unit in &mut full.source_templates.units {
            assert_eq!(unit.operation, AnalysisOperation::GraphFacts);
            unit.operation = AnalysisOperation::Declarations;
        }
        assert_eq!(
            full, lean,
            "{path}: declaration-only facts retain visibility"
        );
    }
}

#[cfg(feature = "tree-sitter-scala")]
#[test]
fn scala_forwarding_is_an_explicit_linking_boundary() {
    let source = "object Source { def handler() = () }\nobject Facade {\nexport Source.handler\nexport Source.{handler as renamed}\nexport Source.*\nexport Source.{handler as _, *}\nexport Source.given\n}\nobject Main { def run() = Facade.handler() }\n// export Ignored.fake\n";
    let facts = typed(source, "a.scala");
    assert!(facts.error_lines.is_empty(), "{:?}", facts.error_lines);
    let notes = facts
        .diagnostics
        .iter()
        .filter(|note| note.code == GraphFactCode::UnsupportedExportForwarding)
        .collect::<Vec<_>>();
    assert_eq!(
        notes.iter().map(|note| note.line).collect::<Vec<_>>(),
        [Some(3), Some(4), Some(5), Some(6), Some(7)]
    );
    assert!(notes.iter().all(|note| note.code.is_linking_only()));
    assert!(
        facts
            .calls
            .iter()
            .any(|call| call.callee == "Facade.handler")
    );
    assert!(!facts.declarations.iter().any(|d| d.name == "renamed"));
    assert!(
        !facts
            .exports
            .iter()
            .any(|e| e.source.as_deref() == Some("Source.handler"))
    );
}
