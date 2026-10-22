use super::schema::validate_schema;
use crate::contracts::prepare_many_and_validate;
use serde_json::{Value, json};

#[test]
fn integral_numeric_lexemes_reach_generated_query_types() {
    for (tool, raw) in [
        ("localFetch", r#"{"path":"a.ts","length":1.0}"#),
        ("localFetch", r#"{"path":"a.ts","length":1e0}"#),
        ("localFetch", r#"{"path":"a.ts","length":1E+0}"#),
        ("localFetch", r#"{"path":"a.ts","offset":-0.0}"#),
        ("localFetch", r#"{"path":"a.ts","offset":0.0}"#),
        (
            "localFetch",
            r#"{"path":"a.ts","offset":9.007199254740991e15}"#,
        ),
        ("structureSearch", r#"{"path":"src","page":1.0}"#),
        (
            "astSearch",
            r#"{"path":"a.ts","operation":"match","pattern":"target($A)","matchPageSize":1.0}"#,
        ),
        (
            "lspSearch",
            r#"{"path":"a.ts","symbolName":"target","line":2.0}"#,
        ),
    ] {
        let query: Value = serde_json::from_str(raw).expect("raw numeric JSON");
        let mut rows = prepare_many_and_validate(tool, json!({"queries":[query]}))
            .expect("JSON Schema accepts mathematically integral numbers");
        let row = rows.pop().expect("one query");
        let decoded = match tool {
            "localFetch" => {
                serde_json::from_value::<octocode_contract_types::LocalFetchQuery>(row).map(|_| ())
            }
            "structureSearch" => {
                serde_json::from_value::<octocode_contract_types::StructureSearchQuery>(row)
                    .map(|_| ())
            }
            "astSearch" => {
                serde_json::from_value::<octocode_contract_types::AstSearchQuery>(row).map(|_| ())
            }
            "lspSearch" => {
                serde_json::from_value::<octocode_contract_types::LspSearchQuery>(row).map(|_| ())
            }
            _ => unreachable!(),
        };
        assert!(decoded.is_ok(), "{tool} {raw}: {decoded:?}");
    }
}

#[test]
fn fractional_string_and_out_of_range_numbers_stay_rejected() {
    for raw in [
        r#"{"path":"a.ts","length":1.25}"#,
        r#"{"path":"a.ts","offset":-0.25}"#,
        r#"{"path":"a.ts","length":"1"}"#,
        r#"{"path":"a.ts","length":"1e0"}"#,
        r#"{"path":"a.ts","length":0.0}"#,
        r#"{"path":"a.ts","length":50001.0}"#,
        r#"{"path":"a.ts","offset":9.007199254740992e15}"#,
        r#"{"path":"a.ts","offset":1e80}"#,
    ] {
        let query: Value = serde_json::from_str(raw).expect("numeric JSON");
        assert!(
            prepare_many_and_validate("localFetch", json!({"queries":[query]})).is_err(),
            "{raw}"
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
        json!({"queries":[{"state":state,"questions":{"check":{
            "type":"noul","instructions":"The state contains numeric evidence."
        }}}]}),
    )
    .expect("untyped state is accepted");
    assert_eq!(rows[0]["state"].to_string(), before);
}

#[test]
fn output_validation_keeps_numeric_evidence_and_continuations_byte_identical() {
    let mut output: Value = serde_json::from_str(
        r#"{"results":[{"index":0,"data":{"path":"a.ts","content":"x","totalLines":1.0,"next":{"nextPage":{"tool":"localFetch","query":{"queries":[{"path":"a.ts","length":1.0,"offset":0.0}]}}}}}]}"#,
    )
    .expect("serialized response");
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
