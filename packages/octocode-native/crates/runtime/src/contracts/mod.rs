//! Generated public tool contracts and transport-neutral input preparation.

pub(crate) mod generated;
pub(crate) mod listing_roles;
mod prepare;
pub mod query_limits;
mod schema_facts;
pub(crate) mod shared_fields;
#[cfg(test)]
mod undeclared;
mod validate;
use octocode_contract_types::ErrorCode;
pub(crate) use validate::{levenshtein, qualifier_terms};

use crate::id::ToolId;
pub(crate) use prepare::{ENVELOPE_ARRAY, ENVELOPE_NESTED};
pub(crate) use schema_facts::{
    item_schemas, property_schemas, query_declares, query_schema_max, query_schema_number,
    query_schema_value, resolve_ref, restorable_fields,
};
pub use validate::{
    ContractValidationError, ValidationIssue, format_input_error, tool_error, validate,
    validate_output, validate_query,
};
pub(crate) use validate::{normalize_input, repair_input, validate_output_in_place};

/// Embedded contracts are immutable across runtime handles and requests.
pub fn parsed_contract() -> Result<&'static serde_json::Value, &'static str> {
    CONTRACT
        .get_or_init(|| serde_json::from_str(contract_json()).map_err(|error| error.to_string()))
        .as_ref()
        .map_err(String::as_str)
}

static CONTRACT: std::sync::OnceLock<Result<serde_json::Value, String>> =
    std::sync::OnceLock::new();

/// A recovery notice of classified results, authored once in core
/// (`classificationNotices.<key>`); empty only if the embedded contract
/// lacks it, which the contract tests rule out.
pub(crate) fn classification_notice(key: &str) -> &'static str {
    parsed_contract()
        .ok()
        .and_then(|contract| contract.pointer(&format!("/classificationNotices/{key}")))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
}

/// One tool's contract entry (`tools[i]`), parsed on first use from its
/// build-verified span of the embedded contract. A tool call touches one tool,
/// so a fresh process parses that entry (~100 KB) instead of the whole
/// multi-megabyte contract — the dominant cost of a short-lived CLI call.
/// Reuses the full parse when something (catalog, scheme) already forced it.
pub(crate) fn tool_contract(tool: ToolId) -> Result<&'static serde_json::Value, &'static str> {
    static TOOLS: [std::sync::OnceLock<Result<serde_json::Value, String>>; ToolId::ALL.len()] =
        [const { std::sync::OnceLock::new() }; ToolId::ALL.len()];
    let index = tool.index();
    if let Some(Ok(contract)) = CONTRACT.get() {
        return contract["tools"]
            .get(index)
            .ok_or("tool is not in the embedded contract");
    }
    TOOLS[index]
        .get_or_init(|| {
            let (start, end) = generated::CONTRACT_TOOL_SPANS[index];
            contract_json()
                .get(start..end)
                .ok_or_else(|| "tool span is outside the embedded contract".to_owned())
                .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()))
        })
        .as_ref()
        .map_err(String::as_str)
}

/// [`tool_contract`] by wire name; `None` for a name the contract lacks.
pub(crate) fn tool_contract_named(
    name: &str,
) -> Option<Result<&'static serde_json::Value, &'static str>> {
    ToolId::from_name(name).map(tool_contract)
}

/// Replace contract-violating result rows with row-level
/// `outputContractViolation` errors so one drifting emitter cannot discard a
/// batch's healthy rows. Returns the patched envelope only when every issue
/// maps to a result row and the patched envelope itself validates; envelope-
/// level violations return `None` and the caller keeps the whole-call failure.
pub(crate) fn isolate_row_violations(
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
            "errorCode": ErrorCode::OutputContractViolation,
        });
    }
    validate_output(tool_name, &patched).ok()?;
    Some(patched)
}

/// Validate one test query: a bare row or a one-row envelope, through the
/// same path every call takes ([`normalize_input`], then
/// [`prepare_many_and_validate`]).
#[cfg(test)]
pub(crate) fn prepare_and_validate(
    tool_name: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, ContractValidationError> {
    let mut rows = prepare_many_and_validate(tool_name, normalize_input(tool_name, input))?;
    Ok(rows.pop().unwrap_or(serde_json::Value::Null))
}

/// One row of a validated batch: its validated query, or its own issues.
pub(crate) type RowValidation = Result<serde_json::Value, ContractValidationError>;

/// Validate a fresh-query envelope once and isolate its invalid rows, so
/// valid rows can still execute. Rejected rows carry issue paths at their
/// original index. Isolation does not apply, and the whole call fails with
/// every issue, for a malformed envelope shape, a single-row input, clasify
/// (matrices share batch-level rules), every row invalid, or an envelope
/// issue that persists over the valid rows alone.
pub(crate) fn prepare_rows(
    tool_name: &str,
    input: serde_json::Value,
) -> Result<Vec<RowValidation>, ContractValidationError> {
    let issues = prepare::envelope_issues(tool_name, &input);
    if !issues.is_empty() {
        return Err(ContractValidationError { issues });
    }
    let (mut validated, batch) = validate::validate_batch(tool_name, input)?;
    let rows = match validated.get_mut("queries") {
        Some(serde_json::Value::Array(rows)) => std::mem::take(rows),
        _ => Vec::new(),
    };
    let failed = batch
        .rows
        .iter()
        .filter(|issues| !issues.is_empty())
        .count();
    let whole = |batch: validate::BatchIssues| {
        let mut issues: Vec<_> = batch
            .rows
            .into_iter()
            .flatten()
            .chain(batch.envelope)
            .collect();
        issues.dedup();
        ContractValidationError { issues }
    };
    if failed == 0 && batch.envelope.is_empty() {
        return Ok(rows.into_iter().map(Ok).collect());
    }
    if failed == 0 || failed == rows.len() || tool_name == ToolId::Clasify.as_str() {
        return Err(whole(batch));
    }
    // An envelope issue may be the invalid rows' own (the batch size): check
    // the envelope again over the valid rows alone.
    if !batch.envelope.is_empty() {
        let valid = rows
            .iter()
            .zip(&batch.rows)
            .filter(|(_, issues)| issues.is_empty())
            .map(|(row, _)| row.clone())
            .collect();
        validated["queries"] = serde_json::Value::Array(valid);
        if validate::validate_envelope(tool_name, &mut validated).is_err() {
            return Err(whole(batch));
        }
    }
    Ok(rows
        .into_iter()
        .zip(batch.rows)
        .enumerate()
        .map(|(index, (row, mut issues))| {
            if issues.is_empty() {
                return Ok(row);
            }
            for issue in &mut issues {
                if issue.path.first().map(String::as_str) != Some("queries") {
                    issue
                        .path
                        .splice(0..0, ["queries".to_owned(), index.to_string()]);
                }
            }
            Err(ContractValidationError { issues })
        })
        .collect())
}

/// Validate a fresh-query or saved-response envelope and return its query rows.
/// Saved-response reads have no query rows. [`normalize_input`] wraps a bare
/// fresh row into an envelope first. The envelope's shape
/// is checked first (`prepare::envelope_issues`: a bare array or a nested
/// envelope is rejected with a repairable issue, never reshaped); then the
/// complete envelope validates, so cross-query limits and indexed
/// diagnostics remain contract-owned. Clasify's batch relation rules run
/// at engine admission (`tools::clasify::admission`).
pub fn prepare_many_and_validate(
    tool_name: &str,
    input: serde_json::Value,
) -> Result<Vec<serde_json::Value>, ContractValidationError> {
    let issues = prepare::envelope_issues(tool_name, &input);
    if !issues.is_empty() {
        return Err(ContractValidationError { issues });
    }
    match validate(tool_name, input)? {
        serde_json::Value::Object(mut validated) => match validated.remove("queries") {
            Some(serde_json::Value::Array(queries)) => Ok(queries),
            _ => Ok(Vec::new()),
        },
        _ => Ok(Vec::new()),
    }
}

/// Fingerprint of the tool contract `@octocodeai/config` generated from core.
#[cfg(test)]
const fn contract_fingerprint() -> &'static str {
    generated::CONTRACT_FINGERPRINT
}

/// Canonical generated contract JSON used by adapters for registration.
#[must_use]
pub const fn contract_json() -> &'static str {
    generated::CONTRACT_JSON
}

/// CLI discovery content (`schema` views, `<tool> --help` sections), parsed
/// on first use. Built from the same contract; the build checks the fingerprint.
pub fn cli_presentation() -> Result<&'static serde_json::Value, &'static str> {
    static PRESENTATION: std::sync::OnceLock<Result<serde_json::Value, String>> =
        std::sync::OnceLock::new();
    PRESENTATION
        .get_or_init(|| {
            serde_json::from_str(generated::CLI_PRESENTATION_JSON)
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(String::as_str)
}

/// Each tool's `name`, `cli` and `selection.cli` from the CLI presentation
/// (an array in presentation order), parsed on first use: what building the
/// CLI's tool flags reads, without the presentation's `schema` views.
pub fn cli_tool_flags() -> Result<&'static serde_json::Value, &'static str> {
    static FLAGS: std::sync::OnceLock<Result<serde_json::Value, String>> =
        std::sync::OnceLock::new();
    FLAGS
        .get_or_init(|| {
            serde_json::from_str(generated::CLI_TOOL_FLAGS_JSON).map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(String::as_str)
}

/// The `<tool> --help` section of the CLI presentation tool `name`.
#[must_use]
pub fn cli_tool_help(name: &str) -> Option<&'static str> {
    generated::CLI_TOOL_HELP
        .iter()
        .find(|(tool, _)| *tool == name)
        .map(|(_, help)| *help)
}

/// Provenance written by `@octocodeai/config`: core package version, contract
/// fingerprint, and the digest of the embedded contract bytes.
#[cfg(test)]
const fn contract_provenance_json() -> &'static str {
    generated::CONTRACT_PROVENANCE_JSON
}

#[cfg(test)]
mod contract_owner_tests {
    use super::{
        contract_json, contract_provenance_json, isolate_row_violations, normalize_input,
        prepare_and_validate, prepare_many_and_validate, validate_output,
    };
    use serde_json::json;

    /// The flags and help parts the build cuts from the CLI presentation
    /// are exactly its tools' `cli`, `selection.cli` and `help`.
    #[test]
    fn cli_tool_parts_match_the_presentation() {
        let presentation = super::cli_presentation().expect("presentation");
        let tools = presentation["tools"].as_array().expect("tools");
        let flags = super::cli_tool_flags().expect("flags");
        assert_eq!(flags.as_array().map(Vec::len), Some(tools.len()));
        for (tool, entry) in tools.iter().zip(flags.as_array().into_iter().flatten()) {
            let name = tool["name"].as_str().expect("name");
            assert_eq!(entry["name"], name);
            assert_eq!(entry["cli"], tool["cli"], "{name} cli");
            assert_eq!(
                entry["selection"]["cli"], tool["selection"]["cli"],
                "{name} selection.cli"
            );
            assert_eq!(
                super::cli_tool_help(name),
                tool["help"].as_str(),
                "{name} help"
            );
        }
        assert_eq!(super::cli_tool_help("nope"), None);
    }

    /// Every classification notice native renders is authored in the
    /// embedded contract.
    #[test]
    fn every_classification_notice_comes_from_the_contract() {
        for key in ["failedWithRead", "failed", "imageInspection"] {
            assert!(
                !super::classification_notice(key).is_empty(),
                "classificationNotices.{key}"
            );
        }
    }

    /// Normalization only wraps a bare row as a one-row `queries`; it never
    /// repairs a value (an encoded `queries` string, a string number), which
    /// validation then rejects.
    #[test]
    fn normalization_only_wraps_a_bare_row() {
        let row = json!({"mainGoal":"g","reasoning":"r","path":"/tmp","matchString":"x","include":["*.ts"]});
        let expected = json!({"queries":[row.clone()]});
        let mut bare = row.clone();
        bare["responseLength"] = json!(100);
        let wrapped = normalize_input("localSearch", bare);
        assert_eq!(wrapped["queries"], expected["queries"]);
        assert_eq!(wrapped["responseLength"], 100);
        let prepared = prepare_many_and_validate("localSearch", wrapped)
            .expect("a bare row validates as one row");
        assert_eq!(prepared.len(), 1);
        let encoded = serde_json::to_string(&json!([row])).expect("encode");
        let envelope = json!({"queries": encoded, "responseLength": "100"});
        assert_eq!(normalize_input("localSearch", envelope.clone()), envelope);
        assert!(prepare_many_and_validate("localSearch", envelope).is_err());
        // An array, or an object with only envelope fields, is not a row.
        assert_eq!(
            normalize_input("localSearch", json!([row.clone()])),
            json!([row])
        );
        let only_envelope = json!({"responseLength": 100});
        assert_eq!(
            normalize_input("localSearch", only_envelope.clone()),
            only_envelope
        );
        let error = prepare_many_and_validate("localSearch", json!([row]))
            .expect_err("an array is not the envelope");
        assert!(
            error.issues[0].message.contains("{\"queries\":[...]}"),
            "{error:?}"
        );
        // Not validated or defaulted: invalid rows and empty input pass through.
        let invalid = json!({"queries":[{"bogus":1}]});
        assert_eq!(normalize_input("localSearch", invalid.clone()), invalid);
        assert_eq!(normalize_input("localSearch", json!({})), json!({}));
    }

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
    fn public_queries_take_an_optional_brief_and_drop_a_blank_one() {
        let omitted = prepare_and_validate("localFetch", json!({"path":"/tmp/source.rs"}))
            .expect("a call without a brief is accepted");
        assert!(omitted.get("mainGoal").is_none() && omitted.get("reasoning").is_none());
        let blank = prepare_and_validate(
            "localFetch",
            json!({"path":"/tmp/source.rs","mainGoal":"   ","reasoning":"   "}),
        )
        .expect("a blank brief is dropped, not rejected");
        assert!(blank.get("mainGoal").is_none() && blank.get("reasoning").is_none());
        let error = prepare_and_validate(
            "localFetch",
            json!({"path":"/tmp/source.rs","goal":" Read the source. "}),
        )
        .expect_err("goal is an unknown field, not a second name for mainGoal");
        assert_eq!(error.issues[0].rule_id, "schema.unknown-field");
        let ok = prepare_and_validate(
            "localFetch",
            json!({"path":"/tmp/source.rs","mainGoal":" Read the source. ","reasoning":" The next step needs these lines. "}),
        )
        .expect("a trimmed brief is accepted");
        assert_eq!(ok["mainGoal"], "Read the source.");
        assert_eq!(ok["reasoning"], "The next step needs these lines.");
    }

    #[test]
    fn bulk_queries_are_defaulted_and_validated_with_stable_order() {
        let queries = prepare_many_and_validate(
            "localFetch",
            json!({"queries":[
                {"path":"/tmp/a","mainGoal": "test", "reasoning":"Read both."},
                {"path":"/tmp/b","mainGoal": "test", "reasoning":"Read both."}
            ]}),
        )
        .expect("valid bulk input");
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0]["path"], "/tmp/a");
        assert_eq!(queries[1]["path"], "/tmp/b");
        assert!(queries[0].get("extras").is_none());
        assert!(queries[1].get("extras").is_none());
        let split = prepare_many_and_validate(
            "localFetch",
            json!({"queries":[
                {"path":"/tmp/a","mainGoal":"Find the writer.","reasoning":"Read both."},
                {"path":"/tmp/b","mainGoal":"Find the caller.","reasoning":"Read one."}
            ]}),
        )
        .expect("each batch row carries its own goal and reasoning");
        assert_eq!(split[0]["mainGoal"], "Find the writer.");
        assert_eq!(split[1]["mainGoal"], "Find the caller.");
        assert_eq!(split[1]["reasoning"], "Read one.");
    }

    #[test]
    fn a_clasify_row_keeps_its_keyed_questions_and_trims_its_brief() {
        let queries = prepare_many_and_validate(
            "clasify",
            json!({"queries":[{
                "id":"row",
                "reasoning":"  Classify the state.  ","mainGoal":"Decide the next read.",
                "state":{"text":"one"},
                "questions":{
                    "q1":{"type":"noul","instructions":"Is it relevant?"},
                    "q2":{"type":"score","instructions":"Rate risk","criteria":["low","high"]}
                }
            }]}),
        )
        .expect("valid clasify row");
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0]["state"], json!({"text":"one"}));
        assert_eq!(queries[0]["questions"]["q1"]["type"], "noul");
        assert_eq!(
            queries[0]["questions"]["q2"]["criteria"],
            json!(["low", "high"])
        );
        assert_eq!(queries[0]["reasoning"], "Classify the state.");
    }

    #[test]
    fn syntax_operation_is_rejected_without_core_alias() {
        assert!(
            prepare_and_validate(
                "astSearch",
                json!({"operation":"syntax","path":"/tmp/lib.rs"}),
            )
            .is_err()
        );
    }

    #[test]
    fn each_tool_span_parses_to_its_contract_entry() {
        // `tool_contract` parses one tool from its span when the full contract
        // is not yet parsed; every span must yield exactly `tools[i]`.
        let contract = super::parsed_contract().expect("generated contract");
        let tools = contract["tools"].as_array().expect("tool array");
        assert_eq!(super::generated::CONTRACT_TOOL_SPANS.len(), tools.len());
        for ((start, end), (tool, id)) in super::generated::CONTRACT_TOOL_SPANS
            .iter()
            .zip(tools.iter().zip(super::ToolId::ALL))
        {
            let parsed: serde_json::Value =
                serde_json::from_str(&contract_json()[*start..*end]).expect("span JSON");
            assert_eq!(&parsed, tool, "{id}");
            assert_eq!(super::tool_contract(id).expect("tool contract"), tool);
            assert_eq!(
                super::tool_contract_named(id.as_str())
                    .expect("known tool")
                    .expect("tool contract"),
                tool
            );
        }
        assert!(super::tool_contract_named("nope").is_none());
    }

    #[test]
    fn embedded_contract_is_compact_json() {
        // Every artifact embeds the contract, so the build re-serializes it
        // compactly (the source is pretty-printed at 3.4x the size).
        let text = contract_json();
        assert!(!text.contains('\n'), "embedded contract is pretty-printed");
        // Canonical Zod constraints and per-candidate failure receipts: 2,572,176 bytes.
        assert!(
            text.len() < 2_573_000,
            "embedded contract is {} bytes",
            text.len()
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
    fn embedded_contract_is_the_unmodified_config_output() {
        // octocode-config records the digest of the contract it generated; a
        // hand edit to the embedded bytes breaks the match. No native-side pin
        // needs updating on regeneration.
        // The embed is compact, so the digest is taken over the generated source
        // bytes; the embedded text must be the same JSON document.
        use sha2::{Digest, Sha256};
        const SOURCE: &str =
            include_str!("../../../../../octocode-config/contract/tool-contract.json");
        let provenance: serde_json::Value =
            serde_json::from_str(contract_provenance_json()).expect("provenance JSON");
        assert_eq!(provenance["sourcePackage"], "@octocodeai/octocode-core");
        assert_eq!(
            provenance["contractFingerprint"],
            super::contract_fingerprint()
        );
        assert_eq!(
            provenance["contractSha256"],
            hex::encode(Sha256::digest(SOURCE.as_bytes())),
            "generated source differs from what octocode-config recorded"
        );
        let source: serde_json::Value = serde_json::from_str(SOURCE).expect("source JSON");
        let embedded: serde_json::Value =
            serde_json::from_str(contract_json()).expect("embedded JSON");
        assert_eq!(
            embedded, source,
            "embedded contract differs from its source"
        );
    }

    #[test]
    fn public_response_and_tree_limits_are_pinned_against_silent_drift() {
        // Provenance records the core revision and generated fingerprint, but
        // both move with regeneration. It does not pin intended numeric bounds:
        // a core change that relaxes a public limit passes provenance checks.
        // This pins the two limits that also surface in the live MCP schema so any
        // change is a visible, reviewed test diff.
        fn collect(value: &serde_json::Value, response: &mut Vec<u64>, tree_depth: &mut Vec<u64>) {
            match value {
                serde_json::Value::Object(map) => {
                    if let Some(max) = map
                        .get("responseLength")
                        .and_then(|schema| schema.get("maximum"))
                        .and_then(serde_json::Value::as_u64)
                    {
                        response.push(max);
                    }
                    if let Some(max) = map
                        .get("maxDepth")
                        .and_then(|schema| schema.get("maximum"))
                        .and_then(serde_json::Value::as_u64)
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
            "expected at least one responseLength bound in the contract"
        );
        assert!(
            response_maxima.iter().all(|max| *max == 50_000),
            "responseLength.maximum drifted from 50000: {response_maxima:?}"
        );
        assert!(
            !tree_depth_maxima.is_empty(),
            "expected the tree-recursion maxDepth bound in the contract"
        );
        assert!(
            tree_depth_maxima.iter().all(|max| *max == 20),
            "public maxDepth bounds drifted from 20: {tree_depth_maxima:?}"
        );
    }

    #[test]
    fn output_contract_rejects_rows_without_an_index() {
        let error = validate_output("localSearch", &json!({"results":[{"data":{"files":[]}}]}))
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
            json!({"index":0,"data":{"files":[]}}),
            json!({
                "index":0,
                "meta":{"evidence":{"kind":"lexical","confidence":"medium"}},
                "data":{"files":[]}
            }),
        ] {
            validate_output("localSearch", &json!({"results":[row]}))
                .expect("canonical result envelope");
        }
    }

    #[test]
    fn output_contract_validates_shared_evidence_without_mutating_compact_output() {
        let (tool, data) = (
            "ghSearchHistory",
            json!({"commits":[
                {"commitSha":"abc123","date":"2026-01-01T00:00:00Z","message":"first"},
                {"commitSha":"def456","date":"2026-01-01T00:00:00Z","message":"second"}
            ]}),
        );
        let output = crate::response::rows::envelope(
            crate::id::ToolId::GhSearchHistory,
            vec![json!({"index":0,"data":data})],
        );
        assert!(
            output["shared"].is_object(),
            "test must exercise real compaction"
        );
        let original = output.clone();
        validate_output(tool, &output).expect("shared evidence satisfies canonical schema");
        assert_eq!(
            output, original,
            "validation must preserve compact wire output"
        );
    }

    #[test]
    fn output_contract_rejects_missing_or_invalid_shared_file_evidence() {
        let output = json!({"results":[{"index":0,"data":{"commits":[{"message":"first"}]}}]});
        validate_output("ghSearchHistory", &output).expect_err("a commit row needs its commitSha");
        let mut invalid = output.clone();
        invalid["shared"] = json!({"commitSha":42});
        validate_output("ghSearchHistory", &invalid).expect_err("shared commitSha must be text");
        invalid["shared"] = json!({"commitSha":"abc123"});
        invalid["results"][0]["data"]["commits"][0]["commitSha"] = json!(42);
        validate_output("ghSearchHistory", &invalid)
            .expect_err("shared defaults must not conceal malformed explicit evidence");
    }

    #[test]
    fn output_contract_isolates_invalid_rows_with_shared_evidence() {
        let output = json!({"shared":{"date":"2026-01-01T00:00:00Z"},"results":[
            {"index":0,"data":{"commits":[{"commitSha":"abc123","message":"first"}]}},
            {"index":1,"data":{"commits":[{"message":"second","commitSha":42}]}}
        ]});
        let error = validate_output("ghSearchHistory", &output).expect_err("second row is invalid");
        let isolated = isolate_row_violations("ghSearchHistory", &output, &error)
            .expect("valid compressed row survives isolation");
        assert_eq!(isolated["results"][0], output["results"][0]);
        assert_eq!(
            isolated["results"][1]["data"]["errorCode"],
            "outputContractViolation"
        );
    }

    #[test]
    fn history_output_accepts_pull_request_optional_content_actions() {
        let mut output = json!({"results":[{
            "index":0,
            "status":"empty",
            "data":{
                "pullRequests":[{
                    "number":463,
                    "title":"Example",
                    "state":"merged",
                    "author":"octocode",
                    "createdAt":"2026-08-07T20:37:26Z",
                    "hints":{
                        "readBody":{
                            "tool":"ghGetHistoryItem",
                            "confidence":"exact",
                            "query":{"queries":[{
                                "operation":"pullRequest",
                                "owner":"octocodeai",
                                "repo":"octocode",
                                "number":463,
                                "sections":["body"],
                                "mainGoal": "test", "reasoning":"Read the optional body.",
                                "extras":["details"]
                            }]}
                        }
                    }
                }],
                "hints":{"text":["Choose a changed path first."]}
            }
        }]});
        crate::response::channels::publish_hints(&mut output, crate::id::ToolId::GhGetHistoryItem);
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
                "artifacts":[],
                "pagination":{"pageSize":5,"hasMore":true,"totalItems":100},
                "next":{"nextPage":{"tool":"artifactSearch","query":{"queries":[query]},"confidence":"exact"}}
            }}]})
        };
        let buggy = validate_output(
            "artifactSearch",
            &data(
                json!({"ecosystem":"npm","packageName":null,"registryUrl":null,
                "keywords":["x"],"pageSize":5,"page":2}),
            ),
        )
        .expect_err("null packageName/registryUrl must be rejected");
        assert!(
            buggy.issues.iter().any(|issue| issue
                .path
                .iter()
                .any(|part| part == "packageName" || part == "registryUrl")),
            "expected a packageName/registryUrl issue, got {buggy:?}"
        );
        validate_output(
            "artifactSearch",
            &data(json!({"ecosystem":"npm","keywords":["x"],"pageSize":5,"page":2})),
        )
        .expect("a keyword page continuation is valid");
    }

    #[test]
    fn ghsearchcode_advisory_next_action_accepts_defaults_but_requires_repo() {
        // Continuation validation applies query defaults on a clone, so
        // page/pageSize/debug may be omitted. ghStructure's owner/repo are
        // genuinely required and keep the follow-up executable.
        let data = |mut query: serde_json::Value| {
            let object = query.as_object_mut().unwrap();
            object
                .entry("mainGoal")
                .or_insert_with(|| json!("Find the repository."));
            object
                .entry("reasoning")
                .or_insert_with(|| json!("Verify the scoped repository exists."));
            json!({"results":[{"index":0,"data":{
                "files":[],
                "next":{"viewTree":{"tool":"ghStructure","query":{"queries":[query]},
                    "confidence":"exact","why":"Verify structure."}}
            }}]})
        };
        validate_output(
            "ghSearchCode",
            &data(json!({"owner":"o","repo":"r","path":""})),
        )
        .expect("defaulted pagination fields may be omitted");
        let invalid = validate_output("ghSearchCode", &data(json!({"owner":"o","path":""})))
            .expect_err("missing repo must be rejected");
        assert!(
            invalid
                .issues
                .iter()
                .any(|issue| issue.path.iter().any(|part| part == "repo")),
            "expected a repo issue, got {invalid:?}"
        );
    }

    #[test]
    fn filecontent_viewtree_accepts_defaults_but_requires_repo() {
        // Tree pagination/debug fields are defaulted during validation; the
        // repository identity is still required.
        let data = |mut query: serde_json::Value| {
            let object = query.as_object_mut().unwrap();
            object
                .entry("mainGoal")
                .or_insert_with(|| json!("Read the repository tree."));
            object
                .entry("reasoning")
                .or_insert_with(|| json!("Continue the paged read."));
            json!({"results":[{"index":0,"data":{
                "owner":"o","repo":"r","path":"missing.md","error":"not found",
                "next":{"viewTree":{"tool":"ghStructure","query":{"queries":[query]},
                    "confidence":"low"}}
            }}]})
        };
        validate_output(
            "ghGetFileContent",
            &data(json!({"owner":"o","repo":"r","path":"."})),
        )
        .expect("defaulted tree pagination fields may be omitted");
        let invalid = validate_output("ghGetFileContent", &data(json!({"owner":"o","path":"."})))
            .expect_err("missing repo must be rejected");
        assert!(
            invalid
                .issues
                .iter()
                .any(|issue| issue.path.iter().any(|part| part == "repo")),
            "expected a repo issue, got {invalid:?}"
        );
    }

    #[test]
    fn ghsearchhistory_nextpage_requires_serialized_defaults_and_operation() {
        // Executable output continuations use the parsed query shape, so the
        // defaulted pageSize is serialized and operation remains required.
        let data = |mut query: serde_json::Value| {
            let object = query.as_object_mut().unwrap();
            object
                .entry("mainGoal")
                .or_insert_with(|| json!("Find the history page."));
            object
                .entry("reasoning")
                .or_insert_with(|| json!("Read the next history page."));
            json!({"results":[{"index":0,"data":{
                "pullRequests":[],
                "next":{"nextPage":{"tool":"ghSearchHistory","query":{"queries":[query]},"confidence":"exact"}}
            }}]})
        };
        validate_output(
            "ghSearchHistory",
            &data(json!({"operation":"pullRequest","owner":"o","repo":"r","page":2,"pageSize":30})),
        )
        .expect("serialized history defaults are executable");
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
                "next":{"nextPage":{"tool":"ghGetHistoryItem","query":{"queries":[query]},"confidence":"exact"}}
            }}]})
        };
        let buggy = validate_output(
            "ghGetHistoryItem",
            &data(json!({"operation":"compare","owner":"o","repo":"r","base":"a","head":"b","page":null,"pageSize":30})),
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
                json!({"operation":"compare","owner":"o","repo":"r","base":"a","head":"b","page":1,"pageSize":30}),
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
            "ghStructure",
            json!({
                "owner": "a",
                "repo": "b",
                "materialize": true,
                "materializeOffset": 12,
                "mainGoal": "test", "reasoning": "Exercise materialized tree validation."
            }),
        )
        .expect("materialize fields are in the generated tree contract");
        assert_eq!(query["materialize"], true);
        assert_eq!(query["materializeOffset"], 12);
    }

    /// Schema single-source guard: assert that no `.rs` source file defines
    /// inline JSON Schema vocabulary (`"$schema"`, `"inputSchema"` as an object
    /// key inside a `json!()` macro call, or `"properties"` as an *lvalue* in a
    /// JSON literal). Schemas are embedded from `@octocodeai/config`'s
    /// generated `contract/`; this scan catches accidental copy-paste of schema
    /// fragments into tool runners.
    ///
    /// Patterns checked (as substrings in non-comment, non-test lines):
    ///   - `json!({"$schema":` — top-level JSON Schema declaration
    ///   - `json!({"inputSchema":` — MCP tool registration schema inline
    ///   - `"inputSchema": {` — same, written as a field expression
    #[test]
    fn no_inline_schema_literals_outside_generated_contracts() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let src_dir = manifest_dir.join("src");
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
        scan_for_schema_literals(&src_dir, forbidden, &mut violations);

        assert!(
            violations.is_empty(),
            "Hand-authored JSON Schema literals found in native sources \
             — move them to octocode-core and run `yarn contracts:regen`:\n{}",
            violations.join("\n")
        );
    }

    fn scan_for_schema_literals(
        dir: &std::path::Path,
        patterns: &[&str],
        violations: &mut Vec<String>,
    ) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_for_schema_literals(&path, patterns, violations);
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
