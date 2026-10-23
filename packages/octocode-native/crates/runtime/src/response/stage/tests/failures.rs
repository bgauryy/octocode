use super::*;

#[test]
fn pager_failures_keep_their_kind() {
    assert_eq!(
        response_failure(ResponseError::Cancelled),
        ExecutionError::Cancelled
    );
}

#[test]
fn clasify_all_failed_needs_every_row_to_fail() {
    let failed = json!({"id":"a","error":{"errorCode":"pathNotFound","error":"gone"}});
    let answered = json!({"id":"b","answers":{"q":{"type":"noul","noul":0.9}}});
    assert!(response_all_failed(&json!({"queries":[failed.clone()]})));
    assert!(response_all_failed(
        &json!({"queries":[failed.clone(), failed.clone()]})
    ));
    assert!(!response_all_failed(
        &json!({"queries":[failed, answered.clone()]})
    ));
    assert!(!response_all_failed(&json!({"queries":[answered]})));
    assert!(!response_all_failed(&json!({"queries":[]})));
}

#[test]
fn rejected_batch_rows_stay_explicit_errors() {
    let rejected = json!({"index":1,"status":"error","data":{"error":"Check the query fields.","errorCode":"invalidInput"}});
    let outcome = stage("localFetch", json!({"results":[fetch_row(0), rejected]}));
    assert_eq!(outcome.structured_content["results"][1]["status"], "error");
    assert!(!outcome.all_failed);
}

#[test]
fn a_malformed_row_becomes_an_error_row_and_marks_failure() {
    let malformed = json!({"index":0,"data":{"path":7}});
    let outcome = stage("localFetch", json!({"results":[malformed]}));
    assert_eq!(outcome.structured_content["results"][0]["status"], "error");
    assert!(outcome.failure.is_some());
    assert!(outcome.all_failed);
}

#[test]
fn an_invalid_envelope_is_a_contract_error_not_a_repair() {
    let result = finish(
        input(
            ToolId::LocalFetch,
            json!({"results":"not-an-array"}),
            ResponsePageOptions::default(),
        ),
        &json!({}),
        &context(),
    )
    .expect("stage runs");
    assert!(result.is_err());
}

/// An explicitly paged response hashes its rows; a GitHub error row's
/// request id must not change that snapshot.
#[test]
fn row_pages_ignore_provider_request_ids() {
    let snapshot = |request_id: &str| {
        let rows = json!({"results":[
                {"index":0,"data":{"path":"a.txt","content":"one\n".repeat(200),"totalLines":200}},
                {"index":1,"status":"error","data":{"error":"Not found","errorCode":"notFound",
                    "httpStatus":404,"requestId":request_id,
                    "rateLimit":{"remaining":4999,"resetEpochSeconds":1_700_000_000}}}]});
        let outcome = finish(
                input(
                    ToolId::GhGetFileContent,
                    rows,
                    ResponsePageOptions {
                        response_length: Some(300),
                        ..Default::default()
                    },
                ),
                &json!({"queries":[{"owner":"o","repo":"r","path":"a.txt","mainGoal":"g","reasoning":"r"}]}),
                &context(),
            )
            .expect("stage runs")
            .expect("valid envelope");
        outcome.structured_content["responsePagination"]["next"]["query"]["responseSnapshot"]
            .clone()
    };
    let first = snapshot("360F:376635:800998:A22B33:6ABD2D5E");
    assert!(first.is_string(), "{first}");
    assert_eq!(first, snapshot("47F4:376635:800A3C:A22BF9:6ABD2D60"));
}

/// Structured windows are slices of the envelope's exact JSON: joined in
/// order, they parse back to the envelope.
#[test]
fn structured_windows_join_into_the_envelope_json() {
    let content = "x".repeat(3_000);
    let structured =
        json!({"results":[{"index":0,"data":{"path":"a.txt","content":content,"totalLines":1}}]});
    let mut whole = String::new();
    // Saved as the runtime saves it: its key is a digest, never the
    // all-zero key of a bare pager run (which reads as a token).
    let mut options = ResponsePageOptions {
        response_length: Some(1_000),
        response_scope: Some(PageScope::Structured),
        saved: Some(crate::response::page_model::SavedResponse {
            key: [7; 32],
            expires_at: String::new(),
            storage: "memory",
            replayed: false,
        }),
        ..ResponsePageOptions::default()
    };
    loop {
        let page = finish(
            input(ToolId::LocalFetch, structured.clone(), options.clone()),
            &json!({"path":"/tmp/a.txt"}),
            &context(),
        )
        .expect("stage runs")
        .expect("valid envelope")
        .structured_content;
        whole.push_str(page["responseWindow"].as_str().expect("window"));
        // The last page carries no block.
        let Some(block) = page.get("responsePagination") else {
            break;
        };
        assert_eq!(block["scope"], "structuredContent", "{page}");
        let Some(next) = options.follow(&block["next"]["query"]) else {
            break;
        };
        options = next;
    }
    assert_eq!(
        serde_json::from_str::<Value>(&whole).expect("the windows join into JSON"),
        structured
    );
}
