use super::*;
use octocode_test_support::json_fixture;

#[test]
fn intersections_keep_defaults_and_reject_every_violated_constraint() {
    let schema = json_fixture(
        r#"{
            "allOf": [
                {
                    "type": "object",
                    "properties": {"count": {"type": "integer", "default": 3}},
                    "required": ["count"]
                },
                {"properties": {"count": {"minimum": 2, "maximum": 4}}}
            ]
        }"#,
    );
    let mut value = json!({});
    validate_schema(&schema, &schema, &mut value, &mut vec![]).unwrap();
    assert_eq!(value["count"], 3);
    for invalid in [json!(1), json!(5), json!("3")] {
        assert!(
            validate_schema(&schema, &schema, &mut json!({"count":invalid}), &mut vec![]).is_err()
        );
    }
}

#[test]
fn ordered_bounds_use_inclusive_values_and_absolute_instants() {
    for (kind, pairs) in [
        (
            "number",
            vec![(json!(2), json!(2), true), (json!(3), json!(2), false)],
        ),
        (
            "timestamp",
            vec![
                (
                    json!("2026-10-24T12:00:00+02:00"),
                    json!("2026-10-24T10:00:00Z"),
                    true,
                ),
                (
                    json!("2026-10-24T12:00:01+02:00"),
                    json!("2026-10-24T10:00:00Z"),
                    false,
                ),
            ],
        ),
    ] {
        let schema = json!({"type":"object","x-order":{"lower":"from","upper":"to","kind":kind}});
        for (from, to, valid) in pairs {
            assert_eq!(
                validate_schema(
                    &schema,
                    &schema,
                    &mut json!({"from":from,"to":to}),
                    &mut vec![]
                )
                .is_ok(),
                valid
            );
        }
        assert!(validate_schema(&schema, &schema, &mut json!({}), &mut vec![]).is_ok());
    }
}

#[test]
fn nested_refinement_does_not_hide_the_question_discriminator() {
    let schema = json_fixture(
        r#"{
            "anyOf": [
                {
                    "type": "object",
                    "properties": {"type": {"const": "noul"}},
                    "anyOf": [{"required": ["instructions"]}, {"required": ["criteria"]}]
                },
                {
                    "type": "object",
                    "properties": {"type": {"const": "choice"}},
                    "required": ["criteria"]
                }
            ]
        }"#,
    );
    let mut value = json_fixture(r#"{"type": "choice"}"#);
    let error = validate_schema(&schema, &schema, &mut value, &mut vec![]).unwrap_err();
    assert_eq!(error.issues.len(), 1, "{error:?}");
    assert_eq!(error.issues[0].path, vec!["criteria"]);
}

#[test]
fn union_guards_preserve_enclosing_required_fields_and_property_validation() {
    let schema = json_fixture(
        r#"{
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "state": {"type": ["string", "object", "array"]},
                "source": {"type": "object"},
                "questions": {"type": "array", "minItems": 1}
            },
            "required": ["questions"],
            "oneOf": [{"required": ["state"]}, {"required": ["source"]}]
        }"#,
    );
    for mut value in [
        json_fixture(r#"{"state": "text", "questions": [{}]}"#),
        json_fixture(r#"{"state": [], "questions": [{}]}"#),
        json_fixture(r#"{"state": {}, "questions": [{}]}"#),
        json_fixture(r#"{"source": {}, "questions": [{}]}"#),
    ] {
        assert!(
            validate_schema(&schema, &schema, &mut value, &mut vec![]).is_ok(),
            "{value}"
        );
    }
    for mut value in [
        json_fixture(r#"{"state": "text"}"#),
        json_fixture(r#"{"state": "text", "questions": []}"#),
        json_fixture(r#"{"state": "text", "questions": [{}], "unknown": true}"#),
        json_fixture(r#"{"state": "text", "source": {}, "questions": [{}]}"#),
        json_fixture(r#"{"questions": [{}]}"#),
        json_fixture(r#"{"state": 1, "questions": [{}]}"#),
        json_fixture(r#"{"state": "text", "questions": "encoded"}"#),
    ] {
        assert!(
            validate_schema(&schema, &schema, &mut value, &mut vec![]).is_err(),
            "{value}"
        );
    }
}

#[test]
fn successful_union_branch_defaults_reach_sibling_constraints() {
    let schema = json_fixture(
        r#"{
            "type": "object",
            "required": ["mode"],
            "anyOf": [
                {
                    "type": "object",
                    "properties": {"mode": {"type": "string", "default": "read"}}
                }
            ],
            "properties": {"mode": {"const": "read"}},
            "additionalProperties": false
        }"#,
    );
    let mut value = json!({});
    validate_schema(&schema, &schema, &mut value, &mut vec![]).unwrap();
    assert_eq!(value, json_fixture(r#"{"mode": "read"}"#));
    assert!(
        validate_schema(
            &schema,
            &schema,
            &mut json_fixture(r#"{"mode": "write"}"#),
            &mut vec![]
        )
        .is_err()
    );
}

#[test]
fn reference_chains_validate_sibling_constraints_at_every_level() {
    let schema = json_fixture(
        r##"{
            "$defs": {
                "base": {"type": "integer"},
                "bounded": {"$ref": "#/$defs/base", "minimum": 2}
            },
            "$ref": "#/$defs/bounded",
            "maximum": 3
        }"##,
    );
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
    let cyclic = json_fixture(
        r##"{
            "$defs": {"a": {"$ref": "#/$defs/b"}, "b": {"$ref": "#/$defs/a"}},
            "$ref": "#/$defs/a"
        }"##,
    );
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
    let schema = json_fixture(
        r#"{
            "oneOf": [{"const": "a"}, {"const": "b"}],
            "anyOf": [{"const": "a"}, {"const": "c"}]
        }"#,
    );
    assert!(validate_schema(&schema, &schema, &mut json!("a"), &mut vec![]).is_ok());
    for mut value in [json!("b"), json!("c")] {
        assert!(validate_schema(&schema, &schema, &mut value, &mut vec![]).is_err());
    }
    let schema = json_fixture(r#"{"anyOf": [{"type": "string"}], "const": "a"}"#);
    assert!(validate_schema(&schema, &schema, &mut json!("b"), &mut vec![]).is_err());
}

#[test]
fn type_arrays_enforce_members_and_only_applicable_bounds() {
    let schema = json_fixture(
        r#"{
            "type": ["string", "array", "object", "null"],
            "minLength": 2,
            "maxItems": 1,
            "minProperties": 1
        }"#,
    );
    for (mut value, valid) in [
        (json!("ab"), true),
        (json!("a"), false),
        (json!([]), true),
        (json_fixture(r#"[1, 2]"#), false),
        (json_fixture(r#"{"a": 1, "b": 2}"#), true),
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
            json_fixture(r#"{"type": ["integer", "null"], "minimum": 1}"#),
            json!(1),
            true,
        ),
        (
            json_fixture(r#"{"type": ["integer", "null"]}"#),
            json!(1.5),
            false,
        ),
        (
            json_fixture(r#"{"type": ["integer", "number"]}"#),
            json!(1.5),
            true,
        ),
        (
            json_fixture(r#"{"type": ["boolean", "null"]}"#),
            json!(false),
            true,
        ),
        (
            json_fixture(r#"{"type": ["string", "unsupported"]}"#),
            json!("a"),
            false,
        ),
        (
            json_fixture(r#"{"type": ["string", 1]}"#),
            json!("a"),
            false,
        ),
        (json_fixture(r#"{"type": []}"#), json!("a"), false),
    ] {
        assert_eq!(
            validate_schema(&schema, &schema, &mut value, &mut vec![]).is_ok(),
            valid,
            "{schema}: {value}"
        );
    }
}

#[test]
fn reference_sibling_defaults_override_definition_defaults() {
    let schema = json_fixture(
        r##"{
            "type": "object",
            "properties": {
                "local": {"$ref": "#/$defs/flag", "default": true},
                "shared": {"$ref": "#/$defs/flag"}
            },
            "$defs": {"flag": {"type": "boolean", "default": false}}
        }"##,
    );
    let mut value = json!({});
    validate_schema(&schema, &schema, &mut value, &mut vec![]).expect("valid");
    assert_eq!(value, json_fixture(r#"{"local": true, "shared": false}"#));
}

/// A field's authored guidance (`x-checks`) wins over the generic issue at
/// that field: each one-level shape of a score scale reads the guidance, not
/// the bare minimum-length rule, and a valid scale passes.
#[test]
fn a_fields_own_guidance_wins_over_its_generic_issue() {
    let guidance = "score criteria is an array of at least two levels";
    let schema = json!({
        "type":"object",
        "properties":{"questions":{"type":"object","additionalProperties":{
            "type":"object",
            "properties":{"criteria":{"type":"array","items":{"type":"string"},"minItems":2}}
        }}},
        "x-checks":[{
            "id":"probe.score-levels", "each":"questions", "message":guidance,
            "when":{"type":"object","properties":{"criteria":{"type":"array","maxItems":1}},"required":["criteria"]},
            "assert":{"not":{}}
        }]
    });
    for criteria in [
        json_fixture(r#"["only one"]"#),
        json!([]),
        json_fixture(r#"["low"]"#),
    ] {
        let mut value = json!({"questions":{"q":{"criteria":criteria}}});
        let error = validate_schema(&schema, &schema, &mut value, &mut vec![])
            .expect_err("one level is refused");
        let messages: Vec<_> = error
            .issues
            .iter()
            .map(|issue| issue.message.as_str())
            .collect();
        assert_eq!(messages, [guidance], "{criteria}");
        assert_eq!(error.issues[0].path, ["questions", "q"], "{criteria}");
    }
    let mut valid = json_fixture(r#"{"questions": {"q": {"criteria": ["low", "high"]}}}"#);
    validate_schema(&schema, &schema, &mut valid, &mut vec![]).expect("two levels pass");
}

/// Authored guidance explains a value of the shape its schema declares. A
/// value of another type, or an unknown key, is not the guidance's to
/// explain: the type and the unknown field are named, since the check's
/// assertion fails on them for a reason its message does not state.
#[test]
fn guidance_never_replaces_a_wrong_type_or_an_unknown_field() {
    let guidance = "set first or second";
    let schema = json!({
        "type":"object",
        "properties":{"choice":{"$ref":"#/$defs/choice"}},
        "$defs":{"choice":{
            "type":"object",
            "properties":{"first":{"type":"string"},"second":{"type":"string"}},
            "additionalProperties":false,
            "x-checks":[{
                "id":"probe.choice", "message":guidance,
                "assert":{"anyOf":[
                    {"type":"object","required":["first"]},
                    {"type":"object","required":["second"]}
                ]}
            }]
        }}
    });
    let issues = |choice: serde_json::Value| {
        let mut value = json!({"choice":choice});
        validate_schema(&schema, &schema, &mut value, &mut vec![])
            .expect_err("refused")
            .issues
            .into_iter()
            .map(|issue| (issue.rule_id, issue.path.join(".")))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        issues(json!("first")),
        [("schema.type".to_owned(), "choice".to_owned())]
    );
    assert_eq!(
        issues(json_fixture(r#"{"third": "x"}"#)),
        [("schema.unknown-field".to_owned(), "choice.third".to_owned())]
    );
    assert_eq!(
        issues(json_fixture(r#"{"first": "x", "third": "y"}"#)),
        [("schema.unknown-field".to_owned(), "choice.third".to_owned())]
    );
    // A value of the declared shape still reads the guidance.
    assert_eq!(
        issues(json!({})),
        [("probe.choice".to_owned(), "choice".to_owned())]
    );
}
