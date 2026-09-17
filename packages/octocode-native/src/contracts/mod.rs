//! Generated public tool contracts and transport-neutral input preparation.

pub mod generated;
mod instructions;
mod prepare;
mod validate;

pub use instructions::mcp_instructions;
pub use prepare::{ContractInputError, PrepareOptions, PreparedQuery, prepare};
pub use validate::{
    ContractValidationError, ValidationIssue, format_input_error, validate, validate_output,
    validate_query,
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
    // Delegate to validate_query which handles the wrap/unwrap internally
    // and strips the "queries.0." prefix from any validation error paths.
    validate_query(tool_name, serde_json::Value::Object(prepared.query))
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

    /// S8 schema single-source guard (RFC 20260917-finish-rust-migration):
    /// Assert that no `.rs` source file outside `contracts/generated/` defines
    /// inline JSON Schema vocabulary (`"$schema"`, `"inputSchema"` as an object
    /// key inside a `json!()` macro call, or `"properties"` as an *lvalue* in a
    /// JSON literal). The provenance check above ensures generated schemas
    /// came from an official clean build of `octocode-core`; this complementary
    /// scan catches accidental copy-paste of schema fragments into tool runners.
    ///
    /// Patterns checked (as substrings in non-comment, non-test lines):
    ///   - `json!({"$schema":` — top-level JSON Schema declaration
    ///   - `json!({"inputSchema":` — MCP tool registration schema inline
    ///   - `"inputSchema": {` — same, written as a field expression
    #[test]
    fn no_inline_schema_literals_outside_generated_contracts() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let src_dir = manifest_dir.join("src");
        let generated_dir = src_dir
            .join("contracts")
            .join("generated");

        // Patterns that indicate inline (hand-authored) JSON Schema definition.
        // We do not check for "properties" broadly because it appears in
        // schema-reading code (e.g. contracts/validate.rs). Instead we only
        // flag the two patterns that would only ever appear as schema authors:
        let forbidden: &[&str] = &[
            "json!({\"$schema\":",
            "json!({\"inputSchema\":",
            "\"inputSchema\": {",
        ];

        let mut violations: Vec<String> = Vec::new();
        scan_for_schema_literals(&src_dir, &generated_dir, forbidden, &mut violations);

        assert!(
            violations.is_empty(),
            "Hand-authored JSON Schema literals found outside contracts/generated/ \
             — move them to octocode-core and regenerate:\n{}",
            violations.join("\n")
        );
    }

    fn scan_for_schema_literals(
        dir: &std::path::Path,
        skip: &std::path::Path,
        patterns: &[&str],
        violations: &mut Vec<String>,
    ) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // Skip the generated directory entirely.
            if path == skip || path.starts_with(skip) {
                continue;
            }
            if path.is_dir() {
                scan_for_schema_literals(&path, skip, patterns, violations);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let Ok(content) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for (line_no, line) in content.lines().enumerate() {
                    let trimmed = line.trim();
                    // Skip comment lines.
                    if trimmed.starts_with("//") || trimmed.starts_with('*') {
                        continue;
                    }
                    for pattern in patterns {
                        if trimmed.contains(pattern) {
                            violations.push(format!(
                                "{}:{}: suspicious inline schema pattern `{}`",
                                path.display(),
                                line_no + 1,
                                pattern
                            ));
                        }
                    }
                }
            }
        }
    }
}
