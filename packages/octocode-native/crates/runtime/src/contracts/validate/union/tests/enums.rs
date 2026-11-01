//! A value outside an enum that no branch selects by is that branch's own
//! error: the union reports the branch the value's fields describe and
//! names the field, the value and the allowed values, never another
//! branch's missing field.

use super::super::validate_schema;
use super::*;

/// A location (open, keyed by `path`) beside a diagnostic (open, needs
/// `message`): `role` is declared by the location alone, so it selects
/// nothing.
fn location_or_diagnostic() -> Value {
    json!({
        "$defs": {
            "Location": {"anyOf": [
                {"type":"object","additionalProperties":{},"required":["path"],
                 "properties":{"path":{"type":"string"},"line":{"type":"integer"},
                               "role":{"enum":["call","import","reference"]}}},
                {"type":"object","additionalProperties":{},"required":["symbolName"],
                 "properties":{"symbolName":{"type":"string"},"line":{"type":"integer"},
                               "role":{"enum":["call","import","reference"]}}}
            ]},
            "Diagnostic": {"type":"object","additionalProperties":{},"required":["message"],
                           "properties":{"message":{"type":"string"}}}
        },
        "anyOf": [{"$ref":"#/$defs/Location"},{"$ref":"#/$defs/Diagnostic"}]
    })
}

fn assert_names_the_enum(error: &crate::contracts::ContractValidationError) {
    let [issue] = error.issues.as_slice() else {
        panic!("one issue: {error:?}");
    };
    assert_eq!(issue.rule_id, "schema.enum", "{error:?}");
    assert_eq!(
        issue.path.last().map(String::as_str),
        Some("role"),
        "{error:?}"
    );
    assert!(
        issue.message.contains("\"bogus\"") && issue.message.contains("call, import, reference"),
        "{error:?}"
    );
}

#[test]
fn an_enum_mismatch_inside_a_union_reports_that_field_on_input() {
    let schema = location_or_diagnostic();
    let mut value = json!({"path":"a.rs","line":3,"role":"bogus"});
    let error = super::super::super::schema::validate_input_schema(
        &schema,
        &schema,
        &mut value,
        &mut Vec::new(),
    )
    .expect_err("role is outside its enum");
    assert_names_the_enum(&error);
}

#[test]
fn an_enum_mismatch_inside_a_union_reports_that_field_on_output() {
    let schema = location_or_diagnostic();
    let mut value = json!({"path":"a.rs","line":3,"role":"bogus"});
    let error = validate_schema(&schema, &schema, &mut value, &mut Vec::new())
        .expect_err("role is outside its enum");
    assert_names_the_enum(&error);
}

/// The published lspSearch contract: a references row whose `role` the
/// contract does not list fails at `role`, not as a diagnostic without
/// `message`.
#[test]
fn an_lsp_row_with_an_unlisted_role_fails_at_its_role() {
    let data = json!({
        "operation": "references",
        "lsp": {"serverAvailable": true, "provider": "referencesProvider"},
        "matches": [
            {"path": "src/a.ts", "displayRange": {"startLine": 3, "startCharacter": 1, "endLine": 3},
             "content": "3\ttarget();\n", "role": "bogus"}
        ],
        "pagination": {"page": 1, "totalPages": 1, "totalItems": 1, "hasMore": false}
    });
    let error = crate::contracts::validate_output(
        "lspSearch",
        &json!({"results":[{"index":0,"data":data}]}),
    )
    .expect_err("role is outside its enum");
    let issue = error
        .issues
        .iter()
        .find(|issue| issue.path.last().map(String::as_str) == Some("role"))
        .unwrap_or_else(|| panic!("an issue at role: {error:?}"));
    assert_eq!(issue.rule_id, "schema.enum", "{error:?}");
    assert!(issue.message.contains("\"bogus\""), "{error:?}");
    assert!(
        !error
            .issues
            .iter()
            .any(|issue| issue.rule_id == "schema.required"),
        "{error:?}"
    );
}
