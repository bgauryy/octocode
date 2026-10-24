use super::super::validate_schema;
use super::*;

#[test]
fn selected_operation_reports_its_missing_field() {
    for question in [
        json!({"type":"choice","instructions":"Pick a label"}),
        json!({"type":"score","instructions":"Rate the evidence"}),
    ] {
        let error=prepare_many_and_validate("clasify",json!({"queries":[{"operation":"evaluate","content":"Observed evidence","questions":{"check":question}}]})).expect_err("missing criteria");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.path == ["queries", "0", "questions", "check", "criteria"]),
            "{error:?}"
        );
    }
}

#[test]
fn unknown_operation_is_reported_at_the_selector() {
    let error = prepare_many_and_validate(
        "ghSearchHistory",
        json!({"queries":[{"operation":"invented","owner":"o","repo":"r"}]}),
    )
    .expect_err("unknown operation");
    assert!(
        error
            .issues
            .iter()
            .any(|issue| issue.path == ["queries", "0", "operation"]),
        "{error:?}"
    );
}

#[test]
fn fields_from_a_different_view_are_rejected_without_changing_the_input() {
    let input = json!({"queries":[{"operation":"references","path":"a.rs","symbolName":"run","line":1,"view":"files","contextLines":{"before":1,"after":1}}]});
    let before = input.clone();
    let error = prepare_many_and_validate("lspSearch", input.clone())
        .expect_err("context belongs to content rows");
    assert!(!error.issues.is_empty());
    assert_eq!(input, before);
}

#[test]
fn arbitrary_selector_names_obey_their_schema() {
    let schema = json!({"anyOf":[
        {"type":"object","additionalProperties":false,"properties":{"format":{"const":"text"},"content":{"type":"string"}},"required":["format","content"]},
        {"type":"object","additionalProperties":false,"properties":{"format":{"const":"number"},"value":{"type":"number"}},"required":["format","value"]}
    ]});
    for (mut input, accepted) in [
        (json!({"format":"text","content":"hello"}), true),
        (json!({"format":"number","value":12}), true),
        (json!({"format":"number","content":"12"}), false),
    ] {
        assert_eq!(
            validate_schema(&schema, &schema, &mut input, &mut vec![]).is_ok(),
            accepted,
            "{input}"
        );
    }
}

#[test]
fn ordered_bounds_in_a_union_report_the_invalid_bound() {
    for (kind, lower, upper) in [
        ("number", json!(12), json!(4)),
        (
            "timestamp",
            json!("2026-10-24T12:00:01+02:00"),
            json!("2026-10-24T10:00:00Z"),
        ),
    ] {
        let schema = json!({"anyOf":[
            {"type":"object","additionalProperties":false,
             "properties":{"name":{"type":"string"}},"required":["name"]},
            {"type":"object","additionalProperties":false,
             "properties":{"name":{"type":"string"},"window":{
                 "type":"object","properties":{"floor":{},"ceiling":{}},
                 "required":["floor","ceiling"],
                 "x-order":{"lower":"floor","upper":"ceiling","kind":kind}
             }},"required":["name","window"]}
        ]});
        let mut value = json!({"name":"evidence","window":{"floor":lower,"ceiling":upper}});
        let before = value.clone();
        let error = validate_schema(&schema, &schema, &mut value, &mut vec![])
            .expect_err("reversed bounds");
        assert_eq!(value, before);
        assert_eq!(error.issues.len(), 1, "{error:?}");
        assert_eq!(error.issues[0].path, ["window", "ceiling"]);
        assert_eq!(error.issues[0].rule_id, "schema.order");

        value["window"]["ceiling"] = lower;
        validate_schema(&schema, &schema, &mut value, &mut vec![])
            .expect("equal bounds repair the selected form");
    }
}

#[test]
fn reversed_source_ranges_keep_the_range_diagnostic_in_every_read_form() {
    for tool in ["localFetch", "ghGetFileContent"] {
        for extra in [
            json!({}),
            json!({"view":"compact"}),
            json!({"matchString":["name"]}),
        ] {
            let mut query = json!({"path":"a.rs","ranges":[{"startLine":12,"endLine":4}]});
            query
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            if tool == "ghGetFileContent" {
                query["owner"] = json!("o");
                query["repo"] = json!("r");
            }
            let error = prepare_many_and_validate(tool, json!({"queries":[query.clone()]}))
                .expect_err("reversed source range");
            assert_eq!(error.issues.len(), 1, "{tool}: {query}: {error:?}");
            assert_eq!(
                error.issues[0].path,
                ["queries", "0", "ranges", "0", "endLine"]
            );
            assert_eq!(error.issues[0].rule_id, "schema.order");
            let rendered = crate::contracts::format_input_error(tool, &error);
            let message = rendered["error"].as_str().unwrap();
            assert!(
                message.contains("startLine") && message.contains("endLine"),
                "{message}"
            );
            assert!(!message.contains("not allowed"), "{message}");

            query["ranges"][0]["endLine"] = json!(12);
            prepare_many_and_validate(tool, json!({"queries":[query]}))
                .expect("repairing the bound preserves the read form");
        }
    }
}
