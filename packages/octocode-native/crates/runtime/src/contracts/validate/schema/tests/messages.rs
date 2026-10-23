use super::*;

/// Every `pattern` the contract declares compiles with the engine the
/// validator runs. One that does not (a look-around, say) rejects every
/// value of its field as the caller's invalid input, so the field is
/// unusable; it is a contract defect and fails here instead.
#[test]
fn every_contract_pattern_compiles_with_the_validator_engine() {
    fn patterns<'a>(value: &'a Value, at: String, out: &mut Vec<(String, &'a str)>) {
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    if key == "pattern"
                        && let Some(pattern) = child.as_str()
                    {
                        out.push((at.clone(), pattern));
                    }
                    patterns(child, format!("{at}/{key}"), out);
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    patterns(child, format!("{at}/{index}"), out);
                }
            }
            _ => {}
        }
    }
    let contract: Value =
        serde_json::from_str(crate::contracts::contract_json()).expect("contract");
    let mut found = Vec::new();
    patterns(&contract, String::new(), &mut found);
    assert!(!found.is_empty(), "the contract declares patterns");
    let broken: Vec<_> = found
        .into_iter()
        .filter_map(|(at, pattern)| {
            super::super::compiled_pattern(pattern)
                .err()
                .map(|error| format!("{at}: {pattern}: {}", error.lines().last().unwrap_or("")))
        })
        .collect();
    assert!(
        broken.is_empty(),
        "uncompilable contract patterns:\n{}",
        broken.join("\n")
    );
}

#[test]
fn boolean_for_an_on_off_enum_names_the_intended_value() {
    let regex = json!({"type":"string","enum":["literal","rust","pcre2"]});
    let minify = json!({"type":"string","enum":["none","standard","symbols"]});
    let message = |schema: &Value, mut value: Value| {
        validate_schema(schema, schema, &mut value, &mut vec![])
            .unwrap_err()
            .issues[0]
            .message
            .clone()
    };
    assert!(message(&regex, json!(true)).ends_with("use \"rust\""));
    assert!(message(&regex, json!(false)).ends_with("use \"literal\""));
    assert!(message(&minify, json!(false)).ends_with("use \"none\""));
    assert!(!message(&regex, json!("x")).contains("booleans"));
}

#[test]
fn bounded_records_enforce_property_counts_and_values() {
    let schema = json!({"type":"object", "minProperties":1, "maxProperties":24,
            "propertyNames":{"type":"string", "pattern":"^[a-z][a-zA-Z0-9_]{0,39}$"},
            "additionalProperties":{"type":"string", "minLength":1, "maxLength":800}});
    for count in [0, 1, 24, 25] {
        let mut value: Value = (0..count)
            .map(|index| (format!("claim{index}"), json!("A bounded claim.")))
            .collect::<serde_json::Map<_, _>>()
            .into();
        let result = validate_schema(&schema, &schema, &mut value, &mut vec![]);
        assert_eq!(result.is_ok(), (1..=24).contains(&count));
        if let Err(error) = result {
            assert!(
                error
                    .issues
                    .iter()
                    .any(|issue| issue.rule_id == "schema.size")
            );
        }
    }
    for mut value in [
        json!({"Bad id":"claim"}),
        json!({"claim":""}),
        json!({"claim":1}),
    ] {
        assert!(validate_schema(&schema, &schema, &mut value, &mut vec![]).is_err());
    }
}

fn pattern_cached(pattern: &str) -> bool {
    super::super::pattern::pattern_cache()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains_key(pattern)
}

/// Contract patterns are a fixed set; compiling one per validated string
/// is a per-response cost, since every continuation is validated against
/// every tool's query schema.
#[test]
fn schema_patterns_compile_once_per_process() {
    let pattern = "^cache-probe-[a-z]{3}$";
    let schema = json!({"type":"string","pattern":pattern});
    for value in ["cache-probe-abc", "cache-probe-xyz", "nope"] {
        let _ = validate_schema(&schema, &schema, &mut json!(value), &mut vec![]);
    }
    assert!(pattern_cached(pattern));
    let mut bad = json!("cache-probe-ab");
    assert!(validate_schema(&schema, &schema, &mut bad, &mut vec![]).is_err());
}

/// A rejected line range names the value sent and the shape
/// once, whether it is the wrong text or not a string at all.
#[test]
fn rejected_line_ranges_quote_the_shape() {
    let schema = json!({"type":"array","items":{"type":"string","pattern":super::super::super::shape::LINE_RANGE_PATTERN}});
    let message = |mut value: Value| {
        let mut path = vec!["queries".to_owned(), "0".to_owned(), "ranges".to_owned()];
        validate_schema(&schema, &schema, &mut value, &mut path)
            .unwrap_err()
            .issues[0]
            .message
            .clone()
    };
    assert_eq!(
        message(json!(["[&quot;299-342&quot;]"])),
        r#""[&quot;299-342&quot;]" is not a line range; ranges: ["299-342"] (strings "start-end")"#
    );
    assert_eq!(
        message(json!([{"s":"12-47"}])),
        r#"{"s":"12-47"} is not a line range; ranges: ["299-342"] (strings "start-end")"#
    );
}

/// A line range a typo spells names that span alone: the shape of a
/// line range follows only a value that spells none.
#[test]
fn a_spelled_line_range_names_its_span_without_the_shape() {
    let schema = json!({"type":"array","items":{"type":"string","pattern":super::super::super::shape::LINE_RANGE_PATTERN}});
    let mut path = vec!["queries".to_owned(), "0".to_owned(), "ranges".to_owned()];
    let error = validate_schema(&schema, &schema, &mut json!(["40:50"]), &mut path).unwrap_err();
    assert_eq!(
        error.issues[0].message,
        r#""40:50" is not a line range (did you mean "40-50"?)"#
    );
}

/// A value of another type for an object names the object's shape: its
/// fields, or a record's key and value shape with pinned literals first.
#[test]
fn a_non_object_names_the_object_shape() {
    let message = |schema: &Value, mut value: Value| {
        validate_schema(schema, schema, &mut value, &mut vec!["size".into()])
            .unwrap_err()
            .issues[0]
            .message
            .clone()
    };
    let size = json!({"type":"object","additionalProperties":false,
            "properties":{"greater":{"type":"string"},"less":{"type":"string"}}});
    assert_eq!(
        message(&size, json!(">10k")),
        "Expected object {greater, less}"
    );
    let questions = json!({"type":"object","additionalProperties":{"anyOf":[
        {"type":"object","properties":{"type":{"const":"noul"},"instructions":{"type":"string"}}},
        {"type":"object","properties":{"type":{"const":"choice"},"criteria":{"type":"object"}}}
    ]}});
    assert_eq!(
        message(&questions, json!(["Does it add?"])),
        "Expected object {<name>: {type: noul|choice, instructions, criteria}}"
    );
    let any = json!({"type":"object"});
    assert_eq!(message(&any, json!(1)), "Expected object");
}

/// A bare number for a count range names both readings.
#[test]
fn a_number_for_a_count_range_names_both_readings() {
    let schema = json!({"type":"string","x-countRange":true});
    let error = validate_schema(
        &schema,
        &schema,
        &mut json!(1000),
        &mut vec!["stars".into()],
    )
    .unwrap_err();
    assert_eq!(
        error.issues[0].message,
        r#"Expected string; send ">=1000" for at least 1000, or "1000" for exactly 1000"#
    );
}

/// A branch pattern too long to read gives the note of the field whose
/// branches they are.
#[test]
fn an_unreadable_branch_pattern_gives_the_fields_note() {
    let pattern = format!("^(?:{})$", "[a-z]".repeat(12));
    let schema = json!({"description":"A repo-relative path without . segments.",
            "anyOf":[{"type":"string","pattern":pattern},
                {"type":"array","items":{"type":"string","pattern":pattern}}]});
    let error = validate_input_schema_for_test(&schema, json!("./a"));
    assert_eq!(
        error.issues[0].message,
        r#""./a" does not match the expected form: A repo-relative path without . segments."#,
        "{error:?}"
    );
}

fn validate_input_schema_for_test(
    schema: &Value,
    mut value: Value,
) -> super::super::ContractValidationError {
    super::super::validate_input_schema(schema, schema, &mut value, &mut vec!["path".into()])
        .unwrap_err()
}

/// A value kind matches the unknown field's name in any case.
#[test]
fn a_value_kind_matches_in_any_case() {
    let properties = json!({"ref":{"type":"string","x-valueKinds":["sha","branch"]}});
    let properties = properties.as_object().expect("properties");
    assert_eq!(
        super::super::fields_of_kind(&json!({}), properties, "SHA"),
        ["ref"]
    );
    assert!(super::super::fields_of_kind(&json!({}), properties, "tag").is_empty());
}

/// A rejected qualifier term lists the keys the field declares.
#[test]
fn rejected_qualifier_lists_the_declared_keys() {
    let schema = json!({"type":"string","pattern":"^\\s*(?:(?:author|label):(?:\"[^\"]+\"|[^\\s\"]+)\\s*)+$",
            "x-qualifierKeys":["author","label"]});
    let message = |schema: &Value| {
        validate_schema(
            schema,
            schema,
            &mut json!("linked:issue"),
            &mut vec!["qualifiers".into()],
        )
        .unwrap_err()
        .issues[0]
            .message
            .clone()
    };
    let listed = message(&schema);
    assert!(listed.contains("allowed keys: author, label"), "{listed}");
    assert!(listed.contains("\"linked:issue\""), "{listed}");
    let mut bare = schema.clone();
    bare.as_object_mut()
        .expect("schema")
        .remove("x-qualifierKeys");
    assert!(!message(&bare).contains("allowed keys"));
}
