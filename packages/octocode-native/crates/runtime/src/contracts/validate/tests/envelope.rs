use super::*;

#[test]
fn union_envelope_normalization_preserves_response_controls() {
    let schema = serde_json::json!({"anyOf":[
        {"type":"object","properties":{"queries":{"type":"array"},"responseLength":{"type":"integer"},"responseSnapshot":{"not":{}}}},
        {"type":"object","properties":{"queries":{"not":{}},"responseSnapshot":{"type":"string"},"responseOffset":{"type":"integer"}}}
    ]});
    assert_eq!(
        super::super::one_row_envelope(
            &schema,
            serde_json::json!({"path":"src/a.rs","responseLength":900})
        ),
        serde_json::json!({"queries":[{"path":"src/a.rs"}],"responseLength":900})
    );
    let saved = serde_json::json!({"responseSnapshot":"saved","responseOffset":2});
    assert_eq!(
        super::super::one_row_envelope(&schema, saved.clone()),
        saved
    );
}

#[test]
fn row_plan_retains_union_envelope_checks_and_separates_rows() {
    let query = serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false});
    let input = serde_json::json!({"anyOf":[
        {"type":"object","properties":{"queries":{"type":"array","minItems":1,"items":query},"responseSnapshot":{"not":{}}},"required":["queries"],"additionalProperties":false},
        {"type":"object","properties":{"queries":{"not":{}},"responseSnapshot":{"type":"string","minLength":1}},"required":["responseSnapshot"],"additionalProperties":false}
    ]});
    let plan = super::super::RowPlan::derive(
        &serde_json::json!({"querySchema":query,"inputSchema":input}),
    );
    assert!(plan.item_schema.is_none());
    assert!(
        plan.envelope
            .pointer("/anyOf/0/properties/queries/items")
            .is_none()
    );
    for (mut value, valid) in [
        (serde_json::json!({"queries":[{"path":"a"}]}), true),
        (serde_json::json!({"responseSnapshot":"saved"}), true),
        (
            serde_json::json!({"queries":[{"path":"a"}],"responseSnapshot":"saved"}),
            false,
        ),
        (serde_json::json!({"queries":[]}), false),
        (serde_json::json!({}), false),
    ] {
        assert_eq!(
            super::super::validate_input_schema(
                &input,
                &plan.envelope,
                &mut value,
                &mut Vec::new()
            )
            .is_ok(),
            valid,
            "{value}"
        );
    }
}

/// An envelope field sent in a row is named as one to move beside
/// `queries`, never as a misspelled row field.
#[test]
fn an_envelope_field_in_a_row_is_moved_beside_queries() {
    let error = validate(
        "localFetch",
        json!({"queries":[{"path":"/tmp/a","ranges":["1-2"],"responseLength":3000}]}),
    )
    .expect_err("responseLength is an envelope field");
    assert_eq!(
        format_input_error("localFetch", &error),
        json!({"error":"Move 'responseLength' from queries[0] beside queries: it applies to the whole call."})
    );
}

#[test]
fn an_oversized_batch_says_how_to_split_it() {
    let rows = (0..7)
        .map(|_| json!({"path":"package.json","mainGoal":"g","reasoning":"r"}))
        .collect::<Vec<_>>();
    let error = validate("localFetch", json!({ "queries": rows })).expect_err("too many rows");
    let formatted = format_input_error("localFetch", &error).to_string();
    assert!(
        formatted.contains("split the batch into 2 calls"),
        "{formatted}"
    );
}

#[test]
fn a_top_level_brief_is_moved_into_each_row() {
    let error = crate::contracts::prepare_many_and_validate(
            "localSearch",
            json!({"mainGoal":"g","queries":[{"path":".","matchString":"x","mainGoal":"g","reasoning":"r"}]}),
        )
        .expect_err("mainGoal is per row");
    let formatted = format_input_error("localSearch", &error).to_string();
    assert!(
        formatted.contains("Move 'mainGoal' into each queries[] row"),
        "{formatted}"
    );
}
