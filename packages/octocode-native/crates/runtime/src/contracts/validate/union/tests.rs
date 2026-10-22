//! Union branch selection tests: the fast acceptance path against the
//! reference algorithm.

use crate::contracts::prepare_many_and_validate;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Runs `check` with the fast acceptance path and again with only the
/// reference algorithm, and returns both results.
fn both<T>(check: impl Fn() -> T) -> (T, T) {
    let fast = check();
    super::REFERENCE_ONLY.with(|flag| flag.set(true));
    let reference = check();
    super::REFERENCE_ONLY.with(|flag| flag.set(false));
    (fast, reference)
}

/// Output validation as `validate_output` runs it, keeping the
/// normalized copy so defaults chosen by union branches compare too.
fn validate_output_normalized(
    tool: &str,
    output: &Value,
) -> Result<Value, super::ContractValidationError> {
    let schema = &super::super::contract_tool(tool)?.1["outputSchema"];
    let mut candidate = output.clone();
    super::validate_schema(schema, schema, &mut candidate, &mut Vec::new())?;
    Ok(candidate)
}

/// The copy-free validation the response stage runs returns what
/// `validate_output` returns and leaves the response byte-identical.
fn assert_in_place_matches(tool: &str, output: &Value) {
    let mut in_place = output.clone();
    assert_eq!(
        crate::contracts::validate_output_in_place(tool, &mut in_place),
        crate::contracts::validate_output(tool, output),
        "{tool}: {output}"
    );
    assert_eq!(
        serde_json::to_string(&in_place).expect("json"),
        serde_json::to_string(output).expect("json"),
        "{tool}"
    );
}

fn fixtures() -> Vec<Value> {
    let fixtures: Value = serde_json::from_str(crate::contracts::generated::CONTRACT_FIXTURES_JSON)
        .expect("generated fixtures");
    fixtures.as_array().expect("fixture array").clone()
}

/// Each fixture query, then one variant per field: removed, a boolean,
/// an unknown literal, an object; plus an unknown field. The variants
/// reach every union failure diagnosis (selector mismatch, missing field,
/// mixed forms, widened literals, sibling annotations).
fn query_variants(query: &Value) -> Vec<Value> {
    let mut variants = vec![query.clone()];
    let Some(object) = query.as_object() else {
        return variants;
    };
    for key in object.keys() {
        let mut removed = object.clone();
        removed.remove(key);
        variants.push(Value::Object(removed));
        for replacement in [json!(true), json!("zz-not-a-value"), json!({}), json!(7)] {
            let mut changed = object.clone();
            changed.insert(key.clone(), replacement);
            variants.push(Value::Object(changed));
        }
    }
    let mut unknown = object.clone();
    unknown.insert("bogusField".into(), json!(1));
    variants.push(Value::Object(unknown));
    variants
}

/// The default run compares one in [`SAMPLE`] of the variant matrices
/// below (a fixed, rotating sample, so each fixture contributes and every
/// variant kind is reached across fixtures); the `*_full_matrix` tests
/// (ignored: `cargo test -- --ignored full_matrix`) compare all of it.
const SAMPLE: usize = 4;

/// Whether variant `index` of fixture `ordinal` is in a `stride` sample;
/// the unchanged query (variant 0) always is.
fn sampled(ordinal: usize, index: usize, stride: usize) -> bool {
    index == 0 || (index + ordinal).is_multiple_of(stride)
}

/// The discriminator-first acceptance path returns exactly what
/// the reference algorithm returns (same normalized value, same issues)
/// for every generated fixture and its field variants, as input.
fn compare_input_fixtures(stride: usize) {
    let mut compared = 0;
    let mut rejected = 0;
    for (ordinal, fixture) in fixtures().iter().enumerate() {
        let tool = fixture["tool"].as_str().expect("tool");
        let Some(query) = fixture["input"]["queries"].get(0) else {
            continue;
        };
        for (index, variant) in query_variants(query).into_iter().enumerate() {
            if !sampled(ordinal, index, stride) {
                continue;
            }
            let input = json!({"queries":[variant]});
            let (fast, reference) = both(|| prepare_many_and_validate(tool, input.clone()));
            assert_eq!(fast, reference, "{}: {input}", fixture["id"]);
            compared += 1;
            rejected += usize::from(fast.is_err());
        }
    }
    assert!(
        compared * stride > 1_000 && rejected * stride > 500,
        "{compared}/{rejected}"
    );
}

#[test]
fn fast_union_acceptance_matches_the_reference_on_sampled_input_fixtures() {
    compare_input_fixtures(SAMPLE);
}

#[test]
#[ignore = "full matrix: cargo test -- --ignored full_matrix"]
fn fast_union_acceptance_matches_the_reference_on_every_input_fixture_full_matrix() {
    compare_input_fixtures(1);
}

/// The same on output envelopes, where a row's continuation is the
/// 16-branch `ExecutableContinuation` union: every fixture query and a
/// sixth of its variants as a continuation in its tool's output, some
/// under a wrong tool name; each tool's first query in every output.
/// `stride` samples the variants further (see [`SAMPLE`]).
fn compare_continuation_outputs(stride: usize) {
    let tools: Vec<&str> = crate::contracts::parsed_contract().expect("contract")["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    let envelope = |continuation: Value| {
        json!({"results":[
            {"index":0,"data":{"path":"a.txt","content":"x\n","totalLines":1,
                "next":{"continueWindow":continuation.clone()}}},
            {"index":1,"status":"error","data":{"error":"Not found","errorCode":"notFound",
                "next":{"retry":continuation}}}
        ]})
    };
    // Collect every (output tool, envelope) pair, then compare them on all
    // cores: the reference algorithm is slow on this union, and each
    // worker's `REFERENCE_ONLY` flag is its own thread-local.
    let mut cases: Vec<(String, Value)> = Vec::new();
    let mut seen = Vec::new();
    for (ordinal, fixture) in fixtures().iter().enumerate() {
        let tool = fixture["tool"].as_str().expect("tool").to_owned();
        let Some(query) = fixture["input"]["queries"].get(0) else {
            continue;
        };
        let unchanged = envelope(json!({"tool":tool,"query":{"queries":[query]}}));
        let output_tools = if seen.contains(&tool) {
            vec![tool.as_str()]
        } else {
            tools.clone()
        };
        for (index, output_tool) in output_tools.into_iter().enumerate() {
            if output_tool == tool || sampled(ordinal, index, stride) {
                cases.push((output_tool.to_owned(), unchanged.clone()));
            }
        }
        for (index, variant) in query_variants(query).into_iter().enumerate().step_by(6) {
            if index > 0 && !sampled(ordinal, index / 6, stride) {
                continue;
            }
            let named = if index % 7 == 6 { "localFetch" } else { &tool };
            let output = envelope(json!({"tool":named,"query":{"queries":[variant]}}));
            cases.push((tool.clone(), output));
        }
        seen.push(tool);
    }
    let compare = |(output_tool, output): &(String, Value)| {
        let (fast, reference) = both(|| validate_output_normalized(output_tool, output));
        assert_eq!(fast, reference, "{output_tool}: {output}");
        assert_in_place_matches(output_tool, output);
        fast.is_err()
    };
    // Workers take the next case from one counter: a rejected envelope costs
    // up to a thousand times an accepted one and they cluster by fixture, so
    // fixed chunks kept one worker busy long after the others had finished.
    let next = AtomicUsize::new(0);
    let work = || {
        let mut rejected = 0;
        while let Some(case) = cases.get(next.fetch_add(1, Ordering::Relaxed)) {
            rejected += usize::from(compare(case));
        }
        rejected
    };
    let rejected: usize = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..crate::tools::workers::host_cores())
            .map(|_| scope.spawn(work))
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .sum()
    });
    let compared = cases.len();
    eprintln!("compared {compared} outputs, {rejected} rejected");
    assert!(
        compared * stride > 600 && rejected * stride > 200,
        "{compared}/{rejected}"
    );
}

#[test]
fn fast_union_acceptance_matches_the_reference_on_sampled_continuation_outputs() {
    compare_continuation_outputs(SAMPLE);
}

#[test]
#[ignore = "full matrix: cargo test -- --ignored full_matrix"]
fn fast_union_acceptance_matches_the_reference_on_continuation_outputs_full_matrix() {
    compare_continuation_outputs(1);
}

/// Recorded responses (`OCTOCODE_RECORDED_OUTPUTS`: a directory
/// of `<n>-<tool>-<surface>.json` envelopes captured from the live tools)
/// validate identically on both paths and in place, unchanged and with
/// each row's fields individually corrupted.
#[test]
#[ignore = "needs a recorded corpus: OCTOCODE_RECORDED_OUTPUTS=<dir>"]
fn fast_union_acceptance_matches_the_reference_on_recorded_outputs() {
    let dir = std::env::var("OCTOCODE_RECORDED_OUTPUTS").expect("OCTOCODE_RECORDED_OUTPUTS");
    let mut compared = 0;
    for entry in std::fs::read_dir(dir).expect("corpus dir") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("name")
            .to_owned();
        let Some(tool) = name.split('-').nth(1) else {
            continue;
        };
        let output: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
        let output = output.get("structuredContent").cloned().unwrap_or(output);
        if output.get("thrown").is_some() {
            continue;
        }
        crate::contracts::validate_output(tool, &output).unwrap_or_else(|error| {
            panic!("{name}: unmodified {tool} output was rejected: {error:?}")
        });
        let mut variants = vec![output.clone()];
        for (row_index, row) in output["results"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            for field in row["data"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(k, _)| k)
            {
                for replacement in [json!(true), json!({"tool":"nope"}), json!([1])] {
                    let mut changed = output.clone();
                    changed["results"][row_index]["data"][field] = replacement;
                    variants.push(changed);
                }
            }
        }
        for variant in variants {
            let (fast, reference) = both(|| validate_output_normalized(tool, &variant));
            assert_eq!(fast, reference, "{name}");
            assert_in_place_matches(tool, &variant);
            compared += 1;
        }
    }
    assert!(compared > 0);
    eprintln!("compared {compared} recorded outputs and variants");
}

#[test]
fn clasify_question_selectors_report_the_selected_forms_missing_field() {
    for (question, field) in [
        (
            json!({"type":"choice","instructions":"Choose a label"}),
            "criteria",
        ),
        (json!({"type":"score","instructions":"Rate it"}), "criteria"),
    ] {
        let error = prepare_many_and_validate(
            "clasify",
            json!({"queries":[{
                "id":"missing-question-field",
                "state":"Observed evidence",
                "questions":{"check":question}
            }]}),
        )
        .expect_err("the selected question lacks a required field");
        assert_eq!(error.issues.len(), 1, "{error:?}");
        assert_eq!(
            error.issues[0].path,
            ["queries", "0", "questions", "check", field]
        );
        assert_eq!(error.issues[0].rule_id, "schema.required");
    }
}

#[test]
fn clasify_noul_criteria_and_box_allow_missing_or_null_instructions() {
    for question in [
        json!({"type":"noul","criteria":{"true":"The state contains evidence."}}),
        json!({"type":"noul","instructions":null,"criteria":{"false":"The state is empty."}}),
        json!({"type":"box"}),
        json!({"type":"box","instructions":null}),
    ] {
        prepare_many_and_validate(
            "clasify",
            json!({"queries":[{
                "state":"Observed evidence",
                "questions":{"check":question}
            }]}),
        )
        .expect("Noul with criteria and Box do not require instructions");
    }
}

#[test]
fn clasify_noul_rejects_missing_instructions_and_unusable_criteria() {
    for question in [
        json!({"type":"noul"}),
        json!({"type":"noul","instructions":null,"criteria":null}),
        json!({"type":"noul","instructions":" ","criteria":{"true":[],"false":{}}}),
    ] {
        assert!(
            prepare_many_and_validate(
                "clasify",
                json!({"queries":[{
                    "state":"Observed evidence",
                    "questions":{"check":question}
                }]}),
            )
            .is_err(),
            "a Noul question needs a usable predicate: {question}"
        );
    }
}

#[test]
fn missing_literal_selector_names_its_value_without_changing_plain_requirements() {
    let root = json!({"$defs":{"mode":{"const":"match"}}});
    let branch = json!({
        "properties":{"operation":{"$ref":"#/$defs/mode"},"pattern":{"type":"string"}},
        "required":["operation","pattern"]
    });
    assert_eq!(
        super::sibling_missing_fields(&root, &[&branch], &json!({}), "payload"),
        Some(vec![json!("operation:\"match\" and pattern")])
    );
}

/// A field of the one form whose omitted selector is a single literal
/// carries the fix that sets it; a query that also carries another
/// form's fields (mixed forms) does not.
#[test]
fn a_field_of_one_unselected_form_carries_its_selector_as_the_fix() {
    let issues = |row: Value| {
        prepare_many_and_validate("structureSearch", json!({"queries":[row]}))
            .expect_err("nameRegex is a files field")
            .issues
    };
    let found = issues(json!({"path":".","nameRegex":"x"}));
    let schema = found[0].schema.as_ref().expect("annotated");
    assert_eq!(
        schema["setFields"],
        json!({"operation":"files"}),
        "{found:?}"
    );
    let explicit = issues(json!({"operation":"tree","path":".","nameRegex":"x"}));
    let schema = explicit[0].schema.as_ref().expect("annotated");
    assert!(schema.get("setFields").is_none(), "{explicit:?}");
    assert_eq!(schema["siblingConflict"], true, "{explicit:?}");
}

#[test]
fn section_fields_without_operation_name_the_sections_literal() {
    for (field, value) in [
        (
            "resources",
            json!({"file":{"tool":"localFetch","query":{"path":"a.rs"}}}),
        ),
        ("assessment", json!("opaque")),
    ] {
        let mut row = json!({"state":"evidence","questions":{"fact":{"type":"noul","instructions":"Supported?"}}});
        row[field] = value;
        let error = prepare_many_and_validate("clasify", json!({"queries":[row]}))
            .expect_err("section-only field on evaluation");
        let projected = crate::contracts::format_input_error("clasify", &error);
        assert_eq!(
            projected["error"],
            format!(
                "Remove '{field}' from queries[0]: it applies only with operation:\"sections\"."
            ),
            "{projected}"
        );
    }
}

/// A field that only another form's selector values accept names those
/// values, not "unknown field".
#[test]
fn a_field_of_another_selector_value_names_the_values_that_accept_it() {
    let error = prepare_many_and_validate(
        "artifactSearch",
        json!({"queries":[{"ecosystem":"pypi","keywords":["http"]}]}),
    )
    .expect_err("pypi has no keyword discovery");
    let projected = crate::contracts::format_input_error("artifactSearch", &error);
    assert!(
        projected["error"]
            .as_str()
            .is_some_and(|error| error.starts_with(
                "Remove 'keywords' from queries[0]: it applies only with ecosystem one of \
                 \"npm\", \"crates\""
            )),
        "{projected}"
    );
    assert!(
        !projected.to_string().contains("Unknown field"),
        "{projected}"
    );
}

/// An enum error lists the values of the branch the selector
/// chose, not every branch's merged enum (compare has no issue sections).
#[test]
fn enum_error_lists_only_the_selected_branch_values() {
    let error = prepare_many_and_validate(
        "ghGetHistoryItem",
        json!({"queries":[{
            "mainGoal":"g","reasoning":"r","operation":"compare",
            "owner":"o","repo":"r","base":"v1","head":"v2","sections":["bogus"]
        }]}),
    )
    .expect_err("bogus section");
    let message = error
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(message.contains("patches"), "{message}");
    for foreign in ["body", "comments", "reviews"] {
        assert!(!message.contains(foreign), "{foreign}: {message}");
    }
    // A selector that matches no branch still lists every branch's value.
    let error = prepare_many_and_validate(
            "ghGetHistoryItem",
            json!({"queries":[{"mainGoal":"g","reasoning":"r","operation":"bogus","owner":"o","repo":"r"}]}),
        )
        .expect_err("bogus operation");
    let message = error
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    for operation in ["pullRequest", "issue", "commit", "compare"] {
        assert!(message.contains(operation), "{operation}: {message}");
    }
}

fn clasify_issues(query: Value) -> Vec<super::ValidationIssue> {
    prepare_many_and_validate("clasify", json!({"queries":[query]}))
        .expect_err("invalid clasify query")
        .issues
}

#[test]
fn a_present_alternative_outranks_a_branch_that_forbids_it() {
    let schema = json!({"anyOf":[
        {"type":"object","required":["inline"],"properties":{
            "inline":{"type":"string"},"resource":{"not":{}}
        }},
        {"type":"object","required":["resource"],"properties":{
            "inline":{"not":{}},
            "resource":{"type":"object","required":["path","line","format"],"properties":{
                "path":{"type":"string"},"line":{"type":"integer"},"format":{"enum":["text"]}
            }}
        }}
    ]});
    let mut value = json!({"resource":{"path":false,"line":"wrong","format":"other"}});
    let error = super::validate_schema(&schema, &schema, &mut value, &mut Vec::new())
        .expect_err("the selected resource is malformed");
    assert_eq!(error.issues.len(), 3, "{error:?}");
    assert!(
        error.issues.iter().all(|issue| issue.path[0] == "resource"),
        "{error:?}"
    );
}

/// A branch missing its required `operation` literal is rejected like one
/// pinning it to another literal (core Zod: `invalid_value`), so the
/// default form's own errors lead.
#[test]
fn a_missing_required_selector_rejects_its_branch() {
    let issues = clasify_issues(json!({
        "questions":["q"],
        "source":{"tool":"ghGetFileContent","query":{"path":"a.ts","ranges":["1-2"]}}
    }));
    assert_eq!(issues[0].path, ["queries", "0", "questions"], "{issues:?}");
    assert!(
        !issues
            .iter()
            .any(|issue| issue.path.last().is_some_and(|field| field == "operation")),
        "{issues:?}"
    );
}

/// A wrong discriminator is the first repair, ahead of the fields of the
/// branch it was compared with.
#[test]
fn selector_issues_lead_the_chosen_branch() {
    let issues = clasify_issues(json!({
        "questions":{"check":{"type":"noul","instructions":"The state contains evidence."}},
        "source":{"tool":"astSearch","query":{"path":"a.ts"}}
    }));
    assert_eq!(
        issues[0].path,
        ["queries", "0", "source", "tool"],
        "{issues:?}"
    );
    assert_eq!(issues[0].rule_id, "schema.enum", "{issues:?}");
    for tool in ["localFetch", "ghGetFileContent"] {
        assert!(issues[0].message.contains(tool), "{tool}: {issues:?}");
    }
}

#[test]
fn forbidden_field_names_the_mutually_exclusive_fields() {
    let error = prepare_many_and_validate(
        "lspSearch",
        json!({"queries":[{
            "mainGoal":"g","reasoning":"r","operation":"documentSymbols",
            "path":"/tmp/a.rs","symbolName":"finish","line":3
        }]}),
    )
    .expect_err("a document query forbids symbolName/line");
    let message = error
        .issues
        .iter()
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(!message.contains("forbidden schema"), "{message}");
    assert!(message.contains("mutually exclusive"), "{message}");
    assert!(message.contains("`symbolName`"), "{message}");
    assert!(message.contains("`line`"), "{message}");
}
