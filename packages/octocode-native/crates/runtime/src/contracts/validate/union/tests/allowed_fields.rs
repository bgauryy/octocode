//! The allowed fields an unknown-field error lists: every field of the
//! forms the row's selectors and other fields still fit, not only the one
//! form branch scoring picked.
use super::super::validate_schema;
use super::*;

fn unknown_field_error(tool: &str, row: Value) -> String {
    let error =
        prepare_many_and_validate(tool, json!({ "queries": [row] })).expect_err("unknown field");
    let shown = crate::contracts::validate::format_input_error(tool, &error);
    shown["error"].as_str().unwrap_or_default().to_owned()
}

fn allowed(message: &str) -> Vec<&str> {
    message
        .split_once("Allowed fields: ")
        .map(|(_, list)| list.trim_end_matches('.').split(", ").collect())
        .unwrap_or_default()
}

/// localFetch's content forms (whole file, ranges, matchString) share
/// `view:"content"`, explicit or by default: an unknown field lists the
/// fields of all three, never cursor state or another view's form.
#[test]
fn an_unknown_field_lists_every_field_of_the_forms_the_row_selects() {
    let path = "src/lib.rs";
    for row in [
        json!({"path": path, "bogus": true}),
        json!({"path": path, "view": "content", "bogus": true}),
    ] {
        let message = unknown_field_error("localFetch", row);
        let fields = allowed(&message);
        for field in [
            "block",
            "caseMode",
            "classification",
            "contextLines",
            "matchString",
            "ranges",
            "regexEngine",
            "view",
            "wholeWord",
        ] {
            assert!(fields.contains(&field), "{field} missing: {message}");
        }
        for state in ["snapshot", "readProgress", "selection"] {
            assert!(!fields.contains(&state), "{state} listed: {message}");
        }
    }
    let compact = unknown_field_error(
        "localFetch",
        json!({"path": path, "view": "compact", "bogus": true}),
    );
    let fields = allowed(&compact);
    assert!(
        fields.contains(&"ranges") && fields.contains(&"block"),
        "{compact}"
    );
    for other in ["matchString", "classification"] {
        assert!(!fields.contains(&other), "{other} listed: {compact}");
    }
}

/// A sent field narrows the forms: `matchString` fits only the match form,
/// so its own fields are the allowed ones and no other form's.
#[test]
fn the_row_s_other_fields_narrow_the_allowed_forms() {
    let schema = json!({"anyOf": [
        {"type": "object", "additionalProperties": false, "required": ["path"],
         "properties": {"path": {}, "view": {"const": "content", "default": "content"}}},
        {"type": "object", "additionalProperties": false, "required": ["path", "ranges"],
         "properties": {"path": {}, "ranges": {}, "view": {"const": "content", "default": "content"}}},
        {"type": "object", "additionalProperties": false, "required": ["path", "terms"],
         "properties": {"path": {}, "terms": {}, "caseMode": {},
                        "view": {"const": "content", "default": "content"}}},
        {"type": "object", "additionalProperties": false, "required": ["path", "view"],
         "properties": {"path": {}, "outline": {}, "view": {"const": "symbols"}}}
    ]});
    let known = |mut value: Value| {
        let error =
            validate_schema(&schema, &schema, &mut value, &mut vec![]).expect_err("unknown field");
        let issue = error
            .issues
            .iter()
            .find(|item| item.rule_id == "schema.unknown-field")
            .cloned()
            .expect("unknown-field issue");
        let mut fields = issue.schema.expect("schema")["knownFields"]
            .as_array()
            .expect("knownFields")
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        fields.sort();
        fields
    };
    assert_eq!(
        known(json!({"path": "a", "bogus": 1})),
        ["caseMode", "path", "ranges", "terms", "view"]
    );
    assert_eq!(
        known(json!({"path": "a", "terms": "x", "bogus": 1})),
        ["caseMode", "path", "terms", "view"]
    );
    assert_eq!(
        known(json!({"path": "a", "view": "symbols", "bogus": 1})),
        ["outline", "path", "view"]
    );
}
