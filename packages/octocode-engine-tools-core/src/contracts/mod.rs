//! Generated public tool contracts and transport-neutral input preparation.

pub mod generated;
mod prepare;
mod validate;

pub use prepare::{ContractInputError, PrepareOptions, PreparedBatch, prepare};
pub use validate::{ContractValidationError, ValidationIssue, format_input_error, validate};

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
        }],
    })?;
    let value = serde_json::to_value(prepared).map_err(|error| ContractValidationError {
        issues: vec![ValidationIssue {
            rule_id: "prepare.serialize".to_owned(),
            path: Vec::new(),
            message: error.to_string(),
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
