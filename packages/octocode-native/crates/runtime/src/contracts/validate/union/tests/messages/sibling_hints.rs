use super::*;

/// A sibling form that accepts the rejected value names its selectors only
/// when its other selectors accept the row: graphQuery `direction:"both"`
/// fits traversals, never `hubs`, so no `view` change can make it valid.
#[test]
fn a_sibling_hint_skips_forms_whose_other_selectors_reject_the_row() {
    let error = prepare_rows(
        "graphQuery",
        json!({"queries":[{"operation":"hubs","path":".","direction":"both"}]}),
    )
    .expect_err("hubs rejects both");
    let message = crate::contracts::validate::format_input_error(
        "graphQuery",
        &error,
        &[],
        &serde_json::Value::Null,
    )
    .to_string();
    assert!(message.contains("direction"), "{message}");
    assert!(!message.contains("requires"), "{message}");
}

/// A value only a sibling form accepts names that form's single-literal
/// selector: `unique:"list"` sent with `view:"content"` needs
/// `view:"matchOnly"`.
#[test]
fn a_sibling_hint_names_the_single_literal_selector_that_accepts_the_value() {
    let schema = json!({"anyOf":[
        {"type":"object","additionalProperties":false,
         "properties":{"view":{"const":"content"},"unique":{"enum":["off"]}}},
        {"type":"object","additionalProperties":false,
         "properties":{"view":{"const":"matchOnly"},"unique":{"enum":["off","list"]}}}
    ]});
    let mut value = json!({"view":"content","unique":"list"});
    let error = validate_schema(&schema, &schema, &mut value, &mut vec![])
        .expect_err("list needs matchOnly");
    let hint = r#"(unique:"list" requires view:"matchOnly")"#;
    assert!(
        error.issues.iter().any(|item| item.message.contains(hint)),
        "{error:?}"
    );
}

/// Several forms accepting the value are all named, merged by selector.
#[test]
fn a_sibling_hint_names_every_form_that_accepts_the_value() {
    let form = |view: &str, unique: Value| {
        json!({"type":"object","additionalProperties":false,
               "properties":{"view":{"const":view},"unique":{"enum":unique}}})
    };
    let schema = json!({"anyOf":[
        form("content", json!(["off"])),
        form("matchOnly", json!(["off", "list"])),
        form("count", json!(["off", "list"]))
    ]});
    let mut value = json!({"view":"content","unique":"list"});
    let error = validate_schema(&schema, &schema, &mut value, &mut vec![])
        .expect_err("list needs matchOnly or count");
    let hint = r#"(unique:"list" requires view one of "matchOnly", "count")"#;
    assert!(
        error.issues.iter().any(|item| item.message.contains(hint)),
        "{error:?}"
    );
}
