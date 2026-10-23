use super::*;

/// One row whose evidence precedes its page state and `next`, as the
/// GitHub tools wrote it before the rule.
fn evidence_first_response(tool: ToolId) -> Value {
    let call = json!({"tool": tool.as_str(), "query": {"queries": [{"page": 2}]}});
    let data = json!({
        "evidenceRows": [{"path": "a"}],
        "contentPagination": {"currentPage": 1, "hasMore": true},
        "hints": {"text": ["tip"]},
        "pagination": {"currentPage": 1, "hasMore": true},
        "next": {"nextPage": call},
        "warnings": ["w"],
    });
    if tool.output().resource_major() {
        let mut query = data;
        query["id"] = json!("q");
        json!({"queries": [query]})
    } else {
        json!({"results": [{"index": 0, "status": "ok", "data": data}]})
    }
}

fn row_object(response: &Value, tool: ToolId) -> &Value {
    if tool.output().resource_major() {
        &response["queries"][0]
    } else {
        &response["results"][0]["data"]
    }
}

#[test]
fn every_tool_leads_pagination_and_next_before_its_evidence() {
    for tool in ToolId::ALL {
        let mut response = evidence_first_response(tool);
        lead_continuations(&mut response, tool);
        let keys: Vec<&str> = row_object(&response, tool)
            .as_object()
            .expect("row object")
            .keys()
            .map(String::as_str)
            .collect();
        let at = |name: &str| keys.iter().position(|key| *key == name).expect(name);
        let evidence = at("evidenceRows");
        for lead in ["pagination", "contentPagination", "next"] {
            assert!(
                at(lead) < evidence,
                "{tool:?}: {lead} after the rows: {keys:?}"
            );
        }
        assert!(
            at("pagination") < at("contentPagination"),
            "{tool:?}: {keys:?}"
        );
        assert!(at("contentPagination") < at("next"), "{tool:?}: {keys:?}");
        let anchors = if tool.output().resource_major() { 2 } else { 1 };
        assert!(at("warnings") < anchors, "{tool:?}: {keys:?}");
    }
}

#[test]
fn the_rule_is_idempotent_and_keeps_every_key() {
    for tool in ToolId::ALL {
        let mut once = evidence_first_response(tool);
        lead_continuations(&mut once, tool);
        let mut twice = once.clone();
        lead_continuations(&mut twice, tool);
        assert_eq!(once.to_string(), twice.to_string(), "{tool:?}");
        let before = evidence_first_response(tool);
        assert_eq!(
            row_object(&once, tool)
                .as_object()
                .map(|object| object.len()),
            row_object(&before, tool)
                .as_object()
                .map(|object| object.len()),
            "{tool:?}"
        );
    }
}

#[test]
fn the_json_text_shows_pagination_and_next_before_the_rows() {
    for tool in ToolId::ALL {
        let mut response = evidence_first_response(tool);
        lead_continuations(&mut response, tool);
        // Every surface prints this JSON: MCP text and CLI `--json`.
        let text = response.to_string();
        let (page, next, rows) = (
            text.find("pagination"),
            text.find("next"),
            text.find("evidenceRows"),
        );
        assert!(
            page < rows && next < rows,
            "{tool:?}: continuation after the rows:\n{text}"
        );
    }
}
