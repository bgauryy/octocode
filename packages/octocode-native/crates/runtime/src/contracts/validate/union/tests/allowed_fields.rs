//! The allowed fields an unknown-field error lists: every field of the
//! forms the row's selectors and other fields still fit, not only the one
//! form branch scoring picked.
use super::super::validate_schema;
use super::*;

fn unknown_field_error(tool: &str, row: Value) -> String {
    let error = prepare_rows(tool, json!({ "queries": [row] })).expect_err("unknown field");
    let shown =
        crate::contracts::validate::format_input_error(tool, &error, &[], &serde_json::Value::Null);
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

/// A suggestion never names a field the object already carries: `cursor2`
/// beside `cursor` is not "use 'cursor'", and with no other candidate left
/// the message gives no suggestion.
#[test]
fn an_unknown_field_is_never_corrected_to_a_field_already_sent() {
    let error = prepare_rows("localSearch", json!({"cursor": "x", "cursor2": "y"}))
        .expect_err("unknown field");
    let message = crate::contracts::validate::format_input_error(
        "localSearch",
        &error,
        &[],
        &serde_json::Value::Null,
    )["error"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!message.contains("use 'cursor'"), "{message}");
    assert!(message.contains("cursor2"), "{message}");
    let row = unknown_field_error(
        "localSearch",
        json!({"path": ".", "matchString": ["a"], "path2": "x", "pth": "y"}),
    );
    assert!(!row.contains("use 'path'"), "{row}");
}

/// A value that sends no field of its union's forms has chosen none: the
/// error names every form (`tool and query, or url`) with an example value,
/// never the one form scoring picked ("Allowed fields: url", "Missing
/// required field: url"), wherever the union sits; `{}` names them too.
#[test]
fn a_value_choosing_no_form_names_every_form_with_an_example() {
    let questions = json!({"q":{"type":"noul","instructions":"Is it ok?"}});
    let shown = |row: Value| {
        let error = prepare_rows("clasify", json!({ "queries": [row] })).expect_err("no form");
        crate::contracts::validate::format_input_error("clasify", &error, &[], &Value::Null)
            .to_string()
    };
    for (row, at) in [
        (
            json!({"operation":"evaluate","source":{"file":"x.ts"},"questions":questions}),
            "Unknown field 'file' in queries[0].source;",
        ),
        (
            json!({"operation":"sections","resources":{"a":{"file":"x.ts"}},"questions":questions}),
            "Unknown field 'file' in queries[0].resources.a;",
        ),
        (
            json!({"operation":"evaluate","content":"Screenshot a.","questions":questions,
                "assets":{"a":{"source":{"file":"a.png"}}}}),
            "Unknown field 'file' in queries[0].assets.a.source;",
        ),
        (
            json!({"operation":"evaluate","source":{},"questions":questions}),
            "queries.0.source: Missing required fields;",
        ),
    ] {
        let message = shown(row.clone());
        assert!(message.contains(at), "{row}: {message}");
        assert!(
            message.contains("it takes tool and query, or url, e.g. {")
                && message.contains(r#"\"tool\":\"localFetch\""#),
            "{row}: {message}"
        );
        assert!(
            !message.contains("Allowed fields: url") && !message.contains("required field: url"),
            "{row}: {message}"
        );
    }
    // Forms that share a required field (`type`) are chosen by that field:
    // an unknown field lists the fields of every form.
    let message =
        shown(json!({"operation":"evaluate","content":"x","questions":{"q":{"prompt":"x"}}}));
    assert!(
        message.contains("Allowed fields: criteria, instructions, type"),
        "{message}"
    );
}
