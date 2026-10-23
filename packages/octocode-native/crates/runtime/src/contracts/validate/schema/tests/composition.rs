use super::*;

#[test]
fn nested_refinement_does_not_hide_the_question_discriminator() {
    let schema = serde_json::json!({"anyOf":[
        {"type":"object","properties":{"type":{"const":"noul"}},
         "anyOf":[{"required":["instructions"]},{"required":["criteria"]}]},
        {"type":"object","properties":{"type":{"const":"choice"}},"required":["criteria"]}
    ]});
    let mut value = serde_json::json!({"type":"choice"});
    let error = validate_schema(&schema, &schema, &mut value, &mut vec![]).unwrap_err();
    assert_eq!(error.issues.len(), 1, "{error:?}");
    assert_eq!(error.issues[0].path, vec!["criteria"]);
}

#[test]
fn union_guards_preserve_enclosing_required_fields_and_property_validation() {
    let schema = json!({
        "type":"object", "additionalProperties":false,
        "properties":{
            "state":{"type":["string","object","array"]},
            "source":{"type":"object"},
            "questions":{"type":"array","minItems":1}
        },
        "required":["questions"],
        "oneOf":[{"required":["state"]},{"required":["source"]}]
    });
    for mut value in [
        json!({"state":"text","questions":[{}]}),
        json!({"state":[],"questions":[{}]}),
        json!({"state":{},"questions":[{}]}),
        json!({"source":{},"questions":[{}]}),
    ] {
        assert!(
            validate_schema(&schema, &schema, &mut value, &mut vec![]).is_ok(),
            "{value}"
        );
    }
    for mut value in [
        json!({"state":"text"}),
        json!({"state":"text","questions":[]}),
        json!({"state":"text","questions":[{}],"unknown":true}),
        json!({"state":"text","source":{},"questions":[{}]}),
        json!({"questions":[{}]}),
        json!({"state":1,"questions":[{}]}),
        json!({"state":"text","questions":"encoded"}),
    ] {
        assert!(
            validate_schema(&schema, &schema, &mut value, &mut vec![]).is_err(),
            "{value}"
        );
    }
}

#[test]
fn successful_union_branch_defaults_reach_sibling_constraints() {
    let schema = json!({
        "type":"object", "required":["mode"],
        "anyOf":[{"type":"object","properties":{"mode":{"type":"string","default":"read"}}}],
        "properties":{"mode":{"const":"read"}}, "additionalProperties":false
    });
    let mut value = json!({});
    validate_schema(&schema, &schema, &mut value, &mut vec![]).unwrap();
    assert_eq!(value, json!({"mode":"read"}));
    assert!(validate_schema(&schema, &schema, &mut json!({"mode":"write"}), &mut vec![]).is_err());
}

#[test]
fn reference_chains_validate_sibling_constraints_at_every_level() {
    let schema = json!({
        "$defs":{
            "base":{"type":"integer"},
            "bounded":{"$ref":"#/$defs/base","minimum":2}
        },
        "$ref":"#/$defs/bounded", "maximum":3
    });
    for (mut value, valid) in [
        (json!(1), false),
        (json!(2), true),
        (json!(3), true),
        (json!(4), false),
        (json!(2.5), false),
        (json!("2"), false),
    ] {
        assert_eq!(
            validate_schema(&schema, &schema, &mut value, &mut vec![]).is_ok(),
            valid,
            "{value}"
        );
    }
    let cyclic =
        json!({"$defs":{"a":{"$ref":"#/$defs/b"},"b":{"$ref":"#/$defs/a"}},"$ref":"#/$defs/a"});
    assert_eq!(
        validate_schema(&cyclic, &cyclic, &mut json!(1), &mut vec![])
            .unwrap_err()
            .issues[0]
            .rule_id,
        "schema.unsupported-ref"
    );
}

#[test]
fn adjacent_unions_and_literal_constraints_all_apply() {
    let schema =
        json!({"oneOf":[{"const":"a"},{"const":"b"}],"anyOf":[{"const":"a"},{"const":"c"}]});
    assert!(validate_schema(&schema, &schema, &mut json!("a"), &mut vec![]).is_ok());
    for mut value in [json!("b"), json!("c")] {
        assert!(validate_schema(&schema, &schema, &mut value, &mut vec![]).is_err());
    }
    let schema = json!({"anyOf":[{"type":"string"}],"const":"a"});
    assert!(validate_schema(&schema, &schema, &mut json!("b"), &mut vec![]).is_err());
}

#[test]
fn type_arrays_enforce_members_and_only_applicable_bounds() {
    let schema = json!({"type":["string","array","object","null"],"minLength":2,"maxItems":1,"minProperties":1});
    for (mut value, valid) in [
        (json!("ab"), true),
        (json!("a"), false),
        (json!([]), true),
        (json!([1, 2]), false),
        (json!({"a":1,"b":2}), true),
        (json!({}), false),
        (json!(null), true),
        (json!(1), false),
        (json!(false), false),
    ] {
        assert_eq!(
            validate_schema(&schema, &schema, &mut value, &mut vec![]).is_ok(),
            valid,
            "{value}"
        );
    }
    for (schema, mut value, valid) in [
        (
            json!({"type":["integer","null"],"minimum":1}),
            json!(1),
            true,
        ),
        (json!({"type":["integer","null"]}), json!(1.5), false),
        (json!({"type":["integer","number"]}), json!(1.5), true),
        (json!({"type":["boolean","null"]}), json!(false), true),
        (json!({"type":["string","unsupported"]}), json!("a"), false),
        (json!({"type":["string",1]}), json!("a"), false),
        (json!({"type":[]}), json!("a"), false),
    ] {
        assert_eq!(
            validate_schema(&schema, &schema, &mut value, &mut vec![]).is_ok(),
            valid,
            "{schema}: {value}"
        );
    }
}
