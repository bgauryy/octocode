use super::*;
use serde_json::Value;

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

/// A list item whose pattern is too long to read names the list's own note,
/// never the enforcement regex.
#[test]
fn an_unreadable_item_pattern_gives_the_lists_note() {
    let pattern = format!("^(?:{})$", "[a-z]".repeat(12));
    let schema = json!({"type":"array","description":"Repo-relative paths without . segments.",
            "items":{"type":"string","allOf":[{"pattern":pattern}]}});
    let error = validate_input_schema_for_test(&schema, json!(["./a"]));
    assert_eq!(
        error.issues[0].message,
        r#""./a" does not match the expected form: Repo-relative paths without . segments."#,
        "{error:?}"
    );
}

/// A value outside a declared `format` names that format, as the published
/// schema shows it, never the enforcement regex derived for it.
#[test]
fn a_value_outside_its_format_names_the_format_not_its_pattern() {
    let row = json!({"operation": "pullRequest",
        "filters": {"createdAt": {"include": [{"from": "not-a-date"}]}}});
    let error =
        crate::contracts::prepare_many_and_validate("ghSearchHistory", json!({"queries": [row]}))
            .expect_err("not a date-time");
    let shown = crate::contracts::validate::format_input_error("ghSearchHistory", &error);
    assert_eq!(
        shown["error"],
        r#"queries.0.filters.createdAt.include.0.from: "not-a-date" is not a valid value for from: it must be a date-time (RFC 3339), e.g. 2026-10-01T00:00:00Z"#,
        "{shown}"
    );
    // A format the validator has no example for is still named.
    let schema =
        json!({"type": "string", "format": "x-ticket", "pattern": "^[A-Z]{2,8}-[0-9]{1,9}$"});
    let error = validate_input_schema_for_test(&schema, json!("ab"));
    assert_eq!(
        error.issues[0].message,
        r#""ab" is not a valid value for path: it must be in the x-ticket format"#
    );
}

fn validate_input_schema_for_test(
    schema: &Value,
    mut value: Value,
) -> super::super::ContractValidationError {
    super::super::validate_input_schema(schema, schema, &mut value, &mut vec!["path".into()])
        .unwrap_err()
}

fn first_error(tool: &str, row: Value) -> String {
    let error = crate::contracts::prepare_many_and_validate(tool, json!({"queries": [row]}))
        .expect_err("invalid row");
    crate::contracts::validate::format_input_error(tool, &error)["error"]
        .as_str()
        .expect("message")
        .to_owned()
}

/// A repository path states its rule, on every field that takes one, whatever
/// the field's own note says.
#[test]
fn a_repository_path_states_its_rule_on_every_field() {
    const RULE: &str =
        "it must be repository-relative, with forward slashes and no \".\" or \"..\" segments";
    for (tool, field, row) in [
        (
            "ghSearchCode",
            "queries.0.path",
            json!({"owner":"o","keywords":["y"],"path":"../x"}),
        ),
        (
            "ghSearchCode",
            "queries.0.path",
            json!({"owner":"o","keywords":["y"],"path":"/abs/x"}),
        ),
        (
            "ghSearchHistory",
            "queries.0.path",
            json!({"operation":"commit","owner":"o","repo":"r","path":"a\\b"}),
        ),
        (
            "ghCloneRepo",
            "queries.0.paths.0",
            json!({"owner":"o","repo":"r","paths":["../x"]}),
        ),
    ] {
        let shown = first_error(tool, row.clone());
        assert!(shown.starts_with(&format!("{field}: ")), "{row}: {shown}");
        assert!(shown.contains(RULE), "{row}: {shown}");
    }
}

/// The rule is read from what the pattern does, so only a pattern that behaves
/// as a repository path says it.
#[test]
fn only_a_pattern_that_behaves_as_a_repository_path_states_that_rule() {
    for (pattern, value) in [
        (r"^\S+$", "a b"),
        (r"^[^/\\]+$", "a/b"),
        (r"^(?!\/).+$", "/a"),
        (r"^[^\\]+$", "a\\b"),
    ] {
        let schema = json!({"type":"string","pattern":pattern});
        let shown = validate_input_schema_for_test(&schema, json!(value));
        assert!(
            !format!("{shown:?}").contains("repository-relative"),
            "{pattern}: {shown:?}"
        );
    }
}

/// A value with no rule of its own is named as a value of the field.
#[test]
fn a_pattern_error_reads_as_a_value_of_the_field() {
    let row = json!({"operation":"pullRequest","owner":"o","repo":"r","filters":{"createdAt":{"include":[{"from":"yesterday"}]}}});
    let shown = first_error("ghSearchHistory", row);
    assert!(
        shown.contains(r#""yesterday" is not a valid value for from: it must be"#),
        "{shown}"
    );
}

/// A union of an integer and a string form reports a string against the string
/// form, which says what both forms take.
#[test]
fn a_string_is_judged_by_the_string_form_of_its_union() {
    let row = json!({"operation":"match","path":".","rule":{"kind":"x","nthChild":"junk"}});
    let shown = first_error("astSearch", row);
    assert!(shown.contains("an integer or An+B"), "{shown}");
    assert!(!shown.contains("Expected number"), "{shown}");
}

/// A field several operations take names them all, though one of them is a
/// union of forms of its own.
#[test]
fn a_misplaced_field_names_every_operation_that_takes_it() {
    let shown = first_error("clasify", json!({"operation":"models","context":"x"}));
    assert!(shown.contains("\"evaluate\""), "{shown}");
    assert!(shown.contains("\"sections\""), "{shown}");
}
