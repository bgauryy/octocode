use super::*;

/// A field of a form whose omitted selector is one literal sets it.
#[test]
fn a_field_of_one_unselected_branch_sets_its_selector() {
    let input = json!({"queries":[{"path":".","nameRegex":"x"}]});
    let field = issue(
        "schema.unknown-field",
        &["queries", "0", "nameRegex"],
        Some(
            json!({"knownFields":["path"],"siblingRequires":["operation:\"files\""],
                "setFields":{"operation":"files"}}),
        ),
        None,
    );
    let repaired = repair_input(&input, &[field], always).expect("exact");
    assert_eq!(
        repaired.input,
        json!({"queries":[{"path":".","nameRegex":"x","operation":"files"}]})
    );
    assert!(repaired.dropped.is_empty() && repaired.approximate.is_empty());
}

#[test]
fn an_envelope_field_the_rows_accept_moves_into_each_row() {
    let input = json!({"mainGoal":"g","queries":[{"path":"a"},{"path":"b","mainGoal":"own"}]});
    let root = issue("schema.unknown-field", &["mainGoal"], None, None);
    let rows_accept = |candidate: &Value| candidate.get("mainGoal").is_none();
    let repaired =
        repair_input(&input, std::slice::from_ref(&root), only(rows_accept)).expect("relocation");
    assert_eq!(
        repaired.input,
        json!({"queries":[{"path":"a","mainGoal":"g"},{"path":"b","mainGoal":"own"}]})
    );
    assert!(repaired.dropped.is_empty());
    // Rows that refuse the field: the relocation falls back to a drop.
    let rows_refuse = |candidate: &Value| {
        candidate["queries"].as_array().is_some_and(|rows| {
            rows.iter()
                .all(|row| row.get("mainGoal") != Some(&json!("g")))
        })
    };
    let dropped = repair_input(&input, &[root], only(rows_refuse)).expect("lossy fallback");
    assert_eq!(dropped.dropped, ["mainGoal"]);
}

/// Zero of a positive size (`responseLength: 0`) is dropped so the
/// default applies, named as lossy, instead of becoming one-char pages;
/// when the field's default is that minimum, setting it is exact. A
/// field the row requires names what to read, so a value below its
/// minimum gets no lead.
#[test]
fn an_unsized_field_drops_to_its_default_unless_required() {
    let input = json!({"responseLength":0,"queries":[{"path":"a","line":-2}]});
    let unsized_issue = |path: &[&str], schema: Value, received| {
        issue("schema.range", path, Some(schema), Some(json!(received)))
    };
    let length = unsized_issue(&["responseLength"], json!({"minimum":1,"maximum":50000}), 0);
    let repaired = repair_input(&input, std::slice::from_ref(&length), always).expect("drop");
    assert_eq!(repaired.input, json!({"queries":[{"path":"a","line":-2}]}));
    assert_eq!(repaired.dropped, ["responseLength"]);
    let line = unsized_issue(
        &["queries", "0", "line"],
        json!({"minimum":1,"maximum":50000}),
        -2,
    );
    let requires_line = |candidate: &Value| match candidate.pointer("/queries/0/line") {
        Some(_) => Ok(()),
        None => Err(vec![issue(
            "schema.required",
            &["queries", "0", "line"],
            None,
            None,
        )]),
    };
    assert_eq!(repair_input(&input, &[line], requires_line), None);
    // Above the maximum a required field still clamps: that bound is
    // the nearest value the caller can have meant.
    let large = unsized_issue(
        &["queries", "0", "line"],
        json!({"minimum":1,"maximum":50000}),
        90_000,
    );
    let repaired = repair_input(&input, &[large], requires_line).expect("clamp");
    assert_eq!(repaired.input["queries"][0]["line"], 50_000);
}

/// A size whose default is its minimum (`historyDepth: 0`, default 1)
/// is set to it: the same call as dropping it, exact, nothing lost.
#[test]
fn an_unsized_field_whose_default_is_its_minimum_is_set_exactly() {
    let input = json!({"queries":[{"historyDepth":0}]});
    let depth = issue(
        "schema.range",
        &["queries", "0", "historyDepth"],
        Some(json!({"minimum":1,"maximum":50,"default":1})),
        Some(json!(0)),
    );
    let repaired = repair_input(&input, &[depth], always).expect("exact");
    assert_eq!(repaired.input, json!({"queries":[{"historyDepth":1}]}));
    assert!(repaired.dropped.is_empty() && repaired.approximate.is_empty());
}

/// An unknown envelope field the rows refuse is named where the caller
/// set it, never at the row path the relocation tried.
#[test]
fn a_refused_relocation_names_the_envelope_field() {
    let check = |value: &Value| {
        crate::contracts::prepare_many_and_validate("localFetch", value.clone())
            .map(|_| ())
            .map_err(|error| error.issues)
    };
    let input = json!({"queries":[{"path":"a.txt","mainGoal":"g","reasoning":"r"}],
            "bogusTop":1,"responseLength":0});
    let issues = check(&input).expect_err("unknown field and unsized page");
    let repaired = repair_input(&input, &issues, check).expect("lossy repair");
    assert_eq!(repaired.dropped, ["bogusTop", "responseLength"]);
    assert_eq!(
        repaired.input,
        json!({"queries":[{"path":"a.txt","mainGoal":"g","reasoning":"r"}]})
    );
}

#[test]
fn an_encoded_object_repairs_against_the_actual_tool_schema() {
    let questions = json!({"present":{"type":"noul","instructions":"Is the word present?"}});
    let input = json!({"queries":[{"state":"word","questions":questions.to_string()}]});
    let check = |value: &Value| {
        crate::contracts::prepare_many_and_validate("clasify", value.clone())
            .map(|_| ())
            .map_err(|error| error.issues)
    };
    let issues = check(&input).expect_err("object must not be a string");
    let repaired = repair_input(&input, &issues, check).expect("validated object repair");
    assert_eq!(repaired.input["queries"][0]["questions"], questions);
    assert!(repaired.dropped.is_empty());
}

#[test]
fn singleton_repair_preserves_literals_and_requires_the_whole_array_schema() {
    for (schema, text, expected) in [
        (
            json!({"type":"array","items":{"type":"string"}}),
            " [ab]*.ts ",
            true,
        ),
        (
            json!({"type":"array","items":{"enum":["allowed"]}}),
            "different",
            false,
        ),
        (
            json!({"type":"array","minItems":2,"items":{"type":"string"}}),
            "one",
            false,
        ),
        (
            json!({"type":"array","items":{"type":"number"}}),
            "one",
            false,
        ),
        (
            json!({"type":"array","items":{"type":"string"}}),
            "[\"unfinished",
            false,
        ),
    ] {
        let input = json!({"value":text});
        let problem = issue("schema.type", &["value"], Some(schema), Some(json!(text)));
        let repair = repair_input(&input, &[problem], always);
        assert_eq!(repair.is_some(), expected, "{text}");
        if let Some(repair) = repair {
            assert_eq!(repair.input, json!({"value":[text]}));
            assert!(repair.dropped.is_empty());
        }
    }
}

/// A list for a one-value row field becomes one row per item, the row's
/// other fields copied, later rows keeping their order; a list the field
/// rejects an item of, an empty list, or two lists in one row has no lead.
#[test]
fn a_list_for_a_one_value_field_becomes_one_row_per_item() {
    let string = json!({"type":"string","minLength":1});
    let list = |row: &str, field: &str, received: Value| {
        issue(
            "schema.type",
            &["queries", row, field],
            Some(string.clone()),
            Some(received),
        )
    };
    let input = json!({"queries":[
        {"matchString":["a","b"],"path":"src"},
        {"matchString":"c","path":"lib"}
    ]});
    let repaired = repair_input(
        &input,
        &[list("0", "matchString", json!(["a", "b"]))],
        always,
    )
    .expect("exact");
    assert_eq!(
        repaired.input,
        json!({"queries":[
            {"matchString":"a","path":"src"},
            {"matchString":"b","path":"src"},
            {"matchString":"c","path":"lib"}
        ]})
    );
    assert!(repaired.dropped.is_empty());
    for received in [json!([]), json!(["a", ""]), json!(["a", 1])] {
        let input = json!({"queries":[{"matchString": received.clone()}]});
        let problem = list("0", "matchString", received.clone());
        assert!(
            repair_input(&input, &[problem], always).is_none(),
            "{received}"
        );
    }
    let input = json!({"queries":[{"matchString":["a","b"],"path":["x","y"]}]});
    let both = [
        list("0", "matchString", json!(["a", "b"])),
        list("0", "path", json!(["x", "y"])),
    ];
    assert!(repair_input(&input, &both, always).is_none());
}

/// A list for a one-value field whose row sets a field anchored to one
/// value (`line` for `symbolName`) has no split: every row would keep
/// that one anchor.
#[test]
fn a_list_with_an_anchored_sibling_has_no_split() {
    let symbol = json!({"type":"string","minLength":1,"x-anchoredBy":["line","orderHint"]});
    let lead = |row: Value| {
        let input = json!({ "queries": [row] });
        let problem = issue(
            "schema.type",
            &["queries", "0", "symbolName"],
            Some(symbol.clone()),
            Some(json!(["a", "b"])),
        );
        repair_input(&input, &[problem], always).map(|repaired| repaired.input)
    };
    assert_eq!(
        lead(json!({"symbolName":["a","b"],"path":"x.rs","line":4})),
        None
    );
    assert_eq!(
        lead(json!({"symbolName":["a","b"],"path":"x.rs"})),
        Some(json!({"queries":[
            {"symbolName":"a","path":"x.rs"},
            {"symbolName":"b","path":"x.rs"}
        ]}))
    );
}

/// A row field only the envelope takes moves beside `queries`, exactly;
/// rows that set it to different values have no one envelope value.
#[test]
fn an_envelope_field_in_a_row_moves_beside_queries() {
    let check = |value: &Value| {
        crate::contracts::prepare_many_and_validate("localFetch", value.clone())
            .map(|_| ())
            .map_err(|error| error.issues)
    };
    let lead = |input: Value| {
        let issues = check(&input).expect_err("an envelope field in a row");
        repair_input(&input, &issues, check)
    };
    let lifted = lead(json!({"queries":[
        {"path":"a.txt","responseLength":3000},
        {"path":"b.txt","responseLength":3000}
    ]}))
    .expect("exact");
    assert_eq!(
        lifted.input,
        json!({"queries":[{"path":"a.txt"},{"path":"b.txt"}],"responseLength":3000})
    );
    assert!(lifted.dropped.is_empty() && lifted.approximate.is_empty());
    assert!(
        lead(json!({"queries":[
            {"path":"a.txt","responseLength":3000},
            {"path":"b.txt","responseLength":2000}
        ]}))
        .is_none()
    );
}

/// A bare array of rows becomes the envelope, exactly.
#[test]
fn a_bare_array_becomes_the_envelope() {
    let input = json!([{"path":"a"},{"path":"b"}]);
    let wrap = issue(crate::contracts::ENVELOPE_ARRAY, &[], None, None);
    let repaired = repair_input(&input, &[wrap], always).expect("exact");
    assert_eq!(
        repaired.input,
        json!({"queries":[{"path":"a"},{"path":"b"}]})
    );
    assert!(repaired.dropped.is_empty());
}

/// A nested envelope's rows replace it; its envelope fields move beside
/// `queries` and its row fields into each lifted row; the issues the
/// lifted rows then show are repaired in the next pass (an unknown
/// field is dropped, named).
#[test]
fn a_nested_envelope_is_lifted_and_its_fields_kept_or_named() {
    let input = json!({"queries":[
        {"path":"z"},
        {"bogus":1,"responseLength":3000,"extras":["stats"],
            "queries":[{"path":"a"},{"path":"b","extras":["hints"]}]}
    ]});
    let nested = issue(
        crate::contracts::ENVELOPE_NESTED,
        &["queries", "1", "queries"],
        Some(json!({"envelopeFields":["responseLength"]})),
        None,
    );
    let check = |candidate: &Value| -> Result<(), Vec<ValidationIssue>> {
        let rows = candidate["queries"].as_array().cloned().unwrap_or_default();
        let unknown = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.get("bogus").is_some())
            .map(|(i, _)| {
                let index = i.to_string();
                issue(
                    "schema.unknown-field",
                    &["queries", &index, "bogus"],
                    None,
                    None,
                )
            })
            .collect::<Vec<_>>();
        if unknown.is_empty() {
            Ok(())
        } else {
            Err(unknown)
        }
    };
    let repaired = repair_input(&input, &[nested], check).expect("lifted");
    assert_eq!(
        repaired.input,
        json!({"queries":[
                {"path":"z"},
                {"path":"a","extras":["stats"]},
                {"path":"b","extras":["hints"]}
            ],"responseLength":3000})
    );
    assert_eq!(repaired.dropped, ["queries.1.bogus", "queries.2.bogus"]);
}
