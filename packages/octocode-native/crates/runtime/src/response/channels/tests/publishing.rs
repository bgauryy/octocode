use super::*;

#[test]
fn published_guidance_is_deduplicated_text_and_keeps_pages_executable() {
    let call = json!({"tool":"localFetch","query":{"queries":[{"path":"a.rs","ranges":["2-4"]}]}});
    let mut response = json!({"results":[{"data":{
        "next":{"continueWindow":call},
        "hints":{"text":[" narrow ","narrow",""],"read":call,"duplicate":call},
        "pullRequests":[{"hints":{"read":call}}]
    }}]});
    super::super::publish_hints(&mut response, ToolId::GhGetHistoryItem);
    let data = &response["results"][0]["data"];
    assert_eq!(data["next"]["continueWindow"], call);
    let hints = data["hints"].as_array().expect("string hints");
    assert_eq!(hints.len(), 2);
    assert_eq!(hints[0], "narrow");
    let suggestion = hints[1].as_str().expect("suggested call");
    let query: Value =
        serde_json::from_str(suggestion.strip_prefix("read: localFetch ").expect("tool"))
            .expect("JSON input");
    assert_eq!(query, call["query"]);
    assert_eq!(data["pullRequests"][0]["hints"], json!([suggestion]));
    let once = response.clone();
    super::super::publish_hints(&mut response, ToolId::GhGetHistoryItem);
    assert_eq!(response, once);
    let mut evidence = json!({"queries":[{"answers":{"score":{"type":"score",
            "legend":{"0":{"hints":[" repeated "," repeated "],"nested":{"hints":{"read":call}}}}}}}]});
    let original = evidence.clone();
    super::super::publish_hints(&mut evidence, ToolId::Clasify);
    assert_eq!(evidence, original, "evidence fields are not guidance");
}

#[test]
fn equivalent_hint_calls_ignore_object_order_but_keep_batch_order() {
    let first = json!({"queries":[{"path":"a.rs","ranges":["2-4"]},{"path":"b.rs"}]});
    let reordered = json!({"queries":[{"ranges":["2-4"],"path":"a.rs"},{"path":"b.rs"}]});
    let reversed = json!({"queries":[{"path":"b.rs"},{"path":"a.rs","ranges":["2-4"]}]});
    let mut response = json!({"results":[{"data":{"hints":{
        "read":{"tool":"localFetch","query":first},
        "same":{"tool":"localFetch","query":reordered},
        "reversed":{"tool":"localFetch","query":reversed}
    }}}]});
    super::super::publish_hints(&mut response, ToolId::LocalFetch);
    assert_eq!(
        response["results"][0]["data"]["hints"],
        json!([
            format!("read: localFetch {first}"),
            format!("reversed: localFetch {reversed}")
        ])
    );
}

#[test]
fn only_pages_that_leave_more_to_read_are_remaining() {
    for name in [
        "nextPage",
        "continueWindow",
        "continuePatch",
        "expandScan",
        "retry",
    ] {
        assert!(is_remaining(ToolId::LocalSearch, name), "{name}");
    }
    for name in ["restart", "restartScan", "readBody", "read", "nextpage"] {
        assert!(!is_remaining(ToolId::LocalSearch, name), "{name}");
    }
    assert!(is_remaining(ToolId::Clasify, "clasify"));
}

#[test]
fn every_declared_continuation_kind_has_its_contract_channel() {
    // A tool whose name no kind shares, so the own-walk rule stays out.
    for name in continuation_channels::PAGE_KINDS {
        assert_eq!(channel(ToolId::LspSearch, name), Channel::Page, "{name}");
    }
    for name in continuation_channels::LEAD_KINDS {
        assert_eq!(channel(ToolId::LspSearch, name), Channel::Lead, "{name}");
    }
    assert_eq!(channel(ToolId::Clasify, "clasify"), Channel::Page);
    assert_eq!(channel(ToolId::LocalSearch, "clasify"), Channel::Lead);
    assert_eq!(channel(ToolId::Clasify, "read"), Channel::Lead);
}
