use super::*;

#[test]
fn computed_recipients_and_repeated_arguments_have_independent_occurrences() {
    let source = "function handler(){} registry[key](handler, handler); factory()(handler);";
    let result = facts(source, "ts");
    let observed = slots(&result)
        .into_iter()
        .filter(|slot| matches!(slot.observation, GraphArgumentObservation::Reference { .. }))
        .collect::<Vec<_>>();
    assert_eq!(observed.len(), 3);
    assert_eq!(
        observed
            .iter()
            .map(|s| &s.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3
    );
    assert!(result.argument_sites.iter().any(|site| {
        site.recipient_name.is_none()
            && source[site.source_range.start_byte as usize..site.source_range.end_byte as usize]
                .starts_with("registry[key]")
    }));
    let edited = facts(&format!("// new snapshot\n{source}"), "ts");
    assert_ne!(slots(&result)[0].id, slots(&edited)[0].id);
    assert!(!result.calls.iter().any(|call| call.callee == "handler"));
}

#[test]
fn labels_unicode_crlf_and_multiple_groups_retain_full_slots() {
    let source = "def café(): pass\r\nconsume(callback = café)\r\n";
    let result = facts(source, "py");
    let slot = slots(&result)[0];
    assert_eq!(slot.label.as_deref(), Some("callback"));
    assert_eq!(
        &source[slot.source_range.start_byte as usize..slot.source_range.end_byte as usize],
        "callback = café"
    );
    assert_eq!(slot.range.start.line, 1);
    if languages::find_entry("scala").is_some() {
        let result = facts("object B { def f() = consume(handler)(handler) }", "scala");
        assert_eq!(result.argument_sites.len(), 1);
        assert_eq!(result.argument_sites[0].groups.len(), 2);
        assert_eq!(slots(&result).len(), 2);
    }
}

#[test]
fn argument_extraction_deadline_is_explicit_and_wide_lists_are_complete() {
    let source = format!("fn f() {{ consume({}); }}", vec!["handler"; 1200].join(","));
    let result = facts(&source, "rs");
    assert_eq!(slots(&result).len(), 1200);
    let cancelled = crate::signatures::graph_facts::extract_facts_before(
        &source,
        "fixture.rs",
        "rs",
        std::time::Instant::now(),
        false,
    )
    .expect("admitted")
    .facts;
    assert!(cancelled.diagnostics.iter().any(|d| matches!(
        d.code,
        GraphFactCode::ParseDeadline | GraphFactCode::TraversalDeadline
    )));
}

#[test]
fn decoded_unicode_and_transparent_wrappers_preserve_argument_and_name_ranges() {
    let source = "function café(){}\r\nconsume((café as unknown), café);";
    let result = facts(source, "ts");
    let observed = slots(&result);
    assert_eq!(observed.len(), 2);
    assert_eq!(
        &source[observed[0].source_range.start_byte as usize
            ..observed[0].source_range.end_byte as usize],
        "(café as unknown)"
    );
    for slot in observed {
        let GraphArgumentObservation::Reference {
            selection_byte_range,
            ..
        } = &slot.observation
        else {
            panic!("direct reference")
        };
        assert_eq!(
            &source
                [selection_byte_range.start_byte as usize..selection_byte_range.end_byte as usize],
            "café"
        );
        assert_eq!(slot.range.start.line, 1);
    }
}
