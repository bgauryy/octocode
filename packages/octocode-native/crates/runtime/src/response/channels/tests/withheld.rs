use super::*;

/// Evidence a row withholds (a capped file's hits, a capped body, a
/// clipped line, narrowed patches) is the unread rest of the result: it
/// stays in `next` as one call per admitted batch.
#[test]
fn withheld_evidence_reads_stay_in_next_as_one_call() {
    for name in [
        "readHits",
        "readHits2",
        "readDeclaration3",
        "wholeLines",
        "readBoundedLines",
    ] {
        assert_eq!(channel(ToolId::GhSearchCode, name), Channel::Page, "{name}");
    }
    assert_eq!(channel(ToolId::GhSearchCode, "read2"), Channel::Lead);
    // A matchString patch view shows what was asked: the whole patch is a lead.
    assert_eq!(
        channel(ToolId::GhGetHistoryItem, "readFullPatches2"),
        Channel::Lead
    );
    let read = |path: &str| json!({"tool":"ghGetFileContent","query":{"queries":[{"owner":"o","repo":"r","path":path}]}});
    let mut next = serde_json::Map::new();
    next.insert("nextPage".into(), json!({"tool":"ghSearchCode","query":{"queries":[{"owner":"o","keywords":["k"],"page":2}]}}));
    next.insert("read".into(), read("top"));
    for index in 1..=7 {
        let name = if index == 1 {
            "readHits".to_owned()
        } else {
            format!("readHits{index}")
        };
        next.insert(name, read(&format!("f{index}")));
    }
    next.insert("readHits8".into(), read("f1"));
    next.insert(
        "viewRepo".into(),
        json!({"tool":"ghStructure","query":{"queries":[{"owner":"o","repo":"r"}]}}),
    );
    next.insert(
        "findCode".into(),
        json!({"tool":"ghSearchCode","query":{"queries":[{"owner":"o"}]}}),
    );
    let mut structured = json!({"results":[{"index":0,"data":{"next":next}}]});
    split_hints(&mut structured, ToolId::GhSearchCode);
    let data = &structured["results"][0]["data"];
    let pages = data["next"].as_object().expect("next");
    assert_eq!(
        pages.keys().cloned().collect::<Vec<_>>(),
        vec!["nextPage", "readHits", "readHits2"],
        "{data}"
    );
    let paths = |name: &str| {
        pages[name]["query"]["queries"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| row["path"].as_str().unwrap_or_default().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(paths("readHits"), ["f1", "f2", "f3", "f4", "f5"]);
    assert_eq!(paths("readHits2"), ["f6", "f7"], "duplicates merge once");
    assert_eq!(
        data["hints"].as_object().map(|h| h.len()),
        Some(3),
        "distinct optional routes remain available: {data}"
    );
    let once = structured.clone();
    split_hints(&mut structured, ToolId::GhSearchCode);
    assert_eq!(structured, once, "idempotent");
}

/// Every lead keeps all of its rows, with or without a next page.
#[test]
fn a_partial_object_without_a_page_keeps_all_leads_and_rows() {
    let rows = json!([{"path":"a"},{"path":"b"},{"path":"c"}]);
    let lead = |tool: &str| json!({"tool":tool,"query":{"queries":rows}});
    let next = |page: bool| {
        let mut next = json!({
            "findText":lead("localSearch"),
            "readDefinition":lead("localFetch"),
            "findCallers":lead("lspSearch")
        });
        if page {
            next["nextImporterPage"] = lead("lspSearch");
        }
        next
    };
    let mut open = json!({"results":[{"index":0,"data":{"isPartial":true,"next":next(false)}}]});
    split_hints(&mut open, ToolId::LspSearch);
    let hints = open["results"][0]["data"]["hints"]
        .as_object()
        .expect("hints");
    assert_eq!(hints.len(), 3, "{open}");
    assert!(
        hints.values().all(|call| call["query"]["queries"] == rows),
        "{open}"
    );
    let once = open.clone();
    split_hints(&mut open, ToolId::LspSearch);
    assert_eq!(open, once, "idempotent");
    let mut paged = json!({"results":[{"index":0,"data":{"isPartial":true,"next":next(true)}}]});
    split_hints(&mut paged, ToolId::LspSearch);
    let hints = paged["results"][0]["data"]["hints"]
        .as_object()
        .expect("hints");
    assert_eq!(hints.len(), 3, "all distinct optional routes stay: {paged}");
    assert!(
        hints.values().all(|call| call["query"]["queries"] == rows),
        "a lead keeps its rows: {paged}"
    );
}

/// Opt-in leads leave before ordering: a default lead emitted after
/// optional ones is not capped away by leads the default output drops.
#[test]
fn a_default_lead_remains_when_opt_in_leads_are_hidden() {
    let lead = |tool: &str| json!({"tool":tool,"query":{"queries":[{"path":"a"}]}});
    let mut object = json!({"next":{
        "ignoreCase":lead("localSearch"),
        "findFile":lead("structureSearch"),
        "narrowScope":lead("localSearch"),
        "viewFileCount":lead("localSearch")
    }})
    .as_object()
    .cloned()
    .expect("object");
    shape(&mut object, ToolId::LocalSearch, false, true);
    let kept = object["hints"]
        .as_object()
        .map(|hints| hints.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    assert!(kept.contains(&"viewFileCount".to_owned()), "{kept:?}");
    assert!(kept.contains(&"narrowScope".to_owned()), "{kept:?}");
    assert!(!kept.contains(&"ignoreCase".to_owned()), "{kept:?}");
}

/// With `extras: ["hints"]` (opt-in leads allowed), a default lead
/// still outranks an opt-in one the tool emitted first: asking for more
/// hints never hides one the default output shows.
#[test]
fn opting_into_hints_keeps_the_default_leads() {
    let lead = |tool: &str| json!({"tool":tool,"query":{"queries":[{"path":"a"}]}});
    let mut object = json!({"next":{
        "read":lead("localFetch"),
        "includeSkipped":lead("localSearch"),
        "viewFileCount":lead("localSearch")
    }})
    .as_object()
    .cloned()
    .expect("object");
    shape(&mut object, ToolId::LocalSearch, false, false);
    let kept = object["hints"]
        .as_object()
        .map(|hints| hints.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    assert_eq!(
        kept,
        ["includeSkipped", "viewFileCount", "read"],
        "{object:?}"
    );
}

/// A terminal page (`terminalLimit`) is partial because the rest is on
/// no page, and no lead reaches it either: its leads are optional like
/// any complete row's, so opt-in and the cap apply.
#[test]
fn a_terminal_page_keeps_the_opt_in_lead_policy() {
    let rows = json!([{"path":"a"},{"path":"b"}]);
    let lead = |tool: &str| json!({"tool":tool,"query":{"queries":rows}});
    let shaped = |terminal: bool| {
        let mut object = json!({
            "isPartial":true,
            "next":{
                "findText":lead("localSearch"),
                "readDefinition":lead("localFetch"),
                "findCallers":lead("lspSearch")
            }
        });
        if terminal {
            object["terminalLimit"] = json!(true);
        }
        let mut object = object.as_object().cloned().expect("object");
        shape(&mut object, ToolId::LspSearch, false, true);
        Value::Object(object)
    };
    let open = shaped(false);
    assert_eq!(
        open["hints"].as_object().map(serde_json::Map::len),
        Some(3),
        "{open}"
    );
    let terminal = shaped(true);
    let kept = terminal["hints"]
        .as_object()
        .map(|hints| hints.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    assert!(
        kept.iter()
            .all(|name| continuation_channels::DEFAULT_LEADS.contains(&name.as_str())),
        "{terminal}"
    );
    assert!(
        terminal
            .get("hints")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|hints| hints.values())
            .all(|call| call["query"]["queries"].as_array().map(Vec::len) == Some(1)),
        "{terminal}"
    );
}

#[test]
fn clasify_keeps_its_walk_in_next_and_moves_reads_to_hints() {
    let read = json!({"tool":"localFetch","query":{"path":"/r/a","ranges":["1-2"]}});
    let mut structured = json!({"queries":[{"id":"q","hints":["Use localSearch."],
            "next":{"clasify":{"resources":[],"questions":[]},"read":read},
            "resources":[{"resourceId":"f","pages":[{"next":{"read":read}}]}]}]});
    split_hints(&mut structured, ToolId::Clasify);
    let query = &structured["queries"][0];
    assert!(query["next"]["clasify"].is_object());
    assert_eq!(
        query["hints"],
        json!({"text":["Use localSearch."],"read":read})
    );
    assert_eq!(
        query["resources"][0]["pages"][0]["hints"],
        json!({"read":read})
    );
}
