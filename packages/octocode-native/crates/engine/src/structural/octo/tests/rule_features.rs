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

/// `range` takes the units every match row prints: a 1-based line and a
/// 1-based UTF-16 column, the end exclusive. The emoji before `foo` is two
/// UTF-16 units, so character columns (6..9) miss and UTF-16 columns (7..10)
/// hit.
#[test]
fn exact_range_takes_one_based_lines_and_utf16_columns_with_optional_columns() {
    let source = "\"😀\"; foo();\r\nbar();\n";
    let range = |start: serde_json::Value, end: serde_json::Value| {
        matches(
            source,
            serde_json::json!({"kind":"identifier","range":{"start":start,"end":end}}),
        )
    };
    let found = range(
        serde_json::json!({"line":1,"column":7}),
        serde_json::json!({"line":1,"column":10}),
    );
    assert_eq!(
        found.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
        ["foo"]
    );
    assert_eq!((found[0].start_line, found[0].start_col + 1), (1, 7));
    assert_eq!((found[0].end_line, found[0].end_col + 1), (1, 10));
    // Character columns and the old 0-based units find nothing.
    assert!(
        range(
            serde_json::json!({"line":1,"column":6}),
            serde_json::json!({"line":1,"column":9})
        )
        .is_empty()
    );
    assert!(
        range(
            serde_json::json!({"line":1,"column":1}),
            serde_json::json!({"line":1,"column":12})
        )
        .is_empty()
    );
    // Lines alone select every node on that 1-based line.
    let on = |line: usize| {
        range(
            serde_json::json!({"line":line}),
            serde_json::json!({"line":line}),
        )
        .into_iter()
        .map(|m| m.text)
        .collect::<Vec<_>>()
    };
    assert_eq!(on(1), ["foo"]);
    assert_eq!(on(2), ["bar"]);
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
        serde_json::json!({"kind":"identifier","range":{"start":{"line":1,"column":4},"end":{"line":1,"column":2}}}),
    ] {
        assert!(
            search("foo(a);", "ts", None, Some(&rule)).is_err(),
            "{rule}"
        );
    }
    // Positions are 1-based: a 0 line or column is an input error that says so.
    for (start, end) in [
        (serde_json::json!({"line":0}), serde_json::json!({"line":1})),
        (serde_json::json!({"line":1}), serde_json::json!({"line":0})),
        (
            serde_json::json!({"line":1,"column":0}),
            serde_json::json!({"line":1,"column":4}),
        ),
        (
            serde_json::json!({"line":1,"column":1}),
            serde_json::json!({"line":1,"column":0}),
        ),
    ] {
        let rule = serde_json::json!({"kind":"identifier","range":{"start":start,"end":end}});
        let error = search("foo(a);", "ts", None, Some(&rule))
            .err()
            .unwrap_or_else(|| panic!("a 0 position is rejected: {rule}"))
            .to_string();
        assert!(error.contains("1-based"), "{rule}: {error}");
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
