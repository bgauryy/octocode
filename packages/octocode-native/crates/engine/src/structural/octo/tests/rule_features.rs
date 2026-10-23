use crate::structural::{StructuralMatch, search};

fn matches(source: &str, rule: serde_json::Value) -> Vec<StructuralMatch> {
    search(source, "ts", None, Some(&rule)).expect("valid structural rule")
}

#[test]
fn ordinal_counts_filtered_named_siblings_and_supports_reverse_and_formula() {
    let source = "foo(a, /* note */ b, c);";
    for (position, reverse, expected) in [
        (serde_json::json!(2), false, vec!["b"]),
        (serde_json::json!(1), true, vec!["c"]),
        (serde_json::json!("2n+1"), false, vec!["a", "c"]),
        (serde_json::json!("-n+2"), false, vec!["a", "b"]),
    ] {
        let found = matches(
            source,
            serde_json::json!({"kind":"identifier","inside":{"kind":"arguments"},"nthChild":{"position":position,"reverse":reverse,"ofRule":{"kind":"identifier"}}}),
        );
        assert_eq!(
            found.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
            expected
        );
    }
    assert!(
        matches(
            source,
            serde_json::json!({"kind":"identifier","inside":{"kind":"arguments"},"nthChild":2})
        )
        .is_empty()
    );
}

#[test]
fn exact_range_uses_zero_based_unicode_scalar_boundaries_and_optional_columns() {
    let source = "\"😀\"; foo();\r\n";
    let found = matches(
        source,
        serde_json::json!({"kind":"identifier","range":{"start":{"line":0,"column":5},"end":{"line":0,"column":8}}}),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text, "foo");
    assert_eq!(found[0].start_col, 6);
    assert!(matches(source, serde_json::json!({"kind":"identifier","range":{"start":{"line":0,"column":0},"end":{"line":0,"column":11}}})).is_empty());
    assert_eq!(
        matches(
            source,
            serde_json::json!({"kind":"identifier","range":{"start":{"line":0},"end":{"line":0}}})
        )
        .len(),
        1
    );
}

#[test]
fn selectors_lower_relationships_boolean_rules_and_filtered_ordinals() {
    let source = "function f() { foo(a, b); bar(c); }";
    for (selector, expected) in [
        ("call_expression > identifier", vec!["foo", "bar"]),
        (
            "function_declaration call_expression",
            vec!["foo(a, b)", "bar(c)"],
        ),
        (
            "call_expression > :is(identifier, number)",
            vec!["foo", "bar"],
        ),
        ("identifier:not(identifier)", vec![]),
        (
            "arguments > identifier:nth-child(2 of identifier)",
            vec!["b"],
        ),
        (
            "arguments > identifier:nth-last-child(1 of identifier)",
            vec!["b", "c"],
        ),
        (
            "expression_statement:has(> call_expression)",
            vec!["foo(a, b);", "bar(c);"],
        ),
    ] {
        let found = matches(source, serde_json::json!({"kind":selector}));
        assert_eq!(
            found.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
            expected,
            "{selector}"
        );
    }
}

#[test]
fn malformed_positions_selectors_and_nested_filter_metadata_are_rejected() {
    for rule in [
        serde_json::json!({"kind":"identifier","nthChild":"2n++1"}),
        serde_json::json!({"kind":"identifier >"}),
        serde_json::json!({"kind":"identifier:is("}),
        serde_json::json!({"kind":"identifier","nthChild":{"position":1,"ofRule":{"kind":"identifier","field":"name"}}}),
        serde_json::json!({"kind":"identifier","range":{"start":{"line":2},"end":{"line":1}}}),
    ] {
        assert!(
            search("foo(a);", "ts", None, Some(&rule)).is_err(),
            "{rule}"
        );
    }
}

#[test]
fn adjacent_siblings_respect_punctuation_and_filtered_ordinals_bind_only_the_target() {
    let source = "class A { first=1; second=2; }";
    assert!(
        matches(
            source,
            serde_json::json!({"kind":"public_field_definition + public_field_definition"})
        )
        .is_empty()
    );
    assert_eq!(
        matches(
            source,
            serde_json::json!({"kind":"public_field_definition ~ public_field_definition"})
        )
        .len(),
        1
    );
    let found = matches(
        "foo(a, b, c);",
        serde_json::json!({"kind":"identifier","inside":{"kind":"arguments"},"nthChild":{"position":2,"ofRule":{"pattern":"$S"}}}),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text, "b");
    assert_eq!(found[0].metavars["S"], ["b"]);
}

#[test]
fn ordinal_filters_respect_captures_bound_by_the_outer_pattern() {
    let source = "const xs = [foo(a), foo(b), foo(a)];";
    let found = matches(
        source,
        serde_json::json!({"pattern":"foo($X)","nthChild":{"position":2,"ofRule":{"pattern":"foo($X)"}}}),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text, "foo(a)");
    assert_eq!(found[0].metavars["X"], ["a"]);
    assert!(found[0].start_col > 20);
}
