use super::schema::{validate_input_schema, validate_schema};
use serde_json::{Value, json};

fn accepts_in_both_directions(schema: &Value, value: &Value, expected: bool) {
    for input in [true, false] {
        let mut candidate = value.clone();
        let before = serde_json::to_vec(&candidate).expect("JSON bytes");
        let result = if input {
            validate_input_schema(schema, schema, &mut candidate, &mut Vec::new())
        } else {
            validate_schema(schema, schema, &mut candidate, &mut Vec::new())
        };
        assert_eq!(
            result.is_ok(),
            expected,
            "input={input}, {value}: {result:?}"
        );
        assert_eq!(serde_json::to_vec(&candidate).expect("JSON bytes"), before);
    }
}

#[test]
fn canonical_word_classes_are_ecmascript_in_both_directions() {
    let schema = json!({"type":"string","pattern":r"^[\w.][\w.-]*$"});
    for owner in ["owner", "Owner_12", "a.b-c", ".owner"] {
        accepts_in_both_directions(&schema, &json!(owner), true);
    }
    for owner in ["é", "owneré", "Ω", "中", "𝒜", "😀", "１２"] {
        accepts_in_both_directions(&schema, &json!(owner), false);
    }
}

#[test]
fn canonical_whitespace_classes_distinguish_nel_from_bom() {
    for (pattern, whitespace) in [(r"^\s$", true), (r"^\S$", false)] {
        let schema = json!({"type":"string","pattern":pattern});
        for (text, is_space) in [
            (" ", true),
            ("\t", true),
            ("\n", true),
            ("\r", true),
            ("\u{a0}", true),
            ("\u{feff}", true),
            ("\u{85}", false),
            ("\u{200b}", false),
            ("x", false),
        ] {
            accepts_in_both_directions(&schema, &json!(text), is_space == whitespace);
        }
    }
}

#[test]
fn property_names_obey_the_same_ecmascript_pattern_contract() {
    let schema = json!({
        "type":"object",
        "propertyNames":{"type":"string","pattern":r"^\w+$"},
        "additionalProperties":{"type":"string"}
    });
    accepts_in_both_directions(&schema, &json!({"owner_1":"value"}), true);
    for key in ["é", "Ω", "中", "𝒜", "１２"] {
        accepts_in_both_directions(&schema, &json!({key:"value"}), false);
    }
}

#[test]
fn continuation_queries_reject_non_ecmascript_owners_without_mutating_output() {
    for (owner, accepted) in [("valid_owner", true), ("owneré", false), ("中", false)] {
        let mut output = json!({"results":[{"index":0,"data":{
            "path":"a.ts", "content":"x", "totalLines":1,
            "related":[{"kind":"related","tool":"ghStructure","input":{"queries":[{"operation":"tree","owner":owner,"repo":"repo"}]}}]
        }}]});
        let before = serde_json::to_vec(&output).expect("wire bytes");
        let result = super::validate_output_in_place("localFetch", &mut output);
        assert_eq!(result.is_ok(), accepted, "{owner}: {result:?}");
        assert_eq!(serde_json::to_vec(&output).expect("wire bytes"), before);
    }
}

/// A short pattern is the expected form, named with its field: the field's
/// description is usage prose and stays out of the error. A pattern too long
/// to read gives the field's note of the accepted forms instead.
#[test]
fn a_pattern_error_names_the_field_and_its_form_not_its_usage_prose() {
    let message = |pattern: &str, note: &str| {
        let schema = json!({"type":"object","properties":{"owner":{
            "type":"string","pattern":pattern,"description":note}}});
        let mut value = json!({"owner":"a b"});
        let error = validate_input_schema(&schema, &schema, &mut value, &mut Vec::new())
            .expect_err("pattern");
        error.issues[0].message.clone()
    };
    assert_eq!(
        message(r"^[\w.][\w.-]*$", "User or org; alone: its public repos."),
        r#""a b" is not a valid owner: it must match ^[\w.][\w.-]*$"#
    );
    let long = format!("^(?:{})$", "[a-z]+|".repeat(10));
    assert_eq!(
        message(&long, r#"Forms: "10..50"."#),
        r#""a b" does not match the expected form: Forms: "10..50"."#
    );
}
