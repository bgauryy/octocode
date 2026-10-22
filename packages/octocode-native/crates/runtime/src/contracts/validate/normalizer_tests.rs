use super::apply_normalization_rules;
use crate::contracts::prepare_many_and_validate;
use serde_json::{Value, json};

fn rules() -> Value {
    json!([
        {"phase":"normalize","opcode":"trim_fields","args":{"fields":["brief","values[]"]}},
        {"phase":"normalize","opcode":"drop_blank_fields","args":{"fields":["brief"]}}
    ])
}

#[test]
fn declared_trim_matches_ecmascript_for_plain_and_array_strings() {
    let mut input = json!({"queries":[{
        "brief":"\u{feff}\u{a0} x\u{feff}\u{85}y \t",
        "values":["\u{feff}one\u{feff}","\u{85}two\u{85}","\u{a0}three\u{a0}"," \tfour\r\n",1,false,null],
        "unlisted":"\u{feff} preserved \u{85}"
    }]});
    apply_normalization_rules(&rules(), &mut input).expect("declared normalizers");
    assert_eq!(
        input["queries"][0],
        json!({
            "brief":"x\u{feff}\u{85}y",
            "values":["one","\u{85}two\u{85}","three","four",1,false,null],
            "unlisted":"\u{feff} preserved \u{85}"
        })
    );
}

#[test]
fn declared_blank_removal_uses_ecmascript_whitespace() {
    let rules = json!([{"phase":"normalize","opcode":"drop_blank_fields","args":{
        "fields":["bom","nel","nbsp","ascii","word","number"]
    }}]);
    let mut input = json!({"queries":[{
        "bom":"\u{feff}","nel":"\u{85}","nbsp":"\u{a0}","ascii":" \t\r\n",
        "word":" word ","number":1
    }]});
    apply_normalization_rules(&rules, &mut input).expect("declared blank removal");
    assert_eq!(
        input["queries"][0],
        json!({"nel":"\u{85}","word":" word ","number":1})
    );
}

#[test]
fn declared_normalization_is_idempotent_and_preserves_internal_whitespace() {
    let mut input = json!({"queries":[{
        "brief":"\u{feff}",
        "values":[" \talpha\u{feff} beta\u{85}gamma\r\n","\u{feff}","\u{85}"],
        "unlisted":{"text":"\u{feff}opaque\u{feff}","number":1.0}
    }]});
    let opaque = input["queries"][0]["unlisted"].to_string();
    apply_normalization_rules(&rules(), &mut input).expect("first normalization");
    assert!(input["queries"][0].get("brief").is_none());
    assert_eq!(
        input["queries"][0]["values"],
        json!(["alpha\u{feff} beta\u{85}gamma", "", "\u{85}"])
    );
    assert_eq!(input["queries"][0]["unlisted"].to_string(), opaque);
    let first = input.to_string();
    apply_normalization_rules(&rules(), &mut input).expect("second normalization");
    assert_eq!(input.to_string(), first);
}

#[test]
fn canonical_briefs_trim_bom_and_retain_nel_before_generated_decoding() {
    let rows = prepare_many_and_validate(
        "localFetch",
        json!({"queries":[{
            "path":"a.ts","mainGoal":"\u{feff}goal\u{feff}","reasoning":"\u{85}"
        }]}),
    )
    .expect("canonical typed input");
    assert_eq!(rows[0]["mainGoal"], "goal");
    assert_eq!(rows[0]["reasoning"], "\u{85}");
    serde_json::from_value::<octocode_contract_types::LocalFetchQuery>(rows[0].clone())
        .expect("generated ECMAScript constraints agree");
}
