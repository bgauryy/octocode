use super::*;

#[test]
fn result_rows_require_stable_indices_and_preserve_optional_diagnostics() {
    let data = json!({"files":[{"path":"src/lib.rs"}]});
    validate_output("localSearch", &json!({"results":[{"data":data}]}))
        .expect_err("row correlation is required");
    for row in [
        json!({"index":0,"data":data}),
        json!({"index":0,"data":data,"meta":{"evidence":{"kind":"lexical","confidence":"high"}}}),
    ] {
        let output = json!({"results":[row]});
        validate_output("localSearch", &output).expect("canonical result envelope");
    }
}

#[test]
fn equal_evidence_stays_on_its_rows_instead_of_moving_to_a_shared_alias() {
    let data = json!({"commits":[
        {"commitSha":"abc123","date":"2026-01-01T00:00:00Z","messageHeadline":"first"},
        {"commitSha":"def456","date":"2026-01-01T00:00:00Z","messageHeadline":"second"}
    ]});
    let rows = vec![json!({"index":0,"data":data})];
    let output = crate::response::rows::envelope(crate::id::ToolId::GhSearchHistory, rows.clone());
    validate_output("ghSearchHistory", &output).expect("complete evidence");
    assert_eq!(output["results"], json!(rows));
    assert!(output.get("shared").is_none());
}

#[test]
fn malformed_evidence_is_isolated_without_changing_successful_siblings() {
    let output = json!({"results":[
        {"index":0,"data":{"commits":[{"commitSha":"abc123","messageHeadline":"first"}]}},
        {"index":1,"data":{"commits":[{"messageHeadline":"second","commitSha":42}]}}
    ]});
    let error = validate_output("ghSearchHistory", &output).expect_err("second row is invalid");
    let isolated =
        isolate_row_violations("ghSearchHistory", &output, &error).expect("isolate invalid row");
    assert_eq!(isolated["results"][0], output["results"][0]);
    assert_eq!(
        isolated["results"][1]["data"]["errorCode"],
        "outputContractViolation"
    );
}

#[test]
fn executable_calls_require_explicit_kind_and_the_correct_request_form() {
    let continued =
        json!({"kind":"continue","tool":"localFetch","input":{"cursor":"issued-handle"}});
    let related =
        json!({"kind":"related","tool":"localFetch","input":{"queries":[{"path":"src/lib.rs"}]}});
    let data = json!({"path":"src/lib.rs","content":"fn main() {}","next":{"continueWindow":continued},"related":[related]});
    let output = |data| json!({"results":[{"index":0,"data":data}]});
    validate_output("localFetch", &output(data.clone())).expect("explicit channels");
    for call in [
        json!({"kind":"continue","tool":"localFetch","input":{"queries":[{"path":"src/lib.rs"}]}}),
        json!({"kind":"related","tool":"localFetch","input":{"cursor":"issued-handle"}}),
        json!({"tool":"localFetch","query":{"queries":[{"path":"src/lib.rs"}]}}),
    ] {
        let mut malformed = data.clone();
        malformed["next"]["continueWindow"] = call;
        validate_output("localFetch", &output(malformed))
            .expect_err("never infer or repair executable calls");
    }
}

#[test]
fn heritage_scan_rows_keep_their_label_under_the_lsp_contract() {
    // `implementation` lists the declarations a syntax scan found beside
    // the server's rows, each labelled `source: "native-graph-facts"`; the
    // label is contract vocabulary, so the whole answer is never withheld
    // as a violation because of it.
    let data = json!({
        "kind": "implementation",
        "lsp": {"serverAvailable": true, "provider": "implementationProvider"},
        "matches": [
            {"path": "src/calls.ts", "displayRange": {"startLine": 3, "startCharacter": 14, "endLine": 3},
             "content": "3\texport class SlowRunner implements Runner {\n"},
            {"path": "src/other.ts", "displayRange": {"startLine": 5, "startCharacter": 14, "endLine": 5},
             "content": "5\texport class Other implements Runner {\n", "source": "native-graph-facts"}
        ],
        "pagination": {"page": 1, "totalPages": 1, "totalItems": 2, "hasMore": false}
    });
    validate_output("lspSearch", &json!({"results":[{"index":0,"data":data}]}))
        .expect("a scan-labelled implementation row is valid output");
}
