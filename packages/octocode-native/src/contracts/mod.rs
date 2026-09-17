//! Generated public tool contracts and transport-neutral input preparation.

pub mod generated;
mod instructions;
mod prepare;
mod validate;

pub use instructions::mcp_instructions;
pub use prepare::{ContractInputError, PrepareOptions, PreparedQuery, prepare};
pub use validate::{
    ContractValidationError, ValidationIssue, format_input_error, validate, validate_output,
};

/// Embedded contracts are immutable across runtime handles and requests.
pub fn parsed_contract() -> Result<&'static serde_json::Value, &'static str> {
    static CONTRACT: std::sync::OnceLock<Result<serde_json::Value, String>> =
        std::sync::OnceLock::new();
    CONTRACT
        .get_or_init(|| serde_json::from_str(contract_json()).map_err(|error| error.to_string()))
        .as_ref()
        .map_err(String::as_str)
}

/// Prepare and validate a single tool query. Returns the validated query
/// `Value` with schema defaults applied. Callers that need response-paging
/// options (`responseCharLength`, `renderText`, etc.) should parse them from
/// the raw input *before* calling this function, since those envelope fields
/// are not part of the per-query contract.
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
    // Wrap the single query in the canonical { queries: [q] } envelope that
    // the JSON-Schema validators and normalization rules expect, then unwrap
    // after validation to return a flat single-query Value.
    let wrapped = serde_json::json!({ "queries": [serde_json::Value::Object(prepared.query)] });
    let mut validated = validate(tool_name, wrapped)?;
    // Extract the validated, defaulted query from position 0.
    validated["queries"]
        .as_array_mut()
        .and_then(|arr| arr.first_mut())
        .map(|q| q.take())
        .ok_or_else(|| ContractValidationError {
            issues: vec![ValidationIssue {
                rule_id: "prepare.extract".to_owned(),
                path: Vec::new(),
                message: "validated queries array was empty".to_owned(),
                schema: None,
                received: None,
            }],
        })
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

/// Provenance for the generated contract, including the clean canonical-core
/// revision and the fingerprint embedded above.
pub const fn contract_provenance_json() -> &'static str {
    include_str!("generated/contract-provenance.json")
}

#[cfg(test)]
mod contract_owner_tests {
    use super::{PrepareOptions, contract_provenance_json, prepare_and_validate, validate_output};
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
    fn generated_contract_has_clean_matching_provenance() {
        let provenance: serde_json::Value =
            serde_json::from_str(contract_provenance_json()).expect("provenance JSON");
        assert_eq!(provenance["sourcePackage"], "@octocodeai/octocode-core");
        assert_eq!(provenance["sourceDirty"], false);
        assert_eq!(
            provenance["contractFingerprint"],
            super::contract_fingerprint()
        );
        assert!(
            provenance["sourceRevision"]
                .as_str()
                .is_some_and(|revision| revision.len() == 40
                    && revision.chars().all(|ch| ch.is_ascii_hexdigit()))
        );
    }

    #[test]
    fn output_contract_rejects_unattributed_rows() {
        let error = validate_output(
            "localSearch",
            &json!({"results":[{"data":{"searchEngine":"rg","files":[]}}]}),
        )
        .expect_err("rows require index and evidence metadata");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.path.last().is_some_and(|part| part == "index"))
        );
    }

    #[test]
    fn output_contract_accepts_attributed_tool_data() {
        validate_output(
            "localSearch",
            &json!({
                "results":[{
                    "index":0,
                    "meta":{"evidence":{"kind":"lexical","confidence":"medium"}},
                    "data":{"searchEngine":"rg","files":[]}
                }]
            }),
        )
        .expect("canonical result envelope");
    }

    #[test]
    fn tree_materialize_fields_survive_generated_validation() {
        let query = prepare_and_validate(
            "ghSearch",
            json!({
                "operation": "tree",
                "owner": "a",
                "repo": "b",
                "materialize": true,
                "materializeOffset": 12
            }),
            PrepareOptions::default(),
        )
        .expect("materialize fields are in the generated tree contract");
        assert_eq!(query["materialize"], true);
        assert_eq!(query["materializeOffset"], 12);
    }
}
