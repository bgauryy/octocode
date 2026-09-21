//! Generated public tool contracts and transport-neutral input preparation.

pub mod generated;
mod prepare;
mod validate;

pub use prepare::{ContractInputError, PrepareOptions, prepare};
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

/// Replace contract-violating result rows with row-level
/// `outputContractViolation` errors so one drifting emitter cannot discard a
/// batch's healthy rows. Returns the patched envelope only when every issue
/// maps to a result row and the patched envelope itself validates; envelope-
/// level violations return `None` and the caller keeps the whole-call failure.
pub fn isolate_row_violations(
    tool_name: &str,
    output: &serde_json::Value,
    error: &ContractValidationError,
) -> Option<serde_json::Value> {
    let mut row_issues: std::collections::BTreeMap<usize, Vec<String>> =
        std::collections::BTreeMap::new();
    for issue in &error.issues {
        let index = match (issue.path.first().map(String::as_str), issue.path.get(1)) {
            (Some("results"), Some(second)) => second.parse::<usize>().ok()?,
            _ => return None,
        };
        row_issues.entry(index).or_default().push(format!(
            "{}: {}",
            issue.path.get(2..).unwrap_or_default().join("."),
            issue.message
        ));
    }
    if row_issues.is_empty() {
        return None;
    }
    let mut patched = output.clone();
    let rows = patched.get_mut("results")?.as_array_mut()?;
    for (index, issues) in &row_issues {
        let row = rows.get_mut(*index)?;
        row["status"] = serde_json::Value::String("error".into());
        row["data"] = serde_json::json!({
            "error": format!(
                "Row output violated the {tool_name} contract and was withheld: {}",
                issues.join("; ")
            ),
            "errorCode": "outputContractViolation",
        });
    }
    validate_output(tool_name, &patched).ok()?;
    Some(patched)
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
    let query = validate_query(tool_name, serde_json::Value::Object(prepared.query))?;
    if tool_name == "semanticAssess" {
        validate_semantic_relations(&query)?;
    }
    Ok(query)
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
    let queries = validated
        .get("queries")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if tool_name == "semanticAssess" {
        for query in &queries {
            validate_semantic_relations(query)?;
        }
    }
    Ok(queries)
}

fn validate_semantic_relations(query: &serde_json::Value) -> Result<(), ContractValidationError> {
    let resources = query["resources"].as_array().cloned().unwrap_or_default();
    let questions = query["questions"].as_array().cloned().unwrap_or_default();
    for (field, rows) in [("resources", &resources), ("questions", &questions)] {
        let mut seen = std::collections::HashSet::new();
        for (index, row) in rows.iter().enumerate() {
            if let Some(id) = row.get("id").and_then(serde_json::Value::as_str)
                && !seen.insert(id)
            {
                return Err(ContractValidationError {
                    issues: vec![ValidationIssue {
                        rule_id: "semantic-assess.unique-ids".into(),
                        path: vec![field.into(), index.to_string(), "id".into()],
                        message: format!("Duplicate {field} id: {id}"),
                        schema: None,
                        received: None,
                    }],
                });
            }
        }
    }
    let cells = resources.len().saturating_mul(questions.len());
    if cells > 25 {
        return Err(ContractValidationError {
            issues: vec![ValidationIssue {
                rule_id: "semantic-assess.cell-limit".into(),
                path: Vec::new(),
                message: format!("resources × questions produces {cells} cells; maximum is 25."),
                schema: None,
                received: None,
            }],
        });
    }
    Ok(())
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
        PrepareOptions, contract_json, contract_provenance_json, isolate_row_violations,
        prepare_and_validate, prepare_many_and_validate, validate_output,
    };
    use serde_json::json;

    #[test]
    fn row_scoped_output_violations_degrade_to_row_errors() {
        let output = json!({"results":[
            {"index":0,"data":{"error":"upstream failed"}},
            {"index":1,"data":{"bogusKey":true}}
        ]});
        let error = validate_output("localSearch", &output).expect_err("row 1 violates");
        let patched = isolate_row_violations("localSearch", &output, &error).expect("isolated");
        assert_eq!(patched["results"][0]["data"]["error"], "upstream failed");
        assert!(patched["results"][0].get("status").is_none());
        assert_eq!(patched["results"][1]["status"], "error");
        assert_eq!(
            patched["results"][1]["data"]["errorCode"],
            "outputContractViolation"
        );
        assert!(validate_output("localSearch", &patched).is_ok());
        // Envelope-level violations stay whole-call failures.
        let envelope = json!({"results":"not-an-array"});
        let error = validate_output("localSearch", &envelope).expect_err("envelope violates");
        assert!(isolate_row_violations("localSearch", &envelope, &error).is_none());
    }

    #[test]
    fn public_queries_accept_optional_but_nonblank_reasoning() {
        // Omitting reasoning is accepted: it is optional across every tool.
        let omitted = prepare_and_validate(
            "localFetch",
            json!({"path":"/tmp/source.rs"}),
            PrepareOptions::default(),
        );
        assert!(omitted.is_ok(), "reasoning must be optional: {omitted:?}");
        assert!(
            omitted.unwrap().get("reasoning").is_none(),
            "omitted reasoning must never be fabricated"
        );

        // A supplied-but-blank reasoning is still rejected.
        let blank = prepare_and_validate(
            "localFetch",
            json!({"path":"/tmp/source.rs","reasoning":"   "}),
            PrepareOptions::default(),
        );
        assert!(
            blank.is_err(),
            "reasoning, when supplied, must be nonblank: {blank:?}"
        );
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
    fn semantic_matrix_stays_one_query_for_resource_major_runtime_execution() {
        let question = json!({"type":"noul","instructions":"Is it relevant?"});
        let queries = prepare_many_and_validate(
            "semanticAssess",
            json!({
                "id":"matrix",
                "reasoning":"  Classify every resource.  ",
                "resources":[
                    {"id":"r1","context":{"value":{"text":"one"}}},
                    {"id":"r2","context":{"value":{"text":"two"}}}
                ],
                "questions":[
                    {"id":"q1","question":question},
                    {"id":"q2","question":{"type":"score","instructions":"Rate risk","criteria":["low","high"]}}
                ]
            }),
            PrepareOptions::default(),
        )
        .expect("valid semantic matrix");
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0]["resources"][0]["id"], "r1");
        assert_eq!(queries[0]["resources"][1]["id"], "r2");
        assert_eq!(queries[0]["questions"][0]["id"], "q1");
        assert_eq!(queries[0]["questions"][1]["id"], "q2");
        assert_eq!(queries[0]["reasoning"], "Classify every resource.");
    }

    #[test]
    fn semantic_matrix_rejects_duplicate_ids_and_more_than_twenty_five_cells() {
        let question = json!({"type":"noul","instructions":"Is it relevant?"});
        let duplicate = prepare_many_and_validate(
            "semanticAssess",
            json!({
                "id":"duplicates",
                "reasoning":"Classify resources.",
                "resources":[
                    {"id":"same","context":{"value":"one"}},
                    {"id":"same","context":{"value":"two"}}
                ],
                "questions":[{"id":"q1","question":question}]
            }),
            PrepareOptions::default(),
        )
        .expect_err("duplicate IDs must fail");
        assert_eq!(duplicate.issues[0].path, ["resources", "1", "id"]);

        let resources: Vec<_> = (0..6)
            .map(|index| json!({"id":format!("r{index}"),"context":{"value":{"index":index}}}))
            .collect();
        let questions: Vec<_> = (0..5)
            .map(|index| json!({"id":format!("q{index}"),"question":question.clone()}))
            .collect();
        let oversized = prepare_many_and_validate(
            "semanticAssess",
            json!({
                "id":"oversized",
                "reasoning":"Classify resources.",
                "resources":resources,
                "questions":questions
            }),
            PrepareOptions::default(),
        )
        .expect_err("matrix cell limit must fail");
        assert_eq!(oversized.issues[0].rule_id, "semantic-assess.cell-limit");
        assert!(oversized.issues[0].message.contains("maximum is 25"));
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
    fn generated_contract_is_enforcement_only() {
        // Presentation and MCP instructions are core-delivered by the JS
        // layers; the embed carries machine-enforced surfaces exclusively.
        let contract = super::parsed_contract().expect("generated contract");
        let top = contract.as_object().expect("contract object");
        for forbidden in ["mcpInstructions", "mcpInstructionTable"] {
            assert!(!top.contains_key(forbidden), "top-level {forbidden}");
        }
        for tool in contract["tools"].as_array().expect("tool array") {
            let name = tool["name"].as_str().expect("tool name");
            let object = tool.as_object().expect("tool object");
            for forbidden in ["title", "description", "examples", "annotations"] {
                assert!(!object.contains_key(forbidden), "{name}.{forbidden}");
            }
            assert!(
                object.contains_key("shortDescription")
                    && object.contains_key("querySchema")
                    && object.contains_key("inputSchema")
                    && object.contains_key("outputSchema")
                    && object.contains_key("rules"),
                "{name} enforcement fields"
            );
        }
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
    fn generated_contract_body_hash_is_pinned_against_hand_edits() {
        // `generated_contract_has_clean_matching_provenance` compares two
        // co-generated literals; the fingerprint is generator-authored and never
        // recomputed from the body, so a hand-edit to the generated contract
        // passes it. This pin recomputes a digest over the embedded bytes:
        // regeneration from core must update the literal (same discipline as
        // the napi ABI snapshot); any other change to the generated body fails.
        use sha2::{Digest, Sha256};
        let digest = hex::encode(Sha256::digest(contract_json().as_bytes()));
        assert_eq!(
            digest, "134ded867a03df452157bb3e3a3b92d6f5e52f00a0ff0dcc366a860dc620ee43",
            "generated contract body changed without regeneration from core"
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
    fn history_output_accepts_pull_request_optional_content_actions() {
        let output = json!({"results":[{
            "index":0,
            "status":"empty",
            "data":{
                "type":"pullRequests",
                "pullRequests":[{
                    "number":463,
                    "title":"Example",
                    "state":"merged",
                    "author":"octocode",
                    "createdAt":"2026-08-07T20:37:26Z",
                    "next":{
                        "getBody":{
                            "tool":"ghGetHistoryItem",
                            "confidence":"exact",
                            "query":{
                                "operation":"pullRequest",
                                "owner":"octocodeai",
                                "repo":"octocode",
                                "number":463,
                                "content":{"body":true},
                                "reasoning":"Read the optional body.",
                                "debug":false
                            }
                        }
                    }
                }],
                "errorCode":"noSelectedFilesMatched",
                "hints":["Choose a changed path first."]
            }
        }]});
        validate_output("ghGetHistoryItem", &output).expect("valid history output");
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
    fn ghsearch_advisory_next_action_accepts_defaults_but_requires_operation() {
        // Continuation validation applies query defaults on a clone, so
        // page/pageSize/debug may be omitted. The operation discriminator is
        // genuinely required and keeps the follow-up executable.
        let data = |query: serde_json::Value| {
            json!({"results":[{"index":0,"data":{
                "operation":"tree",
                "next":{"viewStructure":{"tool":"ghSearch","query":query,
                    "confidence":"exact","why":"Verify structure."}}
            }}]})
        };
        validate_output(
            "ghSearch",
            &data(json!({"operation":"tree","owner":"o","repo":"r","path":""})),
        )
        .expect("defaulted pagination fields may be omitted");
        let invalid = validate_output("ghSearch", &data(json!({"owner":"o","repo":"r","path":""})))
            .expect_err("missing operation must be rejected");
        assert!(
            invalid
                .issues
                .iter()
                .any(|issue| issue.path.iter().any(|part| part == "operation")),
            "expected an operation issue, got {invalid:?}"
        );
    }

    #[test]
    fn deadcode_verify_references_accepts_defaults_but_requires_anchor() {
        // orderHint/page/format/debug are defaulted during validation. The URI
        // remains a real anchored-reference requirement.
        let data = |query: serde_json::Value| {
            json!({"results":[{"index":0,"data":{
                "operation":"topology",
                "analysis":"deadCode",
                "results":[{"file":"src/util.ts","name":"greet","kind":"function",
                    "line":1,"reason":"unreferenced-export","viaHeuristic":"reexport-chain"}],
                "completeness":{"results":"complete","graph":"complete","diagnostics":"complete"},
                "next":{"verifyReferences":{"tool":"lspSearch","query":query,
                    "confidence":"high","why":"Verify candidate before deletion."}}
            }}]})
        };
        let minimal = json!({"operation":"references","uri":"/r/a.ts","symbolName":"greet",
            "lineHint":1,"includeDeclaration":false,"groupByFile":true});
        validate_output("astSearch", &data(minimal.clone()))
            .expect("defaulted lspSearch fields may be omitted");
        let mut missing_anchor = minimal;
        missing_anchor.as_object_mut().unwrap().remove("uri");
        let invalid = validate_output("astSearch", &data(missing_anchor))
            .expect_err("missing anchored URI must be rejected");
        assert!(
            invalid
                .issues
                .iter()
                .any(|issue| issue.path.iter().any(|part| part == "uri")),
            "expected a URI issue, got {invalid:?}"
        );
    }

    #[test]
    fn filecontent_viewtree_accepts_defaults_but_requires_operation() {
        // Tree pagination/debug fields are defaulted during validation; the
        // operation discriminator is still required.
        let data = |query: serde_json::Value| {
            json!({"results":[{"index":0,"data":{
                "owner":"o","repo":"r","path":"missing.md","error":"not found",
                "next":{"viewTree":{"tool":"ghSearch","query":query,
                    "confidence":"low"}}
            }}]})
        };
        validate_output(
            "ghGetFileContent",
            &data(json!({"operation":"tree","owner":"o","repo":"r","path":"."})),
        )
        .expect("defaulted tree pagination fields may be omitted");
        let invalid = validate_output(
            "ghGetFileContent",
            &data(json!({"owner":"o","repo":"r","path":"."})),
        )
        .expect_err("missing operation must be rejected");
        assert!(
            invalid
                .issues
                .iter()
                .any(|issue| issue.path.iter().any(|part| part == "operation")),
            "expected an operation issue, got {invalid:?}"
        );
    }

    #[test]
    fn ghsearchhistory_nextpage_accepts_defaults_but_requires_operation() {
        // pageSize/debug are defaulted during validation; operation remains the
        // required discriminator for an executable history continuation.
        let data = |query: serde_json::Value| {
            json!({"results":[{"index":0,"data":{
                "type":"pullRequests","owner":"o","repo":"r","pullRequests":[],
                "next":{"nextPage":{"tool":"ghSearchHistory","query":query,"confidence":"exact"}}
            }}]})
        };
        validate_output(
            "ghSearchHistory",
            &data(json!({"operation":"pullRequests","owner":"o","repo":"r","page":2})),
        )
        .expect("defaulted history pageSize may be omitted");
        let invalid = validate_output(
            "ghSearchHistory",
            &data(json!({"owner":"o","repo":"r","page":2})),
        )
        .expect_err("missing operation must be rejected");
        assert!(
            invalid
                .issues
                .iter()
                .any(|issue| issue.path.iter().any(|p| p == "operation")),
            "expected an operation issue, got {invalid:?}"
        );
    }

    #[test]
    fn ghgethistoryitem_compare_nextpage_keeps_page() {
        // Regression: ghGetHistoryItem (compare) read pagination.nextPage:null
        // and clobbered the query's required `page` with null. The builder now
        // skips the continuation when there is no real next page.
        let data = |query: serde_json::Value| {
            json!({"results":[{"index":0,"data":{
                "type":"compare","owner":"o","repo":"r","base":"a","head":"b",
                "next":{"nextPage":{"tool":"ghGetHistoryItem","query":query,"confidence":"exact"}}
            }}]})
        };
        let buggy = validate_output(
            "ghGetHistoryItem",
            &data(json!({"operation":"compare","owner":"o","repo":"r","base":"a","head":"b","page":null,"filePage":1,"pageSize":30})),
        )
        .expect_err("null page must be rejected");
        assert!(
            buggy
                .issues
                .iter()
                .any(|issue| issue.path.iter().any(|p| p == "nextPage")),
            "expected a nextPage continuation issue, got {buggy:?}"
        );
        if let Err(fixed) = validate_output(
            "ghGetHistoryItem",
            &data(
                json!({"operation":"compare","owner":"o","repo":"r","base":"a","head":"b","page":1,"filePage":1,"pageSize":30}),
            ),
        ) {
            assert!(
                !fixed
                    .issues
                    .iter()
                    .any(|issue| issue.path.iter().any(|p| p == "page")),
                "compare nextPage query must keep page: {fixed:?}"
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
