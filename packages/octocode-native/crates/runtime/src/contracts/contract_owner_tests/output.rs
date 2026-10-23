use super::*;

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
