//! Generated public tool contracts and transport-neutral input preparation.

pub mod generated;
mod instructions;
mod prepare;
mod validate;

pub use instructions::mcp_instructions;
pub use prepare::{ContractInputError, PrepareOptions, PreparedBatch, prepare};
pub use validate::{ContractValidationError, ValidationIssue, format_input_error, validate};

/// Embedded contracts are immutable across runtime handles and requests.
pub fn parsed_contract() -> Result<&'static serde_json::Value, &'static str> {
    static CONTRACT: std::sync::OnceLock<Result<serde_json::Value, String>> =
        std::sync::OnceLock::new();
    CONTRACT
        .get_or_init(|| serde_json::from_str(contract_json()).map_err(|error| error.to_string()))
        .as_ref()
        .map_err(String::as_str)
}

pub fn prepare_and_validate(
    tool_name: &str,
    input: serde_json::Value,
    options: PrepareOptions<'_>,
) -> Result<serde_json::Value, ContractValidationError> {
    let prepared = prepare(tool_name, input, options).map_err(|error| ContractValidationError {
        issues: vec![ValidationIssue {
            rule_id: "prepare.envelope".to_owned(),
            path: Vec::new(),
            message: error.to_string(),
            schema: None,
            received: None,
        }],
    })?;
    let value = serde_json::to_value(prepared).map_err(|error| ContractValidationError {
        issues: vec![ValidationIssue {
            rule_id: "prepare.serialize".to_owned(),
            path: Vec::new(),
            message: error.to_string(),
            schema: None,
            received: None,
        }],
    })?;
    validate(tool_name, value)
}

/// Fingerprint of the canonical sibling-core contract used for this build.
#[must_use]
pub const fn contract_fingerprint() -> &'static str {
    generated::CONTRACT_FINGERPRINT
}

/// Canonical generated contract JSON used by adapters for registration.
#[must_use]
pub const fn contract_json() -> &'static str {
    generated::CONTRACT_JSON
}

#[cfg(test)]
mod contract_owner_tests {
    use super::{PrepareOptions, prepare_and_validate};
    use serde_json::json;

    #[test]
    fn syntax_operation_is_rejected_without_core_alias() {
        assert!(
            prepare_and_validate(
                "astSearch",
                json!({"operation":"syntax","path":"/tmp/lib.rs"}),
                PrepareOptions::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn rewrite_replacement_is_rejected_without_core_alias() {
        assert!(
            prepare_and_validate(
                "astRewrite",
                json!({
                    "path":"/tmp/src/lib.rs",
                    "pattern":"fn $N() {}",
                    "replacement":"fn $N() {}"
                }),
                PrepareOptions::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn lsp_line_is_rejected_without_core_alias() {
        assert!(
            prepare_and_validate(
                "lspSearch",
                json!({
                    "uri":"/tmp/lib.rs",
                    "symbolName":"is_available",
                    "line":257,
                    "operation":"definition"
                }),
                PrepareOptions::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn gh_search_tree_accepts_materialize_fields() {
        let prepared = prepare_and_validate(
            "ghSearch",
            json!({
                "operation": "tree",
                "owner": "o",
                "repo": "r",
                "materialize": true,
                "materializeOffset": 50
            }),
            PrepareOptions::default(),
        )
        .expect("tree materialize fields are additive");
        assert_eq!(prepared["queries"][0]["materialize"], json!(true));
        assert_eq!(prepared["queries"][0]["materializeOffset"], json!(50));
    }

    #[test]
    fn gh_search_rejects_materialize_on_code() {
        assert!(
            prepare_and_validate(
                "ghSearch",
                json!({
                    "operation": "code",
                    "keywords": ["x"],
                    "materialize": true
                }),
                PrepareOptions::default(),
            )
            .is_err()
        );
    }
}
