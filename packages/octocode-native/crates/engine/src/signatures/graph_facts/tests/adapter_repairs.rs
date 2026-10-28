use super::*;

#[test]
fn constructor_occurrences_survive_without_inventing_argument_targets() {
    for (path, source, labels) in [
        (
            "a.java",
            "class Parent { Parent(Runnable value) {} } class Child extends Parent { static void handler() {} Child(){ super(Child::handler); } Child(int x){ this(); } }",
            vec!["super", "this"],
        ),
        (
            "a.cpp",
            "void handler() {} struct Box { Box(void (*value)()) {} }; void run(){ auto value = new Box(handler); }",
            vec!["Box"],
        ),
        (
            "a.cu",
            "void handler() {} struct Box { Box(void (*value)()) {} }; void run(){ auto value = new Box(handler); }",
            vec!["Box"],
        ),
        (
            "a.cs",
            "class Parent { public Parent(System.Action value) {} } class Child : Parent { static void handler() {} public Child(): base(handler) {} } class Box { public Box(System.Action value) {} static void handler() {} static void Run(){ Box value = new(handler); } }",
            vec!["base", "new"],
        ),
    ] {
        if languages::find_entry(&extension_of(path, true, "")).is_none() {
            continue;
        }
        let facts = extract_graph_facts_with_metadata(source, path)
            .unwrap()
            .facts;
        assert_eq!(
            facts
                .calls
                .iter()
                .map(|call| call.callee.as_str())
                .collect::<Vec<_>>(),
            labels,
            "{path}: {facts:?}"
        );
    }
}

#[test]
fn constructor_binding_ranges_and_independent_arguments_are_retained() {
    use crate::graph::{GraphCallBinding as B, GraphCallBoundary as R};
    for (path, source, expected) in [
        (
            "a.java",
            "// λ🙂\nclass C { C(){ this(null); } C(Object x){ super(); } void run(){ new pkg.Box<String>(handler); } }",
            vec![
                (
                    "this(null);",
                    "this",
                    B::Unassessed {
                        reason: R::ConstructorDelegation,
                    },
                ),
                (
                    "super();",
                    "super",
                    B::Unassessed {
                        reason: R::ConstructorDelegation,
                    },
                ),
                (
                    "new pkg.Box<String>(handler)",
                    "pkg.Box",
                    B::ConstructedType,
                ),
            ],
        ),
        (
            "a.cs",
            "// λ🙂\nclass C { C(): base(handler) {} void Run(){ C x = new(handler); new pkg.Box<int>(); } }",
            vec![
                (
                    ": base(handler)",
                    "base",
                    B::Unassessed {
                        reason: R::ConstructorDelegation,
                    },
                ),
                (
                    "new(handler)",
                    "new",
                    B::Unassessed {
                        reason: R::TargetTypeUnavailable,
                    },
                ),
                ("new pkg.Box<int>()", "pkg.Box", B::ConstructedType),
            ],
        ),
        (
            "a.cpp",
            "// λ🙂\nvoid run(){ new ns::Box<int>(handler); new Box(); new Box; }",
            vec![
                ("new ns::Box<int>(handler)", "ns::Box", B::ConstructedType),
                ("new Box()", "Box", B::ConstructedType),
                ("new Box", "Box", B::ConstructedType),
            ],
        ),
        (
            "a.cu",
            "void run(){ new Box(handler); }",
            vec![("new Box(handler)", "Box", B::ConstructedType)],
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
        assert_eq!(
            facts.calls.len(),
            expected.len(),
            "{path}: {:?}",
            facts.calls
        );
        assert_eq!(
            facts.source_digest,
            crate::digest::sha256(source.as_bytes())
        );
        for (call, (text, callee, binding)) in facts.calls.iter().zip(expected) {
            assert_eq!(call.binding, binding, "{path}: {call:?}");
            assert_eq!(call.callee, callee);
            assert_eq!(call.kind, "constructs");
            let range = call.source_range.as_ref().unwrap();
            assert_eq!(
                source.get(range.start_byte as usize..range.end_byte as usize),
                Some(text)
            );
            assert!(call.caller_id.is_some());
            assert!(matches!(
                call.parse_assessment,
                crate::graph::GraphParseAssessment::Parsed { .. }
            ));
            for site in facts
                .argument_sites
                .iter()
                .filter(|site| site.source_range == *range)
            {
                assert_eq!(site.call_id.as_deref(), Some(call.id.as_str()));
            }
        }
        let reference_slots = facts
            .argument_sites
            .iter()
            .flat_map(|site| &site.groups)
            .filter_map(|group| match &group.observation {
                crate::graph::GraphArgumentGroupObservation::Slots { slots } => Some(slots),
                _ => None,
            })
            .flatten()
            .filter(|slot| {
                matches!(
                    slot.observation,
                    crate::graph::GraphArgumentObservation::Reference { .. }
                )
            })
            .count();
        assert!(
            reference_slots >= 1,
            "{path}: argument evidence survives independently"
        );
    }
}

#[test]
fn constructor_forms_outside_named_type_admission_remain_observations() {
    use crate::graph::{GraphCallBinding as B, GraphCallBoundary as R};
    for path in ["a.cpp", "a.cu"] {
        if languages::find_entry(&extension_of(path, true, "")).is_none() {
            continue;
        }
        let source = "void run(){ new (place()) Box(handler); new Box[3]; new int(1); new typename T::Box(handler); }";
        let facts = typed(source, path);
        let constructs = facts
            .calls
            .iter()
            .filter(|call| call.kind == "constructs")
            .collect::<Vec<_>>();
        assert_eq!(constructs.len(), 4, "{path}: {:?}", facts.calls);
        assert!(constructs.iter().all(|call| call.binding
            == B::Unassessed {
                reason: R::UnsupportedConstructorForm
            }));
        assert_eq!(
            facts
                .calls
                .iter()
                .filter(|call| call.callee == "place")
                .count(),
            1
        );
        assert!(!facts.calls.iter().any(|call| call.callee == "handler"));
    }
}

#[test]
fn oxc_constructor_policy_stays_distinct_from_named_type_admission() {
    let facts = crate::signatures::extract_graph_facts_with_metadata_inner(
        "function Factory() {} new Factory(handler); Factory();",
        "a.js",
    )
    .unwrap()
    .facts;
    assert_eq!(
        facts
            .calls
            .iter()
            .map(|call| call.kind.as_str())
            .collect::<Vec<_>>(),
        ["constructs", "calls"]
    );
    assert!(
        facts
            .calls
            .iter()
            .all(|call| call.binding == crate::graph::GraphCallBinding::SyntaxLookup)
    );
}

#[test]
fn constructor_recovery_never_taints_a_healthy_neighbor() {
    let source = "class C { void broken(){ new Box(,); } void healthy(){ new Box(); } }";
    let facts = typed(source, "a.java");
    assert!(!facts.error_lines.is_empty());
    assert_eq!(facts.calls.len(), 2);
    assert!(matches!(
        facts.calls[0].parse_assessment,
        crate::graph::GraphParseAssessment::Recovered { .. }
    ));
    assert!(matches!(
        facts.calls[1].parse_assessment,
        crate::graph::GraphParseAssessment::Parsed { .. }
    ));
    let expired = extract_graph_facts_with_metadata_before(
        source,
        "a.java",
        "java",
        std::time::Instant::now() - std::time::Duration::from_millis(1),
    )
    .unwrap()
    .facts;
    assert!(expired.calls.is_empty());
    assert!(expired.diagnostics.iter().any(|note| matches!(
        note.code,
        GraphFactCode::ParseDeadline | GraphFactCode::TraversalDeadline
    )));
}

#[test]
fn java_instance_qualified_creation_and_nested_calls_keep_distinct_premises() {
    let facts = typed(
        "class C { void run(){ outer.new Inner(handler); new Box(build()) { void extra(){ nested(); } }; } }",
        "a.java",
    );
    assert!(facts.error_lines.is_empty());
    assert_eq!(facts.calls.len(), 4);
    assert_eq!(
        facts.calls[0].binding,
        crate::graph::GraphCallBinding::Unassessed {
            reason: crate::graph::GraphCallBoundary::UnsupportedConstructorForm
        }
    );
    assert_eq!(
        facts.calls[1].binding,
        crate::graph::GraphCallBinding::ConstructedType
    );
    assert_eq!(facts.calls[2].callee, "build");
    assert_eq!(facts.calls[3].callee, "nested");
    assert!(!facts.calls.iter().any(|call| call.callee == "handler"));
}

#[test]
fn every_installed_call_adapter_has_a_positive_and_declaration_only_control() {
    let cases = [
        ("ts", "function run(){ work(); }"),
        ("tsx", "function run(){ work(); }"),
        ("js", "function run(){ work(); }"),
        ("py", "def run():\n work()\n"),
        ("go", "package p\nfunc run(){ work() }"),
        ("rs", "fn run(){ work(); }"),
        ("java", "class C { void run(){ work(); } }"),
        ("c", "void run(){ work(); }"),
        ("cpp", "void run(){ work(); }"),
        ("cu", "void run(){ work(); }"),
        ("cs", "class C { void run(){ work(); } }"),
        ("scala", "object C { def run() = work() }"),
        ("asm", "start:\n nop\n"),
    ];
    for entry in languages::all_entries() {
        let (ext, source) = cases
            .iter()
            .find(|(ext, _)| entry.extensions.contains(ext))
            .unwrap_or_else(|| panic!("missing conformance case: {}", entry.name));
        let path = format!("a.{ext}");
        let facts = typed(source, &path);
        assert!(facts.error_lines.is_empty(), "{}", entry.name);
        assert_eq!(
            facts.calls.len(),
            usize::from(entry.fact_capabilities.calls),
            "{}",
            entry.name
        );
        assert!(facts.calls.iter().all(|call| call.binding
            == crate::graph::GraphCallBinding::SyntaxLookup
            && call.callee == "work"));
        let lean =
            crate::signatures::extract_declarations_document_with_extension(source, &path, ext)
                .unwrap();
        assert!(lean.calls.is_empty());
        assert!(lean.argument_sites.is_empty());
        assert_eq!(lean.declarations, facts.declarations);
    }
}

pub(super) fn typed(source: &str, path: &str) -> GraphFactsDocument {
    extract_graph_facts_with_metadata(source, path)
        .unwrap()
        .facts
}
