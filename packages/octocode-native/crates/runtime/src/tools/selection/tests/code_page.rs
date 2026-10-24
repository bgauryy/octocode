//! Selection of one `ghSearchCode` provider page at the response boundary.
use super::*;

#[test]
fn context_selection_activates_without_optional_research_metadata() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (runtime, calls) = selecting_runtime(&executor);
    let query = json!({
        "owner":"a", "repo":"b", "keywords":["retry"],
        "classification":{"predicates":["Does this evidence state when retries stop?"]}
    });
    let mut structured = json!({"results":[{"index":0,"data":{"files":[
        {"path":"a.rs","matches":[{"line":1,"value":"stop after three attempts"}]},
        {"path":"b.rs","matches":[{"line":1,"value":"start a new attempt"}]}
    ]}}]});
    let mut response_query = query.clone();

    apply_code_rows(&mut structured, query, &mut response_query, &runtime)
        .expect("classified page without optional metadata");

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(structured["results"][0]["data"].get("selection").is_some());
    assert!(structured["results"][0]["data"].get("ranking").is_none());
    assert!(response_query.get("selection").is_some());
    assert!(response_query.get("ranking").is_none());
}

#[test]
fn code_page_selects_in_source_order_and_replays_without_a_second_judgment() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (runtime, calls) = selecting_runtime(&executor);
    let query = json!({"owner":"a","repo":"b","keywords":["target"],"classification":{"predicates":["wanted evidence"]}});
    let baseline = json!({"results":[{"index":0,"data":{"files":[
        {"path":"one.rs","lines":["source evidence"]},{"path":"two.rs","lines":["source evidence"]},{"path":"three.rs","lines":["source evidence"]}
    ],"pagination":{"currentPage":1,"hasMore":true}}}]});
    let mut first = baseline.clone();
    let mut response_query = query.clone();
    apply_code_rows(&mut first, query.clone(), &mut response_query, &runtime).expect("first page");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        first["results"][0]["data"]["selection"]["scope"],
        "providerPage"
    );
    assert_eq!(
        first["results"][0]["data"]["files"],
        json!([{"path":"one.rs","lines":["source evidence"]},{"path":"two.rs","lines":["source evidence"]},{"path":"three.rs","lines":["source evidence"]}])
    );
    assert!(response_query.get("selection").is_some());

    let mut replay = baseline;
    let mut replay_query = response_query.clone();
    apply_code_rows(&mut replay, response_query, &mut replay_query, &runtime).expect("replay");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        replay["results"][0]["data"]["files"],
        first["results"][0]["data"]["files"]
    );
}

#[test]
fn new_candidate_pages_drop_prior_decisions_without_mutating_source_evidence() {
    use crate::response::result::{Continuation, continuation_row};
    let query =
        json!({"page":2,"classification":{"predicates":["wanted"]},"selection":"prior-plan"});
    let page = Continuation::continued(ToolId::GhSearchCode, query.clone()).build();
    let read = Continuation::continued(ToolId::GhGetFileContent, query.clone()).build();
    let related = Continuation::related(ToolId::GhSearchCode, query).build();
    let source =
        json!({"next":{"sourceValue":page},"query":{"selection":"literal source content"}});
    let mut data = json!({"next":{"nextPage":page,"read":read},"related":[related],"files":[{"evidence":source,"next":{"nextMatchPage":page}}]});
    strip_plan_from_tool_continuations(&mut data, ToolId::GhSearchCode);
    for call in [
        &data["next"]["nextPage"],
        &data["next"]["read"],
        &data["related"][0],
        &data["files"][0]["next"]["nextMatchPage"],
    ] {
        assert!(continuation_row(call).unwrap().get("selection").is_none());
    }
    assert_eq!(
        continuation_row(&data["next"]["nextPage"]).unwrap()["classification"]["predicates"],
        json!(["wanted"])
    );
    assert!(
        continuation_row(&data["next"]["read"])
            .unwrap()
            .get("classification")
            .is_none()
    );
    assert!(
        continuation_row(&data["related"][0])
            .unwrap()
            .get("classification")
            .is_none()
    );
    assert_eq!(data["files"][0]["evidence"], source);
}

#[test]
fn rejected_rows_do_not_misalign_goal_queries() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (runtime, calls) = selecting_runtime(&executor);
    let plain = json!({"owner":"a","repo":"b","keywords":["target"]});
    let goal = json!({"owner":"a","repo":"b","keywords":["target"],"classification":{"predicates":["wanted"]}});
    let mut structured = json!({"results":[
        {"index":0,"status":"error","data":{"errorCode":"invalidInput"}},
        {"index":1,"data":{"files":[{"path":"one.rs","lines":["source evidence"]},{"path":"two.rs","lines":["source evidence"]}]}},
        {"index":2,"data":{"files":[{"path":"three.rs","lines":["source evidence"]},{"path":"four.rs","lines":["source evidence"]}]}}
    ]});
    let mut response_query = json!({"queries":[plain.clone(),goal.clone()]});
    apply_sanitized_rows(
        ToolId::GhSearchCode,
        &mut structured,
        &[plain, goal].map(CheckedQuery::checked),
        &[1, 2],
        &mut response_query,
        &runtime,
    )
    .expect("mixed rows");
    assert!(structured["results"][1]["data"].get("selection").is_none());
    assert_eq!(
        structured["results"][2]["data"]["selection"]["status"],
        "complete"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(response_query["queries"][0].get("selection").is_none());
    assert!(response_query["queries"][1].get("selection").is_some());
}

#[test]
fn only_code_search_selects_at_the_response_boundary() {
    // Local search/fetch and GitHub file reads rank before their own
    // source pagination; the engine calls `apply_sanitized_rows` only for
    // a tool whose output says it ranks one provider page.
    let boundary = ToolId::ALL
        .into_iter()
        .filter(|tool| tool.output().selects_provider_page())
        .collect::<Vec<_>>();
    assert_eq!(boundary, [ToolId::GhSearchCode]);
}

#[test]
fn selected_page_rejects_null_evidence_drift_but_accepts_key_reordering() {
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let (runtime, calls) = selecting_runtime(&executor);
    let query = json!({"owner":"o","repo":"r","keywords":["queue"],"classification":{"predicates":["find queue"]}});
    let original = json!({"results":[{"index":0,"data":{"files":[
        {"path":"a.rs","detail":null,"lines":["source evidence"]},
        {"path":"b.rs","lines":["source evidence"]}
    ]}}]});
    let mut first = original.clone();
    let mut replay_query = query.clone();
    apply_code_rows(&mut first, query.clone(), &mut replay_query, &runtime)
        .expect("initial selected page");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        first["results"][0]["data"]["selection"]["status"],
        "complete"
    );

    let mut same_evidence = json!({"results":[{"index":0,"data":{"files":[
        {"lines":["source evidence"],"detail":null,"path":"a.rs"},
        {"path":"b.rs","lines":["source evidence"]}
    ]}}]});
    let mut same_response = replay_query.clone();
    apply_code_rows(
        &mut same_evidence,
        replay_query.clone(),
        &mut same_response,
        &runtime,
    )
    .expect("same source with reordered object keys");
    assert_eq!(
        same_evidence["results"][0]["data"]["selection"]["status"],
        "complete"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1, "replay does not rescore");

    let mut changed = original;
    changed["results"][0]["data"]["files"][0]
        .as_object_mut()
        .expect("file object")
        .remove("detail");
    let mut changed_response = replay_query.clone();
    apply_code_rows(&mut changed, replay_query, &mut changed_response, &runtime)
        .expect("changed source is reported in its row");
    assert_eq!(changed["results"][0]["status"], "error");
    assert_eq!(changed["results"][0]["data"]["errorCode"], "staleSnapshot");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "stale replay does not rescore"
    );
}
