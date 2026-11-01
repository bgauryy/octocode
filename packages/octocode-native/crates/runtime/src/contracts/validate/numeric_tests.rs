use super::schema::validate_schema;
use crate::contracts::prepare_many_and_validate;
use serde_json::{Value, json};

#[test]
fn integral_json_numbers_decode_through_canonical_generated_types() {
    for lexeme in ["1.0", "1e0", "1E+0"] {
        let input: Value = serde_json::from_str(&format!(r#"{{"queries":[{{"path":"a.rs","ranges":[{{"startLine":{lexeme},"endLine":{lexeme}}}]}}]}}"#)).unwrap();
        let rows = prepare_many_and_validate("localFetch", input).unwrap();
        let parsed: octocode_contract_types::LocalFetchQuery =
            serde_json::from_value(rows[0].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap()["ranges"][0]["startLine"],
            1
        );
    }
    for value in [json!(1.25), json!("1"), json!(0), json!(-1)] {
        assert!(
            prepare_many_and_validate(
                "localFetch",
                json!({"queries":[{"path":"a.rs","ranges":[{"startLine":value,"endLine":2}]}]})
            )
            .is_err()
        );
    }
}

#[test]
fn untyped_input_state_keeps_its_float_representation() {
    let state: Value = serde_json::from_str(
        r#"{"integral":1.0,"fraction":0.25,"negativeZero":-0.0,"nested":[2.0,{"count":3.0}]}"#,
    )
    .expect("float state");
    let before = state.to_string();
    let rows = prepare_many_and_validate(
        "clasify",
        json!({"queries":[{"operation":"evaluate","content":state,"questions":{"check":{
            "type":"noul","instructions":"The state contains numeric evidence."
        }}}]}),
    )
    .expect("untyped state is accepted");
    assert_eq!(rows[0]["content"].to_string(), before);
}

#[test]
fn output_validation_keeps_numeric_evidence_and_continuations_byte_identical() {
    let mut output = json!({"results":[{"index":0,"data":{
        "path":"a.ts","content":"x","totalLines":1.0,
        "next":{"nextPage":{"kind":"continue","tool":"localFetch","input":{"cursor":"issued"}}}
    }}]});
    let before = serde_json::to_vec(&output).expect("response bytes");
    super::validate_output_in_place("localFetch", &mut output).expect("valid output");
    assert_eq!(serde_json::to_vec(&output).expect("response bytes"), before);
    let schema = json!({"type":"object","properties":{"count":{"type":"integer"},"ratio":{"type":"number"}}});
    let mut value = json!({"count":1.0,"ratio":0.25});
    let before = value.to_string();
    validate_schema(&schema, &schema, &mut value, &mut Vec::new()).expect("read-only validation");
    assert_eq!(value.to_string(), before);
}

#[test]
fn input_mode_only_normalizes_declared_integer_positions() {
    let schema = json!({
        "type":"object",
        "$defs":{"count":{"type":"integer","minimum":-9007199254740991_i64}},
        "properties":{
            "count":{"$ref":"#/$defs/count"},
            "nullable":{"type":["integer","null"]},
            "ratio":{"type":"number"},
            "opaque":{}
        }
    });
    let mut value: Value = serde_json::from_str(
        r#"{"count":-9.007199254740991e15,"nullable":0.0,"ratio":1.0,"opaque":{"count":2.0}}"#,
    )
    .expect("mixed numbers");
    super::schema::validate_input_schema(&schema, &schema, &mut value, &mut Vec::new())
        .expect("valid input");
    assert_eq!(value["count"].as_i64(), Some(-9007199254740991));
    assert_eq!(value["nullable"].as_u64(), Some(0));
    assert!(value["ratio"].is_f64());
    assert!(value["opaque"]["count"].is_f64());
}

#[test]
fn integer_conversion_never_saturates_at_machine_boundaries() {
    let schema = json!({"type":"integer"});
    let unsigned_limit = u64::MAX as f64;
    let signed_limit = i64::MIN as f64;
    for number in [unsigned_limit, signed_limit - 2048.0, 1e80, -1e80] {
        let mut value = Value::from(number);
        let before = value.to_string();
        super::schema::validate_input_schema(&schema, &schema, &mut value, &mut Vec::new())
            .expect("an unbounded schema may accept an unrepresentable integer");
        assert!(value.is_f64(), "{number}");
        assert_eq!(value.to_string(), before);
    }
    for (number, expected) in [
        (unsigned_limit - 2048.0, Value::from(u64::MAX - 2047)),
        (signed_limit, Value::from(i64::MIN)),
    ] {
        let mut value = Value::from(number);
        super::schema::validate_input_schema(&schema, &schema, &mut value, &mut Vec::new())
            .expect("machine-representable integer");
        assert!(!value.is_f64());
        assert_eq!(value, expected);
    }
}

#[test]
fn integer_enum_and_constant_compare_the_normalized_input() {
    let schema = json!({"type":"integer","enum":[1,2],"const":1});
    let mut value = json!(1.0);
    super::schema::validate_input_schema(&schema, &schema, &mut value, &mut Vec::new())
        .expect("1.0 is the declared integer constant");
    assert_eq!(value.as_u64(), Some(1));
}

#[test]
fn numeric_literals_and_nested_enum_values_use_json_schema_equality_without_rewriting_output() {
    for (literal, actual, accepted) in [
        (json!(0), json!(0.0), true),
        (json!(0), json!(-0.0), true),
        (json!(3), json!(3.0), true),
        (json!(3.0), json!(3), true),
        (json!(3), json!(3.5), false),
        (json!(3), json!("3"), false),
        (
            json!(9007199254740993_u64),
            json!(9007199254740992.0),
            false,
        ),
        (json!(9007199254740992_u64), json!(9007199254740992.0), true),
        (json!(u64::MAX), json!(u64::MAX as f64), false),
        (json!(i64::MAX), json!(i64::MAX as f64), false),
        (json!(i64::MIN), json!(i64::MIN as f64), true),
        (
            json!({"rows":[0,{"n":3}]}),
            json!({"rows":[0.0,{"n":3.0}]}),
            true,
        ),
        (json!([0, 3]), json!([3.0, 0.0]), false),
    ] {
        for schema in [json!({"const":literal}), json!({"enum":[literal]})] {
            let mut value = actual.clone();
            let before = serde_json::to_vec(&value).unwrap();
            let result = validate_schema(&schema, &schema, &mut value, &mut Vec::new());
            assert_eq!(
                result.is_ok(),
                accepted,
                "{schema} versus {actual}: {result:?}"
            );
            assert_eq!(serde_json::to_vec(&value).unwrap(), before);
        }
    }
}

#[test]
fn numeric_union_selectors_share_literal_equality() {
    let schema = json!({"oneOf":[
        {"type":"object","properties":{"kind":{"const":0},"value":{"type":"string"}},"required":["kind","value"],"additionalProperties":false},
        {"type":"object","properties":{"kind":{"const":1},"count":{"type":"integer"}},"required":["kind","count"],"additionalProperties":false}
    ]});
    for (mut value, accepted) in [
        (json!({"kind":0.0,"value":"ok"}), true),
        (json!({"kind":1.0,"count":3}), true),
        (json!({"kind":0.0,"count":3}), false),
        (json!({"kind":0.25,"value":"no"}), false),
    ] {
        assert_eq!(
            validate_schema(&schema, &schema, &mut value, &mut Vec::new()).is_ok(),
            accepted,
            "{value}"
        );
    }
}

#[test]
fn integral_floats_convert_exactly_or_not_at_all() {
    use super::literals::integral_number;
    let float = |value: f64| serde_json::Number::from_f64(value).expect("finite");
    let two_63 = 9_223_372_036_854_775_808_f64;
    for (input, expected) in [
        (3.0, Some(serde_json::Number::from(3_u64))),
        (-0.0, Some(serde_json::Number::from(0_u64))),
        (
            9_007_199_254_740_993_f64,
            Some(serde_json::Number::from(9_007_199_254_740_992_u64)),
        ),
        (two_63, Some(serde_json::Number::from(1_u64 << 63))),
        (-two_63, Some(serde_json::Number::from(i64::MIN))),
        (-3.0, Some(serde_json::Number::from(-3_i64))),
        (18_446_744_073_709_551_616_f64, None),
        (-2.0 * two_63, None),
        (1.5, None),
    ] {
        assert_eq!(integral_number(&float(input)), expected, "{input}");
    }
}
