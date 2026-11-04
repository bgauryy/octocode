//! The embedded contract's self-audit: a test, since the contract is a
//! constant, so a corrupt one fails here and no process audits it again.
use super::super::audit::validate_in;
use crate::contracts::parsed_contract;
use serde_json::{Value, json};

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
    let root = json!({"$defs":{}});
    for corrupt in [
        json!({"type":"nonsense"}),
        json!({"type":"string","pattern":"("}),
        json!({"$ref":"#/$defs/missing"}),
    ] {
        assert!(validate_in(&root, &corrupt).is_err(), "accepted {corrupt}");
    }
}
