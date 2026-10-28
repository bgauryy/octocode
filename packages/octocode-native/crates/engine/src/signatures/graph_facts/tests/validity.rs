use super::*;

#[cfg(feature = "tree-sitter-cpp")]
#[test]
fn enclosing_recovery_marks_macro_shaped_class_names_without_tainting_neighbors() {
    for modifier in ["PUBLIC_TAG", "RenamedTag", "visible"] {
        let source = format!(
            "class {modifier} Actual : public Base {{ public: void run(); }};\nclass Healthy {{}};\n"
        );
        let value = facts(&source, "declarations.cpp");
        let declarations = value["declarations"].as_array().expect("declarations");
        let candidate = declarations
            .iter()
            .find(|d| d["name"] == modifier)
            .expect("observed parser name");
        assert_eq!(
            candidate["parseAssessment"]["status"], "recovered",
            "{candidate}"
        );
        assert_eq!(
            candidate["parseAssessment"]["scope"],
            "enclosingDeclaration"
        );
        let healthy = declarations
            .iter()
            .find(|d| d["name"] == "Healthy")
            .expect("healthy neighbor");
        assert_eq!(healthy["parseAssessment"]["status"], "parsed", "{healthy}");
    }
}

#[test]
fn occurrence_ranges_are_utf8_and_positions_are_utf16() {
    let source = "// 😀\r\nfn café() { café(); }\r\n";
    let value = facts(source, "unicode.rs");
    let declaration = &value["declarations"][0];
    let selection = &declaration["selectionByteRange"];
    let start = selection["range"]["start_byte"].as_u64().unwrap() as usize;
    let end = selection["range"]["end_byte"].as_u64().unwrap() as usize;
    assert_eq!(&source[start..end], "café");
    assert_eq!(selection["unit"], 0);
    assert_eq!(declaration["selectionRange"]["start"]["line"], 1);
    assert_eq!(declaration["selectionRange"]["end"]["character"], 7);
    assert_eq!(declaration["parseAssessment"]["status"], "parsed");
    let call = &value["calls"][0];
    let start = call["sourceRange"]["range"]["start_byte"].as_u64().unwrap() as usize;
    let end = call["sourceRange"]["range"]["end_byte"].as_u64().unwrap() as usize;
    assert_eq!(&source[start..end], "café()");
    assert_eq!(call["sourceRange"]["unit"], 0);
    assert_eq!(
        value["sourceDigest"],
        crate::digest::sha256(source.as_bytes())
    );
}

#[test]
fn malformed_declarations_do_not_mark_healthy_siblings_as_recovered() {
    for (path, source, broken, healthy) in [
        (
            "a.rs",
            "fn broken() { let = 1; }\nfn healthy() {}\n",
            "broken",
            "healthy",
        ),
        (
            "a.py",
            "def broken():\n    x =\ndef healthy():\n    pass\n",
            "broken",
            "healthy",
        ),
        (
            "a.ts",
            "function broken() { let = ; }\nfunction healthy() {}\n",
            "broken",
            "healthy",
        ),
    ] {
        let value = facts(source, path);
        let declarations = value["declarations"].as_array().unwrap();
        let broken = declarations
            .iter()
            .find(|d| d["name"] == broken)
            .expect("broken occurrence retained");
        assert_eq!(
            broken["parseAssessment"]["status"], "recovered",
            "{path}: {broken}"
        );
        let healthy = declarations
            .iter()
            .find(|d| d["name"] == healthy)
            .expect("healthy occurrence retained");
        assert_eq!(
            healthy["parseAssessment"]["status"], "parsed",
            "{path}: {healthy}"
        );
    }
}

#[test]
fn a_callable_argument_is_not_reported_as_an_invocation() {
    let value = facts(
        "fn target() {}\nfn run() { store(target); }\n",
        "callback.rs",
    );
    let calls = value["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["callee"], "store");
}

#[test]
fn macro_parameter_blockers_belong_to_the_observed_region() {
    use crate::graph::{
        GraphArgumentBinding, GraphArgumentGroupObservation, GraphArgumentObservation,
    };
    use crate::text::analysis::UnitHandle;
    let text = "fn outside() {}\nimpl Host { items! { fn generated(callback: usize) { sink(callback); sink(outside); } } }\nfn healthy(local: usize) { sink(local); }\n";
    let document = crate::signatures::extract_graph_facts_with_metadata_inner(text, "macro.rs")
        .unwrap()
        .facts;
    let mut observed = Vec::new();
    for site in &document.argument_sites {
        for group in &site.groups {
            if let GraphArgumentGroupObservation::Slots { slots } = &group.observation {
                for slot in slots {
                    if let GraphArgumentObservation::Reference {
                        target_name,
                        binding,
                        ..
                    } = &slot.observation
                    {
                        observed.push((target_name.as_str(), binding, site.source_range.unit));
                    }
                }
            }
        }
    }
    let (_, binding, unit) = observed
        .iter()
        .find(|(name, _, _)| *name == "callback")
        .expect("macro parameter reference");
    assert_ne!(*unit, UnitHandle(0));
    let GraphArgumentBinding::Unrepresented { source_range } = binding else {
        panic!("observed local blocker: {binding:?}")
    };
    assert_eq!(source_range.unit, *unit);
    assert_eq!(
        &text[source_range.start_byte as usize..source_range.end_byte as usize],
        "callback"
    );
    let (_, outer, _) = observed
        .iter()
        .find(|(name, _, _)| *name == "outside")
        .unwrap();
    assert!(
        matches!(outer, GraphArgumentBinding::NotAssessed),
        "macro expansion/hygiene is unassessed"
    );
    let (_, local, unit) = observed
        .iter()
        .find(|(name, _, _)| *name == "local")
        .unwrap();
    assert_eq!(*unit, UnitHandle(0));
    assert!(
        matches!(local,GraphArgumentBinding::Unrepresented{source_range} if source_range.unit==UnitHandle(0))
    );
}

#[test]
fn macro_scope_barriers_stay_in_their_lexical_container() {
    use crate::graph::{
        GraphArgumentBinding, GraphArgumentGroupObservation, GraphArgumentObservation,
    };
    use crate::text::analysis::UnitHandle;
    for nested in [false, true] {
        let body = "fn generated(callback: usize) { sink(callback); }";
        let item = if nested {
            format!("items! {{ {body} }}")
        } else {
            body.to_owned()
        };
        let text = format!(
            "fn callback() {{}} impl Host {{ items! {{ {item} }} fn sibling(sibling_param: usize) {{ sink(sibling_param); }} }} fn healthy(outside_param: usize) {{ sink(outside_param); }}"
        );
        let document =
            crate::signatures::extract_graph_facts_with_metadata_inner(&text, "scopes.rs")
                .unwrap()
                .facts;
        let mut count = 0;
        for site in &document.argument_sites {
            for group in &site.groups {
                if let GraphArgumentGroupObservation::Slots { slots } = &group.observation {
                    for slot in slots {
                        if let GraphArgumentObservation::Reference {
                            target_name,
                            binding,
                            ..
                        } = &slot.observation
                        {
                            match target_name.as_str() {
                                "callback" => {
                                    count += 1;
                                    assert_ne!(site.source_range.unit, UnitHandle(0));
                                    assert!(
                                        matches!(binding, GraphArgumentBinding::Unrepresented {source_range} if source_range.unit == site.source_range.unit),
                                        "{binding:?}"
                                    );
                                }
                                "sibling_param" => {
                                    count += 1;
                                    assert!(matches!(binding, GraphArgumentBinding::NotAssessed));
                                }
                                "outside_param" => {
                                    count += 1;
                                    assert!(
                                        matches!(binding, GraphArgumentBinding::Unrepresented {source_range} if source_range.unit == UnitHandle(0))
                                    );
                                }
                                _ => panic!("unexpected reference: {target_name}"),
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(count, 3);
    }
    let document = crate::signatures::extract_graph_facts_with_metadata_inner(
        "items! { fn generated() {} } fn healthy(local: usize) { sink(local); }",
        "root.rs",
    )
    .unwrap()
    .facts;
    let site = document.argument_sites.last().unwrap();
    let GraphArgumentGroupObservation::Slots { slots } = &site.groups[0].observation else {
        panic!("slots")
    };
    assert!(
        matches!(&slots[0].observation, GraphArgumentObservation::Reference {target_name, binding: GraphArgumentBinding::NotAssessed, ..} if target_name=="local")
    );
}
