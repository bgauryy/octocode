use super::drop_answered_leads;
use crate::id::ToolId;
use serde_json::{Value, json};

fn next() -> Value {
    json!({"nextPage": {"kind": "continue", "tool": "x", "input": {"cursor": "c"}}})
}

/// No emitted lead re-asks a query another row of the same request
/// answers, whatever the brief beside it; a lead repeating the row's own
/// query (its retry), one to another tool and one sending something else
/// stay, for every tool.
#[test]
fn no_lead_repeats_another_query_of_the_request() {
    let outline = json!({"path": "src/a.go", "operation": "documentSymbols"});
    let symbols = json!({"path": "src/a.go", "operation": "workspaceSymbol", "symbolFuzzy": "Run"});
    for tool in ToolId::ALL {
        let call = |tool: &str, query: &Value| json!({"kind": "related", "tool": tool, "input": {"queries": [query]}});
        let mut briefed = outline.clone();
        briefed["mainGoal"] = json!("outline a.go");
        briefed["reasoning"] = json!("which declarations");
        let request = json!({"queries": [briefed, symbols]});
        let other = if tool == ToolId::LocalFetch {
            ToolId::LocalSearch
        } else {
            ToolId::LocalFetch
        };
        let kept = vec![
            call(tool.as_str(), &symbols),
            call(other.as_str(), &outline),
            call(
                tool.as_str(),
                &json!({"path": "src/b.go", "operation": "documentSymbols"}),
            ),
        ];
        let mut related = vec![call(tool.as_str(), &outline)];
        related.extend(kept.clone());
        let mut output = json!({"results": [
            {"index": 0, "data": {"related": [call(tool.as_str(), &symbols)], "next": next()}},
            {"index": 1, "data": {"related": related, "next": next()}},
        ]});
        drop_answered_leads(tool, &mut output, &request);
        assert_eq!(
            output["results"][1]["data"]["related"],
            json!(kept),
            "{tool}"
        );
        assert_eq!(output["results"][1]["data"]["next"], next(), "{tool}");
        // Its only lead was the other row's query: none is left, and none is [].
        assert!(
            output["results"][0]["data"].get("related").is_none(),
            "{tool}: {output}"
        );
        // A single query's own retry stays.
        let mut alone = json!({"results": [{"index": 0, "data": {"related": [call(tool.as_str(), &outline)]}}]});
        let before = alone.clone();
        drop_answered_leads(tool, &mut alone, &json!({"queries": [outline]}));
        assert_eq!(alone, before, "{tool}");
    }
}

/// Identical admitted queries are legal. A row's own retry is still its
/// recovery when a sibling happened to ask the same thing and also failed.
#[test]
fn an_own_retry_survives_an_identical_sibling_query() {
    let query = json!({"path": "src/missing.rs"});
    let retry = json!({"kind": "related", "tool": "localFetch",
        "input": {"queries": [query]}});
    let request = json!({"queries": [query, query]});
    let mut output = json!({"results": [
        {"index": 0, "status": "error", "data": {
            "errorCode": "notFound", "error": "source is absent", "related": [retry], "next": next()}},
        {"index": 1, "status": "error", "data": {
            "errorCode": "notFound", "error": "source is absent", "related": [retry], "next": next()}},
    ]});
    let before = output.clone();
    drop_answered_leads(ToolId::LocalFetch, &mut output, &request);
    assert_eq!(
        output, before,
        "neither failed row replaces the other's retry"
    );
}
