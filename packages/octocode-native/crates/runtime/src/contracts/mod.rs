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
    let prepared = prepare(tool_name, input, options).map_err(prepare_validation_error)?;
    // Delegate to validate_query which handles the wrap/unwrap internally
    // and strips the "queries.0." prefix from any validation error paths.
    validate_query(tool_name, serde_json::Value::Object(prepared.query))
}

/// Prepare and validate every query in the canonical bulk envelope. A flat
/// object and a single-element array remain accepted for direct CLI parity.
/// Defaults are applied per query before validating the complete envelope, so
/// cross-query limits and indexed diagnostics remain contract-owned.
pub fn prepare_many_and_validate(
    tool_name: &str,
    input: serde_json::Value,
    options: PrepareOptions<'_>,
) -> Result<Vec<serde_json::Value>, ContractValidationError> {
    let is_bulk = input.is_array()
        || input
            .as_object()
            .is_some_and(|object| object.contains_key("queries"));
    if !is_bulk {
        return prepare_and_validate(tool_name, input, options).map(|query| vec![query]);
    }

    let mut envelope = match input {
        serde_json::Value::Array(queries) => serde_json::json!({"queries": queries}),
        serde_json::Value::Object(object) => serde_json::Value::Object(object),
        _ => unreachable!("bulk input is an array or object"),
    };
    let queries = envelope
        .get_mut("queries")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| {
            prepare_validation_error(ContractInputError::new("queries must be an array"))
        })?;
    for query in queries.iter_mut() {
        let prepared =
            prepare(tool_name, query.take(), options.clone()).map_err(prepare_validation_error)?;
        *query = serde_json::Value::Object(prepared.query);
    }
    let validated = validate(tool_name, envelope)?;
    Ok(validated
        .get("queries")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn prepare_validation_error(error: ContractInputError) -> ContractValidationError {
    ContractValidationError {
        issues: vec![ValidationIssue {
            rule_id: "prepare.envelope".to_owned(),
            path: Vec::new(),
            message: error.to_string(),
            schema: None,
            received: None,
        }],
    }
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
    use super::{
        PrepareOptions, contract_json, contract_provenance_json, prepare_and_validate,
        prepare_many_and_validate, validate_output,
    };
    use serde_json::json;

    #[test]
    fn public_queries_require_explicit_reasoning() {
        for reasoning in [None, Some("   ")] {
            let mut query = json!({"path":"/tmp/source.rs"});
            if let Some(reasoning) = reasoning {
                query["reasoning"] = json!(reasoning);
            }
            let result = prepare_and_validate("localFetch", query, PrepareOptions::default());
            assert!(
                result.is_err(),
                "reasoning must be caller-supplied and nonblank: {result:?}"
            );
        }
    }

    #[test]
    fn bulk_queries_are_defaulted_and_validated_with_stable_order() {
        let queries = prepare_many_and_validate(
            "localFetch",
            json!({"queries":[
                {"path":"/tmp/a","reasoning":"Read a."},
                {"path":"/tmp/b","reasoning":"Read b."}
            ]}),
            PrepareOptions::default(),
        )
        .expect("valid bulk input");
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0]["path"], "/tmp/a");
        assert_eq!(queries[1]["path"], "/tmp/b");
        assert_eq!(queries[0]["debug"], false);
        assert_eq!(queries[1]["debug"], false);
    }

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
    fn public_response_and_tree_limits_are_pinned_against_silent_drift() {
        // `generated_contract_has_clean_matching_provenance` proves the contract
        // MATCHES core, but the fingerprint moves together with any core regen —
        // it does not prove the numeric bounds are still the intended values, so a
        // core change that relaxed a public limit would pass provenance silently.
        // This pins the two limits that also surface in the live MCP schema so any
        // change is a visible, reviewed test diff. (Codifies the concern formerly
        // tracked in the schema-authority drift RFC, on the surviving native
        // authority — the TS granular server that motivated the RFC is gone.)
        fn collect(value: &serde_json::Value, response: &mut Vec<u64>, tree_depth: &mut Vec<u64>) {
            match value {
                serde_json::Value::Object(map) => {
                    if let Some(max) = map
                        .get("responseCharLength")
                        .and_then(|schema| schema.get("maximum"))
                        .and_then(serde_json::Value::as_u64)
                    {
                        response.push(max);
                    }
                    if map.get("description").and_then(serde_json::Value::as_str)
                        == Some("Tree recursion depth.")
                        && let Some(max) = map.get("maximum").and_then(serde_json::Value::as_u64)
                    {
                        tree_depth.push(max);
                    }
                    for child in map.values() {
                        collect(child, response, tree_depth);
                    }
                }
                serde_json::Value::Array(items) => {
                    for child in items {
                        collect(child, response, tree_depth);
                    }
                }
                _ => {}
            }
        }

        let contract: serde_json::Value =
            serde_json::from_str(contract_json()).expect("contract JSON");
        let mut response_maxima = Vec::new();
        let mut tree_depth_maxima = Vec::new();
        collect(&contract, &mut response_maxima, &mut tree_depth_maxima);

        assert!(
            !response_maxima.is_empty(),
            "expected at least one responseCharLength bound in the contract"
        );
        assert!(
            response_maxima.iter().all(|max| *max == 50_000),
            "responseCharLength.maximum drifted from 50000: {response_maxima:?}"
        );
        assert!(
            !tree_depth_maxima.is_empty(),
            "expected the tree-recursion maxDepth bound in the contract"
        );
        assert!(
            tree_depth_maxima.iter().all(|max| *max == 20),
            "tree recursion maxDepth drifted from 20: {tree_depth_maxima:?}"
        );
    }

    #[test]
    fn output_contract_rejects_rows_without_an_index() {
        let error = validate_output(
            "localSearch",
            &json!({"results":[{"data":{"searchEngine":"rg","files":[]}}]}),
        )
        .expect_err("rows require an index");
        assert!(
            error
                .issues
                .iter()
                .any(|issue| issue.path.last().is_some_and(|part| part == "index"))
        );
    }

    #[test]
    fn output_contract_accepts_tool_data_with_or_without_debug_meta() {
        for row in [
            json!({"index":0,"data":{"searchEngine":"rg","files":[]}}),
            json!({
                "index":0,
                "meta":{"evidence":{"kind":"lexical","confidence":"medium"}},
                "data":{"searchEngine":"rg","files":[]}
            }),
        ] {
            validate_output("localSearch", &json!({"results":[row]}))
                .expect("canonical result envelope");
        }
    }

    #[test]
    fn artifact_keyword_continuation_drops_null_lookup_fields() {
        // Regression: keyword-discovery emitted next.nextPage.query with
        // packageName/registry serialized as null (the exact-lookup branch
        // expects strings), tripping outputContractViolation. The builder now
        // strips nulls so the query matches the keyword+cursor branch.
        let data = |query: serde_json::Value| {
            json!({"results":[{"index":0,"data":{
                "type":"npm",
                "artifacts":[],
                "pagination":{"perPage":5,"returned":0,"hasMore":true,"totalFound":100},
                "next":{"nextPage":{"tool":"artifactSearch","query":query,"confidence":"exact"}}
            }}]})
        };
        let buggy = validate_output(
            "artifactSearch",
            &data(json!({"type":"npm","packageName":null,"registry":null,
                "keywords":["x"],"pageSize":5,"cursor":"c"})),
        )
        .expect_err("null packageName/registry must be rejected");
        assert!(
            buggy.issues.iter().any(|issue| issue
                .path
                .iter()
                .any(|part| part == "packageName" || part == "registry")),
            "expected a packageName/registry issue, got {buggy:?}"
        );
        // Fixed shape: no lookup-field issue may remain (other unrelated
        // data-body issues, if any, are tolerated).
        if let Err(fixed) = validate_output(
            "artifactSearch",
            &data(json!({"type":"npm","keywords":["x"],"pageSize":5,"cursor":"c"})),
        ) {
            assert!(
                !fixed.issues.iter().any(|issue| issue
                    .path
                    .iter()
                    .any(|part| part == "packageName" || part == "registry")),
                "keyword continuation must not trip lookup-field errors: {fixed:?}"
            );
        }
    }

    #[test]
    fn ghsearch_advisory_next_action_carries_pagination() {
        // Regression: gh_search empty_scope emitted advisory tree/repositories
        // recovery queries without page/pageSize, which the continuation
        // contract requires. They are now stamped page:1 + pageSize.
        let data = |query: serde_json::Value| {
            json!({"results":[{"index":0,"data":{
                "next":{"viewStructure":{"tool":"ghSearch","query":query,
                    "confidence":"exact","why":"Verify structure."}}
            }}]})
        };
        let buggy = validate_output(
            "ghSearch",
            &data(json!({"operation":"tree","owner":"o","repo":"r","path":""})),
        )
        .expect_err("missing page/pageSize must be rejected");
        assert!(
            buggy.issues.iter().any(|issue| issue
                .path
                .iter()
                .any(|part| part == "page" || part == "pageSize")),
            "expected a page/pageSize issue, got {buggy:?}"
        );
        if let Err(fixed) = validate_output(
            "ghSearch",
            &data(json!({"operation":"tree","owner":"o","repo":"r","path":"",
                "page":1,"pageSize":30})),
        ) {
            assert!(
                !fixed.issues.iter().any(|issue| issue
                    .path
                    .iter()
                    .any(|part| part == "page" || part == "pageSize")),
                "stamped advisory query must not trip page/pageSize errors: {fixed:?}"
            );
        }
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
                "materializeOffset": 12,
                "reasoning": "Exercise materialized tree validation."
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
        let generated_dir = src_dir.join("contracts").join("generated");

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
