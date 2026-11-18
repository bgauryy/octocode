//! The embedded contract's self-audit: a test, since the contract is a
//! constant, so a corrupt one fails here and no process audits it again.
use super::super::audit::validate_in;
use crate::contracts::parsed_contract;
use octocode_test_support::json_fixture;
use serde_json::Value;

#[test]
fn every_schema_of_the_embedded_contract_passes_the_audit() {
    let root = parsed_contract().expect("contract");
    for (name, definition) in root["$defs"].as_object().expect("$defs") {
        validate_in(root, definition).unwrap_or_else(|error| panic!("$defs/{name}: {error:?}"));
    }
    for id in crate::id::ToolId::ALL {
        let tool = crate::contracts::tool_contract(id).expect("tool");
        for field in ["querySchema", "inputSchema", "outputSchema"] {
            let schema: &Value = &tool[field];
            assert!(schema.is_object(), "{}: {field}", id.as_str());
            validate_in(root, schema)
                .unwrap_or_else(|error| panic!("{}: {field}: {error:?}", id.as_str()));
        }
    }
}

/// The audit still refuses what a corrupt contract would hold.
#[test]
fn the_audit_refuses_a_corrupt_schema() {
    let root = json_fixture(r#"{"$defs": {}}"#);
    for corrupt in [
        json_fixture(r#"{"type": "nonsense"}"#),
        json_fixture(r#"{"type": "string", "pattern": "("}"#),
        json_fixture(r##"{"$ref": "#/$defs/missing"}"##),
    ] {
        assert!(validate_in(&root, &corrupt).is_err(), "accepted {corrupt}");
    }
}

/// `x-patternMessage` is the message core authored for a pattern: the audit
/// takes it as a string, and refuses any other value.
#[test]
fn the_audit_takes_a_pattern_message_as_a_string() {
    let root = json_fixture(r#"{"$defs": {}}"#);
    let stated = json_fixture(
        r#"{
            "type": "string",
            "pattern": "^a:b$",
            "x-patternMessage": "Use a:b."
        }"#,
    );
    validate_in(&root, &stated).unwrap_or_else(|error| panic!("{stated}: {error:?}"));
    let corrupt = json_fixture(
        r#"{
            "type": "string",
            "pattern": "^a:b$",
            "x-patternMessage": 3
        }"#,
    );
    assert!(validate_in(&root, &corrupt).is_err(), "accepted {corrupt}");
}
