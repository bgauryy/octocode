use super::*;
use serde_json::json;

#[test]
fn an_explicit_length_rides_the_tools_own_pages_only() {
    let mut structured = json!({"results":[{"data":{"next":{
        "continueWindow":{"tool":"localFetch","query":{"queries":[{"path":"a","offset":5}]}},
        "own":{"tool":"localFetch","query":{"queries":[{"path":"a"}],"responseLength":9}},
        "lsp":{"tool":"lspSearch","query":{"queries":[{"path":"a"}]}}
    }}}]});
    carry_response_length(&mut structured, ToolId::LocalFetch, 1_500);
    let next = &structured["results"][0]["data"]["next"];
    assert_eq!(next["continueWindow"]["query"]["responseLength"], 1_500);
    assert_eq!(next["own"]["query"]["responseLength"], 9);
    assert!(next["lsp"]["query"].get("responseLength").is_none());
}
