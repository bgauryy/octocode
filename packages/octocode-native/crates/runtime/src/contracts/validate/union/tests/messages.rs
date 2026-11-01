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

#[test]
fn a_request_field_inside_a_query_names_where_it_goes_and_nothing_else() {
    for (input, field, repair) in [
        (
            json!({"queries":[{"cursor":"cursor2.abc"}]}),
            "cursor",
            "alone, as next.input gives it, without queries",
        ),
        (
            json!({"queries":[{"path":"src","matchString":["x"],"responseChars":900}]}),
            "responseChars",
            "beside queries",
        ),
    ] {
        let error = prepare_many_and_validate("localSearch", input).expect_err(field);
        let shown = crate::contracts::validate::messages::format_input_error("localSearch", &error);
        let text = shown["error"].as_str().unwrap_or_default();
        assert!(text.contains(field) && text.contains(repair), "{shown}");
        // The row's missing fields follow from the misplacement: not shown.
        assert!(shown.get("details").is_none(), "{shown}");
    }
}

/// A missing selector names the value the sent fields need, from the
/// contract's branches: `nameRegex` fits only the `files` operation of
/// structureSearch, so the error says so instead of a bare missing field.
#[test]
fn a_missing_selector_names_the_value_the_sent_fields_require() {
    let error = prepare_many_and_validate(
        "structureSearch",
        json!({"queries":[{"path":"src","nameRegex":"zzqq"}]}),
    )
    .expect_err("missing operation");
    let message = crate::contracts::validate::format_input_error("structureSearch", &error);
    assert!(
        message["error"]
            .as_str()
            .is_some_and(|error| error.contains("(nameRegex requires operation:\"files\")")),
        "{message}"
    );
}

/// A row without its selector matched no form, so an unknown field's
/// repair is not one guessed form's field list: a field another row field
/// declares (astSearch `rule.pattern`) is named with its owner, and any
/// other unknown field lists the fields of every form.
#[test]
fn an_unknown_field_in_a_row_without_its_selector_names_every_form() {
    let details = |tool: &str, row: serde_json::Value| {
        let error = prepare_many_and_validate(tool, json!({"queries":[row]}))
            .expect_err("missing operation");
        crate::contracts::validate::format_input_error(tool, &error).to_string()
    };
    let nested = details("astSearch", json!({"path":"src","pattern":"fn $F()"}));
    assert!(nested.contains("'pattern' is a field of rule"), "{nested}");
    let unknown = details("astSearch", json!({"path":"src","zzqqField":1}));
    for field in ["rule", "symbolFilter", "line"] {
        assert!(unknown.contains(field), "{field} missing from {unknown}");
    }
}

/// A sibling form that accepts the rejected value names its selectors only
/// when its other selectors accept the row: graphQuery `direction:"both"`
/// fits traversals, never `hubs`, so no `view` change can make it valid.
#[test]
fn a_sibling_hint_skips_forms_whose_other_selectors_reject_the_row() {
    let error = prepare_many_and_validate(
        "graphQuery",
        json!({"queries":[{"operation":"hubs","path":".","direction":"both"}]}),
    )
    .expect_err("hubs rejects both");
    let message = crate::contracts::validate::format_input_error("graphQuery", &error).to_string();
    assert!(message.contains("direction"), "{message}");
    assert!(!message.contains("requires"), "{message}");
}

/// A value only a sibling form accepts names the selector the row omitted
/// that picks that form: `unique:"list"` needs `view:"matchOnly"`.
#[test]
fn a_sibling_hint_names_the_omitted_selector_that_accepts_the_value() {
    let schema = json!({"anyOf":[
        {"type":"object","additionalProperties":false,
         "properties":{"view":{"const":"content"},"unique":{"enum":["off"]}}},
        {"type":"object","additionalProperties":false,
         "properties":{"view":{"const":"matchOnly"},"unique":{"enum":["off","list"]}},
         "required":["view"]}
    ]});
    let mut value = json!({"unique":"list"});
    let error = validate_schema(&schema, &schema, &mut value, &mut vec![])
        .expect_err("list needs matchOnly");
    assert!(
        error
            .issues
            .iter()
            .any(|item| item.message.contains(r#"(unique:"list" requires view:"matchOnly")"#)),
        "{error:?}"
    );
}
