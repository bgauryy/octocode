use super::*;

/// A list value the enum does not list reads less than asked when it
/// goes (`sections:["patch"]` loses the patches): lossy, named.
#[test]
fn unlisted_array_values_drop_and_the_emptied_field_goes() {
    let input = json!({"queries":[{"sections":["files","x","patches","y"],"only":["z"]}]});
    let allowed = json!({"enum":["files","patches"]});
    let issues = [
        issue(
            "schema.enum",
            &["queries", "0", "sections", "1"],
            Some(allowed.clone()),
            Some(json!("x")),
        ),
        issue(
            "schema.enum",
            &["queries", "0", "sections", "3"],
            Some(allowed.clone()),
            Some(json!("y")),
        ),
        issue(
            "schema.enum",
            &["queries", "0", "only", "0"],
            Some(allowed),
            Some(json!("z")),
        ),
    ];
    let repaired = repair_input(&input, &issues, always).expect("lossy fix");
    assert_eq!(
        repaired.input,
        json!({"queries":[{"sections":["files","patches"]}]})
    );
    assert_eq!(
        repaired.dropped,
        [
            "queries.0.only.0",
            "queries.0.sections.1",
            "queries.0.sections.3"
        ]
    );
}

#[test]
fn row_indices_past_nine_remove_in_numeric_order() {
    let rows = (0..12).map(|i| json!({"extra": i})).collect::<Vec<_>>();
    let input = json!({"queries": rows});
    let issues = [2, 10, 11].map(|i| {
        let index = i.to_string();
        issue(
            "schema.unknown-field",
            &["queries", &index, "extra"],
            None,
            None,
        )
    });
    let repaired = repair_input(&input, &issues, always).expect("lossy fix");
    assert_eq!(
        repaired.dropped,
        ["queries.10.extra", "queries.11.extra", "queries.2.extra"]
    );
    let repaired = repaired.input;
    assert!(
        repaired["queries"]
            .as_array()
            .expect("rows")
            .iter()
            .enumerate()
            .all(|(i, row)| { matches!(i, 2 | 10 | 11) == row.get("extra").is_none() })
    );
}

#[test]
fn scalar_enum_case_and_number_bounds_have_one_exact_fix() {
    let input = json!({"queries":[{"view":" Files ","n":500,"m":0.5}]});
    let allowed = json!({"enum":["files","paths"]});
    let issues = [
        issue(
            "schema.enum",
            &["queries", "0", "view"],
            Some(allowed),
            Some(json!(" Files ")),
        ),
        issue(
            "schema.range",
            &["queries", "0", "n"],
            Some(json!({"minimum":1,"maximum":100})),
            Some(json!(500)),
        ),
        issue(
            "schema.range",
            &["queries", "0", "m"],
            Some(json!({"minimum":1})),
            Some(json!(0.5)),
        ),
    ];
    let repaired = repair_input(&input, &issues, always).expect("exact fix");
    assert_eq!(
        repaired.input,
        json!({"queries":[{"view":"files","n":100,"m":1}]})
    );
    assert!(repaired.dropped.is_empty());
}

/// An unlisted scalar becomes the one listed value it means; one no rule
/// maps is never dropped (its default would widen the call): no lead.
#[test]
fn an_unlisted_scalar_maps_to_its_one_value_or_has_no_lead() {
    let lead = |allowed: Value, field: &str, received: Value| {
        let input = json!({"queries":[{"path":"a.ts", field: received.clone()}]});
        let unlisted = issue(
            "schema.enum",
            &["queries", "0", field],
            Some(json!({ "enum": allowed })),
            Some(received),
        );
        repair_input(&input, &[unlisted], always).map(|repaired| {
            assert!(repaired.dropped.is_empty(), "{repaired:?}");
            repaired.input["queries"][0][field].clone()
        })
    };
    assert_eq!(
        lead(json!(["f", "d"]), "entryType", json!("file")),
        Some(json!("f"))
    );
    assert_eq!(
        lead(json!(["literal", "rust", "pcre2"]), "regex", json!("regex")),
        Some(json!("rust"))
    );
    assert_eq!(
        lead(
            json!(["pullRequest", "issue", "commit"]),
            "operation",
            json!("pr")
        ),
        Some(json!("pullRequest"))
    );
    assert_eq!(
        lead(json!(["standard", "symbols"]), "minify", json!("none")),
        None
    );
    assert_eq!(
        lead(
            json!(["smart", "sensitive", "insensitive"]),
            "caseMode",
            json!("ignore")
        ),
        None
    );
}

/// An unknown field set to `true` whose name one row field lists becomes
/// that field's value, exactly; `false`, or a row already setting that
/// field, keeps the unknown field (dropped, named).
#[test]
fn a_true_field_named_by_an_enum_value_becomes_that_value() {
    let named = |row: Value| {
        let input = json!({ "queries": [row] });
        let unknown = issue(
            "schema.unknown-field",
            &["queries", "0", "merged"],
            Some(json!({"knownFields":["state","owner"],"nameFields":["state"]})),
            None,
        );
        repair_input(&input, &[unknown], always).expect("a lead")
    };
    let exact = named(json!({"owner":"o","merged":true}));
    assert_eq!(
        exact.input,
        json!({"queries":[{"owner":"o","state":"merged"}]})
    );
    assert!(exact.dropped.is_empty());
    for row in [
        json!({"merged":false}),
        json!({"merged":true,"state":"open"}),
    ] {
        assert_eq!(named(row).dropped, ["queries.0.merged"]);
    }
}

/// A whole number or boolean sent to a text field becomes its text; a
/// fraction (whose text JSON already lost) has no lead.
#[test]
fn a_number_or_boolean_for_a_text_field_becomes_its_text() {
    let text = |received: Value| {
        let input = json!({"queries":[{"version": received.clone()}]});
        let typed = issue(
            "schema.type",
            &["queries", "0", "version"],
            Some(json!({"type":"string","minLength":1})),
            Some(received),
        );
        repair_input(&input, &[typed], always).map(|repaired| repaired.input)
    };
    assert_eq!(text(json!(3)), Some(json!({"queries":[{"version":"3"}]})));
    assert_eq!(
        text(json!(true)),
        Some(json!({"queries":[{"version":"true"}]}))
    );
    assert_eq!(text(json!(1.1)), None);
}

#[test]
fn a_number_or_boolean_sent_as_text_becomes_the_value() {
    let input = json!({"queries":[{"n":"3","b":" true ","x":"three"}]});
    let typed = |field: &str, kind: &str, text: &str| {
        issue(
            "schema.type",
            &["queries", "0", field],
            Some(json!({"type":kind})),
            Some(json!(text)),
        )
    };
    let repaired = repair_input(
        &input,
        &[typed("n", "integer", "3"), typed("b", "boolean", " true ")],
        always,
    )
    .expect("exact coercion");
    assert_eq!(
        repaired.input,
        json!({"queries":[{"n":3,"b":true,"x":"three"}]})
    );
    assert!(repair_input(&input, &[typed("x", "integer", "three")], always).is_none());
}

/// A boolean for an on/off enum becomes the value it means.
#[test]
fn a_boolean_for_an_on_off_enum_becomes_its_value() {
    for (allowed, received, expected) in [
        (json!(["literal", "rust", "pcre2"]), true, json!("rust")),
        (json!(["literal", "rust", "pcre2"]), false, json!("literal")),
        (json!(["off", "on", "dotall"]), true, json!("on")),
    ] {
        let input = json!({"queries":[{"field": received}]});
        let problem = issue(
            "schema.enum",
            &["queries", "0", "field"],
            Some(json!({"enum": allowed})),
            Some(json!(received)),
        );
        let repaired = repair_input(&input, &[problem], always).expect("exact");
        assert_eq!(repaired.input["queries"][0]["field"], expected);
        assert!(repaired.dropped.is_empty());
    }
}

/// A bare number for a count range (`stars:1000`) means at least or
/// exactly that many: no lead guesses which.
#[test]
fn a_number_for_a_count_range_has_no_lead() {
    let input = json!({"queries":[{"stars":1000}]});
    let count = |schema: Value| {
        issue(
            "schema.type",
            &["queries", "0", "stars"],
            Some(schema),
            Some(json!(1000)),
        )
    };
    let range = json!({"type":"string","pattern":"^\\d+$","x-countRange":true});
    assert!(repair_input(&input, &[count(range)], always).is_none());
    let text = json!({"type":"string"});
    assert_eq!(
        repair_input(&input, &[count(text)], always).map(|repaired| repaired.input),
        Some(json!({"queries":[{"stars":"1000"}]}))
    );
}
