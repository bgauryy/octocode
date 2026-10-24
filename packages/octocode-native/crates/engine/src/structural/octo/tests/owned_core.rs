//! The owned matcher core: context selection, field and sibling relations, custom stops.

use super::*;

#[test]
fn owned_core_context_selects_only_the_requested_node() {
    let rule = json!({"pattern":{"context":"class ContextOnly { $K = $V; }","selector":"public_field_definition"}});
    let matches = run_rule("class Actual { value = 42; other = 7; }", "ts", &rule);
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].metavars["K"], ["value"]);
    assert_eq!(matches[0].metavars["V"], ["42"]);
}

#[test]
fn owned_core_context_has_no_wrapper_prefilter_anchor() {
    let rule = json!({"pattern":{"context":"class OnlyInPattern { $K = $V; }","selector":"public_field_definition"}});
    let query = StructuralQuery::new(None, Some(&rule)).unwrap();
    assert_eq!(query.prefilter(), Prefilter::None);
    assert_eq!(query.recovery_anchors(), None);
}

#[test]
fn owned_core_context_rejects_missing_unknown_and_broken_selections() {
    for rule in [
        json!({"pattern":{"context":"foo($X)","selector":"absent_kind"}}),
        json!({"pattern":{"context":"foo($X)","selector":"class_declaration"}}),
        json!({"pattern":{"context":"class C {","selector":"class_declaration"}}),
        json!({"pattern":{"context":"foo($X)"}}),
        json!({"pattern":{"context":"foo($X)","selector":"call_expression","strictness":"relaxed"}}),
    ] {
        assert!(CompiledRule::new(&lang("ts"), &rule).is_err(), "{rule}");
    }
}

#[test]
fn owned_core_field_relations_distinguish_names_values_and_ancestor_edges() {
    let source = "function alpha() { beta(); } function beta() { alpha(); }";
    let names = run_rule(
        source,
        "ts",
        json!({"kind":"function_declaration","has":{"kind":"identifier","regex":"^alpha$","field":"name"}}),
    );
    assert_eq!(names.len(), 1);
    assert!(names[0].text.starts_with("function alpha"));
    let calls = run_rule(
        source,
        "ts",
        json!({"kind":"identifier","regex":"^alpha$","inside":{"kind":"function_declaration","field":"body","stopBy":"end"}}),
    );
    assert_eq!(calls.len(), 1);
    assert!(calls[0].start_col > names[0].start_col);
    assert!(
        run_rule(
            source,
            "ts",
            json!({"kind":"function_declaration","has":{"kind":"call_expression","field":"name","stopBy":"end"}})
        )
        .is_empty()
    );
}

#[test]
fn owned_core_sibling_relations_skip_tokens_and_search_in_both_directions() {
    let source = "function f() { first(); middle(); last(); }";
    for rule in [
        json!({"kind":"expression_statement","precedes":{"kind":"expression_statement","regex":"^last","stopBy":"end"}}),
        json!({"kind":"expression_statement","follows":{"kind":"expression_statement","regex":"^first","stopBy":"end"}}),
    ] {
        assert_eq!(run_rule(source, "ts", &rule).len(), 2, "{rule}");
    }
    let first = json!({"context":"first();","selector":"expression_statement"});
    let first_then = |relation: Value| json!({"pattern": first, "precedes": relation});
    assert_eq!(
        run_rule(
            source,
            "ts",
            first_then(json!({"kind":"expression_statement","regex":"^last"}))
        )
        .len(),
        0
    );
    assert_eq!(
        run_rule(
            source,
            "ts",
            first_then(
                json!({"kind":"expression_statement","regex":"^middle","stopBy":"neighbor"})
            )
        )
        .len(),
        1
    );
    assert!(
        run_rule(
            "first(); function f() { last(); }",
            "ts",
            first_then(json!({"kind":"expression_statement","regex":"^last","stopBy":"end"}))
        )
        .is_empty()
    );
}

#[test]
fn owned_core_custom_stops_are_inclusive_and_do_not_leak_bindings() {
    let source = "function f() { first(); middle(); last(); }";
    let first = json!({"context":"first();","selector":"expression_statement"});
    let first_then = |relation: Value| json!({"pattern": first, "precedes": relation});
    let stopped = first_then(
        json!({"kind":"expression_statement","regex":"^last","stopBy":{"regex":"^middle"}}),
    );
    assert!(run_rule(source, "ts", &stopped).is_empty());
    let inclusive = first_then(
        json!({"kind":"expression_statement","regex":"^middle","stopBy":{"regex":"^middle"}}),
    );
    assert_eq!(run_rule(source, "ts", &inclusive).len(), 1);
    let no_leak = first_then(json!({
        "kind": "expression_statement",
        "regex": "^last",
        "stopBy": {
            "pattern": {"context": "$HIT($X);", "selector": "expression_statement"},
            "regex": "never"
        }
    }));
    let matches = run_rule(source, "ts", &no_leak);
    assert_eq!(matches.len(), 1);
    assert!(matches[0].metavars.is_empty());
}

#[test]
fn owned_core_ancestor_and_descendant_custom_stops_bound_the_search() {
    let source = "function outer() { function inner() { target(); } }";
    let blocked = run_rule(
        source,
        "ts",
        json!({"kind":"function_declaration","has":{"pattern":"target()","stopBy":{"kind":"function_declaration"}}}),
    );
    assert_eq!(blocked.len(), 1);
    assert!(blocked[0].text.starts_with("function inner"));
    assert!(
        run_rule(
            source,
            "ts",
            json!({"pattern":"target()","inside":{"kind":"program","stopBy":{"kind":"function_declaration"}}})
        )
        .is_empty()
    );
    assert_eq!(run_rule(source, "ts", json!({"pattern":"target()","inside":{"kind":"function_declaration","stopBy":{"kind":"function_declaration"}}}) ).len(), 1);
}

#[test]
fn owned_core_rejects_unknown_fields_and_stop_rule_grammar_kinds() {
    for rule in [
        json!({"kind":"identifier","inside":{"kind":"function_declaration","field":"absent_field"}}),
        json!({"kind":"identifier","follows":{"kind":"identifier","field":"name"}}),
        json!({"kind":"identifier","inside":{"kind":"function_declaration","stopBy":{"kind":"absent_kind"}}}),
    ] {
        assert!(CompiledRule::new(&lang("ts"), &rule).is_err(), "{rule}");
    }
}

#[test]
fn owned_core_stop_rule_depth_and_interruption_remain_explicit() {
    let mut rule = serde_json::json!({"kind":"identifier"});
    for _ in 0..65 {
        rule = serde_json::json!({"kind":"identifier","inside":{"kind":"program","stopBy":rule}});
    }
    assert!(CompiledRule::new(&lang("ts"), &rule).is_err());
    let language = lang("ts").tree_sitter_language();
    let tree = parse_tree(&language, "first(); middle(); last();").unwrap();
    let rule = CompiledRule::new(
        &lang("ts"),
        &json!({"not":{"precedes":{"kind":"expression_statement","stopBy":{"kind":"expression_statement"}}}}),
    )
    .unwrap();
    let line_index = crate::text::LineIndex::tree_sitter("first(); middle(); last();");
    let document = Document {
        line_index: &line_index,
        content: "first(); middle(); last();",
        root: tree.root_node(),
        deadline: Instant::now(),
    };
    let error = rule
        .matches(
            tree.root_node(),
            None,
            &document,
            &mut CaptureEnv::default(),
        )
        .expect_err("an expired relation cannot satisfy negation");
    assert_eq!(error.code, "structural.match.deadline");
}
