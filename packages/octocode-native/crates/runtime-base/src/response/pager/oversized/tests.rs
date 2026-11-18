use super::*;

fn page(value: Value) -> Map<String, Value> {
    value.as_object().cloned().expect("an envelope")
}

/// Short rows and a long cursor: a page over its target by fixed parts.
#[test]
fn fixed_parts_never_flag_a_page() {
    let long = "x".repeat(300);
    let rows = page(json!({
        "results": [{"index":0,"data":{"files":[{"path":"a","matches":[{"line":1,"value":"short"}]}]}}],
        "responsePagination": {"hasMore":true,"next":{"kind":"continue","tool":"localSearch","input":{"cursor":long}}}
    }));
    assert!(!holds_value_over(&rows, ToolId::LocalSearch, 100));
}

/// Free text a page windows, kept whole or cut, is no indivisible value.
#[test]
fn free_text_is_split_on_purpose() {
    let long = "y".repeat(300);
    let hit = page(json!({"results": [{"index":0,
        "data": {"files":[{"path":"a","matches":[{"line":2,"value":long}]}]}}]}));
    assert!(!holds_value_over(&hit, ToolId::LocalSearch, 100));
    let read = page(json!({"results": [{"index":0,"data":{"content":long}}]}));
    assert!(!holds_value_over(&read, ToolId::LocalFetch, 100));
}

/// An identity every part repeats, and an executable call, run whole.
#[test]
fn identities_and_calls_are_indivisible() {
    let long = "z".repeat(300);
    let path = page(json!({"results": [{"index":0,
        "data": {"files":[{"path":long,"matches":[{"line":2,"value":"v"}]}]}}]}));
    assert!(holds_value_over(&path, ToolId::LocalSearch, 100));
    assert!(
        !holds_value_over(&path, ToolId::LocalSearch, 300),
        "at the target it fits"
    );
    let call = page(json!({"results": [{"index":0,"data":{
        "related":[{"kind":"related","tool":"localFetch","input":{"queries":[{"path":long}]}}]
    }}]}));
    assert!(holds_value_over(&call, ToolId::LocalSearch, 100));
}

#[test]
fn unflagging_rejudges_what_the_page_still_holds() {
    let long = "p".repeat(50);
    let mut over =
        json!({"results": [{"index":0,"data":{"files":[{"path":long}]}}], "oversized": true});
    unflag_fitting(&mut over, ToolId::LocalSearch, 10, true);
    assert_eq!(over["oversized"], true);
    unflag_fitting(&mut over, ToolId::LocalSearch, 100, true);
    assert!(over.get("oversized").is_none());
}
