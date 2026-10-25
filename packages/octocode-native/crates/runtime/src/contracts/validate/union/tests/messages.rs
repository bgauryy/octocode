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
fn an_explicit_selector_keeps_its_branch_over_a_closer_sibling_shape() {
    let point = json!({"type":"object","additionalProperties":false,
        "properties":{"format":{"const":"point"}},"required":["format"]});
    let walk = json!({"type":"object","additionalProperties":false,
        "properties":{"format":{"const":"walk"},"depth":{"type":"integer"}},
        "required":["format","depth"]});
    for branches in [vec![walk.clone(), point.clone()], vec![point, walk]] {
        let schema = json!({"anyOf":branches});
        let mut value = json!({"format":"point","depth":2});
        let before = value.clone();
        let error = validate_schema(&schema, &schema, &mut value, &mut vec![])
            .expect_err("a point has no walk depth");
        assert_eq!(value, before);
        assert_eq!(error.issues.len(), 1, "{error:?}");
        assert_eq!(error.issues[0].path, ["depth"]);
        assert_eq!(error.issues[0].rule_id, "schema.unknown-field");
        let message = crate::contracts::format_input_error("example", &error).to_string();
        assert!(message.contains("walk"), "{message}");
        value["format"] = json!("walk");
        validate_schema(&schema, &schema, &mut value, &mut vec![])
            .expect("the stated format admits its field");
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

#[test]
fn a_declared_field_with_wrong_type_is_not_diagnosed_as_an_unknown_field() {
    let bare = json!({"type":"object","additionalProperties":false,
        "properties":{"name":{"type":"string"}},"required":["name"]});
    let selected = json!({"type":"object","additionalProperties":false,
        "properties":{"name":{"type":"string"},"patterns":{"type":"array","items":{"type":"string"}}},
        "required":["name","patterns"]});
    for branches in [vec![bare.clone(), selected.clone()], vec![selected, bare]] {
        let schema = json!({"anyOf":branches});
        let mut value = json!({"name":"source","patterns":"needle"});
        let before = value.clone();
        let error = validate_schema(&schema, &schema, &mut value, &mut vec![])
            .expect_err("a scalar is not an array");
        assert_eq!(value, before);
        assert_eq!(error.issues.len(), 1, "{error:?}");
        assert_eq!(error.issues[0].path, ["patterns"]);
        assert_eq!(error.issues[0].rule_id, "schema.type");
        value["patterns"] = json!(["needle"]);
        validate_schema(&schema, &schema, &mut value, &mut vec![])
            .expect("repairing the type preserves the selected form");
        value["unknown"] = json!(true);
        let error = validate_schema(&schema, &schema, &mut value, &mut vec![])
            .expect_err("unknown fields remain rejected");
        assert_eq!(error.issues.len(), 1, "{error:?}");
        assert_eq!(error.issues[0].path, ["unknown"]);
        assert_eq!(error.issues[0].rule_id, "schema.unknown-field");
    }
}

#[test]
fn reader_matching_forms_report_the_invalid_field_types() {
    for tool in ["localFetch", "ghGetFileContent"] {
        for (patterns, context, fields) in [
            (
                json!("needle"),
                json!({"before":8,"after":8}),
                vec!["matchString"],
            ),
            (json!(["needle"]), json!(8), vec!["contextLines"]),
            (
                json!("needle"),
                json!(8),
                vec!["matchString", "contextLines"],
            ),
        ] {
            let mut query =
                json!({"path":"source.rs","matchString":patterns,"contextLines":context});
            if tool == "ghGetFileContent" {
                query["owner"] = json!("o");
                query["repo"] = json!("r");
            }
            let error = prepare_many_and_validate(tool, json!({"queries":[query.clone()]}))
                .expect_err("invalid field types");
            assert_eq!(error.issues.len(), fields.len(), "{tool}: {error:?}");
            for field in fields {
                assert!(
                    error.issues.iter().any(|issue| {
                        issue.path == ["queries", "0", field] && issue.rule_id == "schema.type"
                    }),
                    "{tool}: {field}: {error:?}"
                );
            }
            let rendered = crate::contracts::format_input_error(tool, &error).to_string();
            assert!(!rendered.contains("Unknown field"), "{tool}: {rendered}");
            query["matchString"] = json!(["needle"]);
            query["contextLines"] = json!({"before":8,"after":8});
            prepare_many_and_validate(tool, json!({"queries":[query]}))
                .expect("correcting only the field types admits the matching form");
        }
    }
}
