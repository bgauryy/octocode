use super::adapter_repairs::typed;
use super::*;

#[test]
fn namespace_occurrences_keep_exact_names_ranges_and_parentage() {
    for extension in ["cpp", "cu"] {
        if languages::find_entry(extension).is_none() {
            continue;
        }
        let source = "// λ🙂\nnamespace Hidden { struct Box {}; }\nnamespace Hidden { void run(){ new Box(); } }\nnamespace Outer::Inner { struct Deep {}; }\nnamespace Ω { namespace Nested { struct Unicode {}; } }\nnamespace { struct Anonymous {}; namespace { struct Deeper {}; } }\nnamespace { struct Another {}; }\nstruct Global {}; void run(){ new ::Global(); new Hidden::Box(); new Outer::Inner::Deep(); }\n";
        let facts = typed(source, &format!("a.{extension}"));
        assert!(
            facts.error_lines.is_empty(),
            "{extension}: {:?}",
            facts.error_lines
        );
        let namespaces = facts
            .declarations
            .iter()
            .filter(|d| d.kind == "namespace")
            .collect::<Vec<_>>();
        assert_eq!(namespaces.len(), 8, "{extension}: {namespaces:?}");
        assert_eq!(namespaces.iter().filter(|d| d.name == "Hidden").count(), 2);
        assert_eq!(
            namespaces
                .iter()
                .filter(|d| d.name == "(anonymous namespace)")
                .count(),
            3
        );
        assert_eq!(
            namespaces
                .iter()
                .map(|d| &d.id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            namespaces.len()
        );
        for namespace in &namespaces {
            let range = namespace.source_range.as_ref().unwrap();
            let selection = namespace.selection_byte_range.as_ref().unwrap();
            let declaration = &source[range.start_byte as usize..range.end_byte as usize];
            let selected = &source[selection.start_byte as usize..selection.end_byte as usize];
            assert!(declaration.starts_with("namespace"));
            if namespace.name == "(anonymous namespace)" {
                assert_eq!(
                    selected, "namespace",
                    "source keyword, not invented name bytes"
                );
                assert!(!namespace.exported);
            } else {
                assert_eq!(selected, namespace.name);
            }
            assert!(matches!(
                namespace.parse_assessment,
                crate::graph::GraphParseAssessment::Parsed { .. }
            ));
        }
        for (name, expected_parent) in [
            ("Box", "Hidden"),
            ("Deep", "Outer::Inner"),
            ("Unicode", "Nested"),
            ("Anonymous", "(anonymous namespace)"),
            ("Deeper", "(anonymous namespace)"),
            ("Another", "(anonymous namespace)"),
        ] {
            let declaration = facts.declarations.iter().find(|d| d.name == name).unwrap();
            let parent = facts
                .declarations
                .iter()
                .find(|d| Some(&d.id) == declaration.parent.as_ref())
                .unwrap();
            assert_eq!(parent.name, expected_parent);
            assert_eq!(parent.kind, "namespace");
            assert!(facts.edges.iter().any(|edge| edge.relation == "contains"
                && edge.from == parent.id
                && edge.to == declaration.id));
        }
        let hidden_run = facts
            .declarations
            .iter()
            .find(|d| d.name == "run" && d.parent.is_some())
            .unwrap();
        assert_eq!(
            hidden_run.kind, "function",
            "namespace function is not a type method"
        );
        let hidden_parent = hidden_run.parent.as_ref().unwrap();
        assert_eq!(
            hidden_parent, &namespaces[1].id,
            "reopened occurrence has its own parent identity"
        );
        let deep_scope = facts
            .declarations
            .iter()
            .find(|d| d.name == "Deeper")
            .unwrap()
            .parent
            .as_ref()
            .unwrap();
        let deep_scope = namespaces.iter().find(|d| &d.id == deep_scope).unwrap();
        assert!(
            deep_scope.parent.is_some(),
            "nested anonymous scope cannot flatten to root"
        );
        let labels = facts
            .calls
            .iter()
            .map(|call| call.callee.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            ["Box", "::Global", "Hidden::Box", "Outer::Inner::Deep"]
        );
        assert!(
            facts
                .calls
                .iter()
                .all(|call| call.binding == crate::graph::GraphCallBinding::ConstructedType)
        );
        let lean = crate::signatures::extract_declarations_document_with_extension(
            source,
            &format!("a.{extension}"),
            extension,
        )
        .unwrap();
        assert_eq!(lean.declarations, facts.declarations);
        let decoded: GraphFactsDocument =
            serde_json::from_str(&serde_json::to_string(&facts).unwrap()).unwrap();
        assert_eq!(
            decoded, facts,
            "cache serialization preserves distinct scope identities"
        );
    }
}

#[test]
fn anonymous_namespace_labels_cannot_be_parsed_as_type_qualifiers() {
    for extension in ["cpp", "cu"] {
        if languages::find_entry(extension).is_none() {
            continue;
        }
        let source = "namespace { struct Box {}; } void run(){ new Box(); }";
        let facts = typed(source, &format!("a.{extension}"));
        let box_decl = facts.declarations.iter().find(|d| d.name == "Box").unwrap();
        let parent = facts
            .declarations
            .iter()
            .find(|d| Some(&d.id) == box_decl.parent.as_ref())
            .unwrap();
        assert_eq!(parent.name, "(anonymous namespace)");
        assert!(!box_decl.exported);
        assert_eq!(
            facts.calls[0].callee, "Box",
            "source label never acquires an anonymous qualifier"
        );
    }
}

#[test]
fn named_namespace_visibility_is_transparent_but_anonymous_and_static_are_not() {
    for extension in ["cpp", "cu"] {
        if languages::find_entry(extension).is_none() {
            continue;
        }
        let source = "namespace Named { void visible(){} inline static void hidden(){} namespace Nested { struct Public {}; } struct Type { void member(){} }; }\nnamespace Named { void reopened(){} }\nnamespace { void private_fn(){} namespace Named { struct Private {}; } }\nnamespace Outer { namespace { struct AlsoPrivate {}; } }\n";
        let facts = typed(source, &format!("a.{extension}"));
        assert!(facts.error_lines.is_empty());
        for (name, expected) in [
            ("visible", true),
            ("hidden", false),
            ("Public", true),
            ("Type", true),
            ("member", false),
            ("reopened", true),
            ("private_fn", false),
            ("Private", false),
            ("AlsoPrivate", false),
        ] {
            let declaration = facts.declarations.iter().find(|d| d.name == name).unwrap();
            assert_eq!(declaration.exported, expected, "{extension}: {name}");
            assert_eq!(
                facts.exports.iter().any(|export| export.name == name),
                expected
            );
        }
        let lean = crate::signatures::extract_declarations_document_with_extension(
            source,
            &format!("a.{extension}"),
            extension,
        )
        .unwrap();
        assert_eq!(lean.declarations, facts.declarations);
        assert_eq!(lean.exports, facts.exports);
    }
}
