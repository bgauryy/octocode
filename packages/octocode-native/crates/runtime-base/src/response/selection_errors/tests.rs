use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

/// A filtered view carries its per-call facts (`selection`, the
/// `next.unfiltered` lead) and no static reminder: that a filtered view
/// is not proof of absence is said once, in the instructions.
#[test]
fn a_filtered_view_adds_no_static_reminder() {
    for tool in [
        ToolId::LocalSearch,
        ToolId::LocalFetch,
        ToolId::GhSearchCode,
        ToolId::GhGetFileContent,
    ] {
        let mut structured = json!({"results":[{"index":0,"data":{
            "selection":{"status":"partial","selectedItems":1,"unknownItems":1},
            "next":{"unfiltered":{"tool":tool.as_str(),"query":{"queries":[{"path":"a.rs"}]}}}
        }}]});
        let before = structured.clone();
        let query = json_fixture(
            r#"{
                "path": "a.rs",
                "classification": {"predicates": ["States the rule?"]}
            }"#,
        );
        settle(tool, &mut structured, &query);
        assert_eq!(structured, before, "{structured}");
    }
}

#[test]
fn a_view_that_left_nothing_out_does_not_prompt_more_reading() {
    let mut structured = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "data": {
                        "selection": {
                            "status": "complete",
                            "selectedItems": 1,
                            "totalItems": 1,
                            "unknownItems": 0
                        }
                    }
                }
            ]
        }"#,
    );
    let before = structured.clone();
    decorate(
        ToolId::LocalFetch,
        &mut structured,
        &json_fixture(
            r#"{
                "path": "a.rs",
                "classification": {"predicates": ["States the rule?"]}
            }"#,
        ),
    );
    assert_eq!(structured, before);
}

/// [`decorate`] and the stage's notice placement.
fn settle(tool: ToolId, structured: &mut Value, query: &Value) {
    decorate(tool, structured, query);
    crate::response::notices::place(structured, "results");
}

#[test]
fn classified_source_failure_gets_reconciled_unavailable_receipt() {
    let mut structured = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "status": "error",
                    "data": {
                        "path": "missing.rs",
                        "errorCode": "notFound",
                        "error": "missing"
                    }
                }
            ]
        }"#,
    );
    decorate(
        ToolId::LocalFetch,
        &mut structured,
        &json_fixture(
            r#"{
                "path": "missing.rs",
                "classification": {"predicates": ["Relevant?"]}
            }"#,
        ),
    );
    let data = &structured["results"][0]["data"];
    assert_eq!(data["error"], "missing");
    assert_eq!(data["selection"]["status"], "unavailable");
    assert_eq!(data["selection"]["scope"], "selectedView");
    assert_eq!(data["selection"]["unit"], "chunks");
    assert_eq!(data["selection"]["totalItems"], 0);
    assert_eq!(data["selection"]["selectedItems"], 0);
    assert_eq!(data["selection"]["excludedItems"], 0);
    assert_eq!(data["selection"]["unknownItems"], 0);
    assert_eq!(data["selection"]["reason"], "sourceUnavailable");
    assert!(data.pointer("/next/unfiltered").is_none());
}

#[test]
fn mixed_batch_pairs_only_admitted_queries_and_preserves_existing_receipts() {
    let existing = json_fixture(
        r#"{
            "status": "partial",
            "scope": "providerPage",
            "unit": "files",
            "totalItems": 2,
            "selectedItems": 1,
            "excludedItems": 0,
            "unknownItems": 1
        }"#,
    );
    let mut structured = json!({"results":[
        {"index":0,"status":"error","data":{"errorCode":"invalidInput","error":"rejected"}},
        {"index":1,"status":"error","data":{"errorCode":"server","error":"plain failed"}},
        {"index":2,"status":"error","data":{"errorCode":"server","error":"classified failed"}},
        {"index":3,"status":"error","data":{"errorCode":"server","error":"already shaped","selection":existing}}
    ]});
    decorate(
        ToolId::GhSearchCode,
        &mut structured,
        &json_fixture(
            r#"{
                "queries": [
                    {"owner": "o", "keywords": ["plain"]},
                    {
                        "owner": "o",
                        "keywords": ["selected"],
                        "classification": {"predicates": ["Relevant?"]}
                    },
                    {
                        "owner": "o",
                        "keywords": ["existing"],
                        "classification": {"predicates": ["Relevant?"]}
                    }
                ]
            }"#,
        ),
    );
    let rows = structured["results"].as_array().unwrap();
    assert!(rows[0]["data"].get("selection").is_none());
    assert!(rows[1]["data"].get("selection").is_none());
    assert_eq!(rows[2]["data"]["selection"]["scope"], "providerPage");
    assert_eq!(rows[2]["data"]["selection"]["unit"], "hits");
    assert_eq!(rows[3]["data"]["selection"], existing);
}

#[test]
fn ordinary_and_unrelated_failures_are_unchanged() {
    for (tool, query) in [
        (
            ToolId::LocalSearch,
            json_fixture(r#"{"path": ".", "matchString": "x"}"#),
        ),
        (ToolId::GhSearchRepo, json_fixture(r#"{"keywords": ["x"]}"#)),
    ] {
        let mut structured = json_fixture(
            r#"{
                "results": [
                    {
                        "index": 0,
                        "status": "error",
                        "data": {"errorCode": "server", "error": "failed"}
                    }
                ]
            }"#,
        );
        let original = structured.clone();
        decorate(tool, &mut structured, &query);
        assert_eq!(structured, original);
    }
}

#[test]
fn a_refused_view_gets_no_unknown_receipt_or_inspection_notice() {
    let mut structured = json_fixture(
        r#"{
            "results": [
                {
                    "index": 0,
                    "status": "error",
                    "data": {
                        "errorCode": "invalidInput",
                        "error": "unsupported view",
                        "next": {
                            "unfiltered": {
                                "tool": "localSearch",
                                "query": {"queries": [{"path": ".", "matchString": "x"}]}
                            }
                        }
                    }
                }
            ]
        }"#,
    );
    let before = structured.clone();
    decorate(
        ToolId::LocalSearch,
        &mut structured,
        &json_fixture(
            r#"{
                "path": ".",
                "matchString": "x",
                "classification": {"predicates": ["Relevant?"]}
            }"#,
        ),
    );
    assert_eq!(structured, before);
}

#[test]
fn each_classified_tool_has_one_selection_receipt_and_an_unavailable_receipt_leaves_all_unknown() {
    assert_eq!(
        ToolId::LocalSearch.selection_receipt(),
        Some((SelectionScope::SelectedView, SelectionUnit::Hits)),
        "a search judges its hits, as its live receipt does"
    );
    for tool in [ToolId::LocalFetch, ToolId::GhGetFileContent] {
        assert_eq!(
            tool.selection_receipt(),
            Some((SelectionScope::SelectedView, SelectionUnit::Chunks))
        );
    }
    for tool in ToolId::ALL
        .into_iter()
        .filter(|tool| tool.selects_provider_page())
    {
        assert_eq!(
            tool.selection_receipt(),
            Some((SelectionScope::ProviderPage, SelectionUnit::Hits)),
            "{tool:?}"
        );
    }
    assert_eq!(ToolId::GhSearchRepo.selection_receipt(), None);
    let receipt = unavailable(
        SelectionScope::ProviderPage,
        SelectionUnit::Hits,
        2,
        "sourceUnavailable",
    );
    assert_eq!(receipt.status, SelectionStatus::Unavailable);
    assert_eq!(
        (
            receipt.total_items,
            receipt.selected_items,
            receipt.excluded_items,
            receipt.unknown_items
        ),
        (2, 0, 0, 2)
    );
    assert_eq!(receipt.reason.as_deref(), Some("sourceUnavailable"));
}
