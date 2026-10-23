use super::*;
use crate::response::result::is_approximate;

#[test]
fn splits_leads_and_prose_out_of_next_without_touching_pages() {
    let page = json!({"tool":"localSearch","query":{"path":"/r","matchString":"x","page":2}});
    let lead = json!({"tool":"localFetch","query":{"path":"/r/a"}});
    let mut structured = json!({"results":[{"index":0,"data":{
            "hints":["Add includeIgnored:true."],
            "next":{"nextPage":page,"read":lead},
            "pullRequests":[{"number":1,"next":{"readBody":lead}}]
        }}],"responsePagination":{"next":{"tool":"localSearch","query":{}}}});
    split_hints(&mut structured, ToolId::LocalSearch);
    let data = &structured["results"][0]["data"];
    assert_eq!(data["next"], json!({"nextPage":page}));
    assert_eq!(
        data["hints"],
        json!({"text":["Add includeIgnored:true."],"read":lead})
    );
    assert_eq!(data["pullRequests"][0]["hints"], json!({"readBody":lead}));
    assert!(data["pullRequests"][0].get("next").is_none());
    assert!(structured["responsePagination"]["next"].is_object());
    let once = structured.clone();
    split_hints(&mut structured, ToolId::LocalSearch);
    assert_eq!(structured, once, "idempotent");
}

#[test]
fn hints_keep_distinct_leads_without_capping_pages_or_text() {
    let call = |tool: &str| json!({"tool":tool,"query":{"path":"/r/a"}});
    let mut low = call("localSearch");
    low["confidence"] = json!("low");
    let mut structured = json!({"results":[{"index":0,"data":{
        "hints":["Tip one.","Tip two.","Tip three."],
        "next":{
            "nextPage":call("lspSearch"),
            "continueWalk":call("lspSearch"),
            "continueWalk2":json!({"tool":"lspSearch","query":{"path":"/r/b"}}),
            "widen":low,
            "readDefinition":call("localFetch"),
            "findCallers":call("lspSearch"),
            "read":call("localFetch")
        }
    }}]});
    split_hints(&mut structured, ToolId::LspSearch);
    let data = &structured["results"][0]["data"];
    assert_eq!(
        data["next"].as_object().map(|next| next.len()),
        Some(3),
        "pages are never capped: {data}"
    );
    let hints = data["hints"].as_object().expect("hints");
    assert_eq!(hints["text"].as_array().map(Vec::len), Some(3));
    let leads = hints
        .keys()
        .filter(|key| *key != "text")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        leads,
        vec!["readDefinition", "findCallers", "read", "widen"],
        "{data}"
    );
    let once = structured.clone();
    split_hints(&mut structured, ToolId::LspSearch);
    assert_eq!(structured, once, "idempotent");
}

/// Result counts do not arbitrarily remove distinct optional routes.
#[test]
fn lead_availability_does_not_depend_on_the_result_count() {
    let call = |tool: &str| json!({"tool":tool,"query":{"path":"a"}});
    let leads = |data: Value, tool: ToolId| {
        let mut structured = json!({"results":[{"index":0,"data":data}]});
        split_hints(&mut structured, tool);
        structured["results"][0]["data"]["hints"]
            .as_object()
            .map(|hints| hints.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let menu =
        || json!({"readFiles":call("ghGetHistoryItem"),"readDiscussion":call("ghGetHistoryItem")});
    let one =
        json!({"pullRequests":[{"number":1,"labels":["bug"]}],"warnings":["w"],"next":menu()});
    assert_eq!(
        leads(one, ToolId::GhGetHistoryItem),
        ["readFiles", "readDiscussion"]
    );
    let two = json!({"pullRequests":[{"number":1},{"number":2}],"next":menu()});
    assert_eq!(leads(two, ToolId::GhGetHistoryItem).len(), 2);
    let partial = json!({"pullRequests":[{"number":1}],"isPartial":true,"next":menu()});
    assert_eq!(leads(partial, ToolId::GhGetHistoryItem).len(), 2);
    // One file with two matches is two hits.
    let file = json!({"files":[{"path":"a.rs","matches":[{"line":1},{"line":2}]}],
            "next":{"read":call("localFetch"),"findCallers":call("lspSearch")}});
    assert_eq!(leads(file, ToolId::LocalSearch).len(), 2);
    let hit = json!({"files":[{"path":"a.rs","matches":[{"line":1,"enclosing":{"line":1}}]}],
            "next":{"read":call("localFetch"),"findCallers":call("lspSearch")}});
    assert_eq!(leads(hit, ToolId::LocalSearch), ["read", "findCallers"]);
    // Exact routes and optional alternatives remain available.
    let exact = |tool: &str| json!({"tool":tool,"confidence":"exact","query":{"path":"a"}});
    let named = json!({"pullRequests":[{"number":1}],
            "next":{"readFiles":exact("ghGetHistoryItem"),"readDiscussion":exact("ghGetHistoryItem")}});
    assert_eq!(
        leads(named, ToolId::GhGetHistoryItem),
        ["readFiles", "readDiscussion"]
    );
    let alternative = json!({"pullRequests":[{"number":1}],
            "next":{"readFiles":exact("ghGetHistoryItem"),"readDiscussion":call("ghGetHistoryItem")}});
    assert_eq!(
        leads(alternative, ToolId::GhGetHistoryItem),
        ["readFiles", "readDiscussion"]
    );
    let paged = json!({"files":[{"path":"a.rs","matches":[{"line":1}]}],
            "next":{"nextPage":call("localSearch"),"read":call("localFetch"),"findCallers":call("lspSearch")}});
    assert_eq!(leads(paged, ToolId::LocalSearch).len(), 2);
}

#[test]
fn contract_priority_leads_precede_rereads() {
    assert_eq!(
        continuation_channels::LEAD_PRIORITY.first(),
        Some(&"readAtMerge")
    );
    let call = |tool: &str| json!({"tool":tool,"query":{"path":"a"}});
    let mut structured = json!({"results":[{"index":0,"data":{
        "pullRequests":[{"number":1,"next":{
            "readBody":call("ghGetHistoryItem"),
            "readUntrimmed":call("ghGetHistoryItem"),
            "readAtMerge":call("ghGetFileContent")
        }}]
    }}]});
    split_hints(&mut structured, ToolId::GhGetHistoryItem);
    let hints = &structured["results"][0]["data"]["pullRequests"][0]["hints"];
    let leads = hints
        .as_object()
        .map(|h| h.keys().cloned().collect::<Vec<_>>());
    assert_eq!(
        leads,
        Some(vec![
            "readAtMerge".to_owned(),
            "readBody".to_owned(),
            "readUntrimmed".to_owned()
        ]),
        "{hints}"
    );
}

/// A wide semantic search page appends its clasify handoff after the
/// tool's own reads; the contract priority keeps it under the cap, so
/// the screening route is reachable at all.
#[test]
fn the_clasify_search_handoff_survives_the_cap() {
    let call = |tool: &str| json!({"tool":tool,"query":{"path":"a"}});
    let mut structured = json!({"results":[{"index":0,"data":{"next":{
        "nextPage":call("ghSearchCode"),
        "read":call("ghGetFileContent"),
        "readHits":call("ghGetFileContent"),
        "clasify":call("clasify")
    }}}]});
    split_hints(&mut structured, ToolId::GhSearchCode);
    let data = &structured["results"][0]["data"];
    assert!(data["next"]["nextPage"].is_object(), "{data}");
    let leads = data["hints"]
        .as_object()
        .map(|h| h.keys().cloned().collect::<Vec<_>>());
    assert_eq!(
        leads,
        Some(vec!["read".to_owned(), "clasify".to_owned()]),
        "{data}"
    );
}

/// The listing of skipped binaries is a lead (the search is complete
/// without it) that a full lead menu still keeps: its warning only
/// counts the files.
#[test]
fn the_skipped_binary_listing_remains_a_lead() {
    assert_eq!(channel(ToolId::LocalSearch, "binarySkipped"), Channel::Lead);
    let call = |tool: &str| json!({"tool":tool,"query":{"path":"a"}});
    let mut structured = json!({"results":[{"index":0,"data":{"next":{
        "nextPage":call("localSearch"),
        "read":call("localFetch"),
        "binarySkipped":call("structureSearch"),
        "findReferences":call("lspSearch")
    }}}]});
    split_hints(&mut structured, ToolId::LocalSearch);
    let data = &structured["results"][0]["data"];
    assert!(data["next"]["nextPage"].is_object(), "{data}");
    assert!(data["next"].get("binarySkipped").is_none(), "{data}");
    let leads = data["hints"]
        .as_object()
        .map(|h| h.keys().cloned().collect::<Vec<_>>());
    assert_eq!(
        leads,
        Some(vec![
            "read".to_owned(),
            "binarySkipped".to_owned(),
            "findReferences".to_owned()
        ]),
        "{data}"
    );
}

/// Minimizing a lead never touches the fields that select its evidence:
/// every `ranges` entry (the rest of a cut declaration is two spans),
/// `matchString` and `path` reach the target unchanged.
#[test]
fn lead_minimization_keeps_every_evidence_selecting_field() {
    let row = json!({"path":"src/lib.rs","ranges":["1-9","15-21"],"matchString":"needle","contextLines":2});
    for lead in [
        json!({"tool":"localFetch","query":{"queries":[row.clone()]}}),
        json!({"tool":"localFetch","query":{"queries":[row.clone(), {"path":"b.rs"}]}}),
    ] {
        let mut structured = json!({"results":[{"index":0,"data":{
            "next":{"read":lead.clone()}
        }}]});
        split_hints(&mut structured, ToolId::LocalFetch);
        let kept = &structured["results"][0]["data"]["hints"]["read"]["query"]["queries"];
        assert_eq!(kept, &lead["query"]["queries"], "{structured}");
    }
}

/// A lead keeps every row its tool built, like a page: its rows may be
/// routes the response never shows (pruned directories at their depth).
#[test]
fn a_lead_keeps_every_row_its_tool_built() {
    let rows = json!([
        {"path":"a.rs","ranges":["1-3","9-12"]},
        {"path":"b.rs","ranges":["4-5"]}
    ]);
    let mut structured = json!({"results":[{"index":0,"data":{
        "next":{
            "nextPage":{"tool":"localFetch","query":{"queries":rows}},
            "read":{"tool":"localFetch","query":{"queries":rows}}
        },
        "files":[{"path":"a.rs","hints":{"read":{"tool":"localFetch","query":{"path":"a.rs","ranges":["1-3","9-12"]}}}}]
    }}]});
    split_hints(&mut structured, ToolId::AstSearch);
    let data = &structured["results"][0]["data"];
    assert_eq!(data["next"]["nextPage"]["query"]["queries"], rows);
    assert_eq!(data["hints"]["read"]["query"], json!({"queries":rows}));
    assert_eq!(
        data["files"][0]["hints"]["read"]["query"]["ranges"],
        json!(["1-3", "9-12"]),
        "a resource-shaped read (clasify's located lines) is one answer"
    );
    let once = structured.clone();
    split_hints(&mut structured, ToolId::AstSearch);
    assert_eq!(structured, once, "idempotent");
}

/// `broadenSearch` rows are alternatives (drop-one-keyword
/// variants), not other rows already shown: the lead keeps them all.
#[test]
fn a_broaden_search_lead_keeps_every_alternative_row() {
    let rows = json!([
        {"owner":"o","repo":"r","operation":"pullRequest","keywords":["a"]},
        {"owner":"o","repo":"r","operation":"pullRequest","keywords":["b"]}
    ]);
    let mut structured = json!({"results":[{"index":0,"status":"empty","data":{
        "next":{"broadenSearch":{"tool":"ghSearchHistory","query":{"queries":rows}}}
    }}]});
    split_hints(&mut structured, ToolId::GhSearchHistory);
    assert_eq!(
        structured["results"][0]["data"]["hints"]["broadenSearch"]["query"]["queries"], rows,
        "{structured}"
    );
}

#[test]
fn an_approximate_lead_yields_its_place_and_keeps_its_caveat() {
    let call = |tool: &str, approximate: bool| {
        let mut call = json!({"tool":tool,"query":{"queries":[{"path":"a"}]}});
        if approximate {
            call["confidence"] = json!("low");
        }
        call
    };
    let mut structured = json!({"results":[{"index":0,"data":{
        "next": {
            "nextPage": call("localSearch", false),
            "widen": call("localSearch", true),
            "read": call("localFetch", false),
            "findReferences": call("lspSearch", false)
        },
        "files": [{"path":"a","hints":{"read":call("localFetch", false)}}],
        "confidence": "exact"
    }}]});
    split_hints(&mut structured, ToolId::LocalSearch);
    let data = &structured["results"][0]["data"];
    let leads = data["hints"].as_object().expect("hints");
    assert_eq!(
        leads.keys().cloned().collect::<Vec<_>>(),
        vec!["read", "findReferences", "widen"],
        "approximate leads follow exact routes: {data}"
    );
    assert!(
        leads
            .iter()
            .all(|(name, lead)| is_approximate(lead) == (name == "widen")),
        "{data}"
    );
    assert_eq!(data["confidence"], "exact", "only continuations change");
}
