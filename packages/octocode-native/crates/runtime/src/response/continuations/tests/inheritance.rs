use super::*;

/// Every lead and page a tool test publishes is named for a kind the
/// contract declares (or for the tool it resumes): the contract owns the
/// vocabulary, and the shared output check enforces it.
#[test]
fn emitted_names_must_be_declared_by_the_contract() {
    use crate::tools::test_support::checks::assert_lead_names_declared;
    let ok = json!({"results":[{"data":{
        "next":{"nextPage":call("localSearch", json!({"path":"a"}))},
        "hints":["readHits2: localFetch {\"queries\":[{\"path\":\"a\",\"mainGoal\":\"g\",\"reasoning\":\"r\"}]}"]
    }}]});
    assert_lead_names_declared(&ok);
    let resumed = json!({"results":[{"data":{"next":{"localSearch":call("localSearch", json!({"path":"a"}))}}}]});
    assert_lead_names_declared(&resumed);
    let undeclared =
        json!({"results":[{"data":{"next":{"notAKind":call("localFetch", json!({"path":"a"}))}}}]});
    let failed = std::panic::catch_unwind(|| assert_lead_names_declared(&undeclared));
    assert!(failed.is_err(), "an undeclared name is refused");
}

#[test]
fn same_tool_calls_inherit_only_extras_cross_tool_calls_only_hints() {
    let first = json!({"mainGoal":"Find a.","reasoning":"First.","extras":["hints","stats"]});
    let second = json!({"mainGoal":"Find b.","extras":[]});
    let mut out = json!({"results":[
        {"index":0,"data":{"next":{
            "nextPage":call("ghSearchCode", json!({"owner":"o","keywords":["a"],"page":2})),
            "viewRepo":call("ghStructure", json!({"owner":"o","repo":"r"}))
        }}},
        {"index":1,"data":{"items":[{"next":{
            "nextPage":call("ghSearchCode", json!({"owner":"o","keywords":["b"],"page":2})),
            "read":call("localFetch", json!({"path":"/a","offset":10}))
        }}]}}
    ]});
    run(
        &mut out,
        ToolId::GhSearchCode,
        &[Some(&first), Some(&second)],
    );
    let page = &out["results"][0]["data"]["next"]["nextPage"]["query"]["queries"][0];
    assert!(
        page.get("mainGoal").is_none() && page.get("reasoning").is_none(),
        "a page carries no brief: {page}"
    );
    assert_eq!(page["extras"], json!(["hints", "stats"]), "{page}");
    let lead = &out["results"][0]["data"]["hints"]["viewRepo"]["query"]["queries"][0];
    assert!(lead.get("mainGoal").is_none(), "{lead}");
    assert_eq!(
        lead["extras"],
        json!(["hints"]),
        "a cross-tool lead keeps only the lead opt-in"
    );
    let items = &out["results"][1]["data"]["items"][0];
    let page = &items["next"]["nextPage"]["query"]["queries"][0];
    assert_eq!(
        page,
        &json!({"owner":"o","keywords":["b"],"page":2}),
        "an empty extras array is the default"
    );
    let read = &items["hints"]["read"]["query"]["queries"][0];
    assert!(read.get("extras").is_none(), "{read}");
}

#[test]
fn selection_pages_keep_the_brief_but_unfiltered_recovery_drops_it() {
    let source = json!({"owner":"o","keywords":["a"],"mainGoal":"Find capacity ownership.","reasoning":"Inspect the release path.","clasifyContext":{"classifiers":["Where is reserved capacity released?"]}});
    let mut page = source.clone();
    page["page"] = json!(2);
    let mut recovery = source.clone();
    recovery.as_object_mut().unwrap().remove("clasifyContext");
    let mut out = json!({"results":[{"index":0,"data":{"next":{
        "nextPage":call("ghSearchCode",page),
        "unfiltered":call("ghSearchCode",recovery)
    }}}]});
    run(&mut out, ToolId::GhSearchCode, &[Some(&source)]);
    let next = &out["results"][0]["data"]["next"];
    let page = &next["nextPage"]["query"]["queries"][0];
    assert_eq!(page["mainGoal"], source["mainGoal"]);
    assert_eq!(page["reasoning"], source["reasoning"]);
    let recovery = &next["unfiltered"]["query"]["queries"][0];
    assert!(recovery.get("mainGoal").is_none());
    assert!(recovery.get("reasoning").is_none());
    assert!(recovery.get("clasifyContext").is_none());
}

/// A tool that builds its page by cloning the row query copies the brief
/// with it; the one continuation stage drops it there too, on pages and
/// leads alike, so no tool has to remember to.
#[test]
fn a_brief_a_tool_copied_into_its_continuation_is_dropped() {
    let source = json!({"mainGoal":"Find a.","reasoning":"First."});
    let copied = |page: u32| json!({"matchString":"a","path":"/r","mainGoal":"Find a.","reasoning":"First.","page":page});
    let mut out = json!({"results":[{"index":0,"data":{
        "next":{"nextPage":call("localSearch", copied(2))},
        "hints":{
            "readFile":call("localFetch", json!({"path":"/r/a","mainGoal":"Find a.","reasoning":"First."}))
        }
    }}]});
    run(&mut out, ToolId::LocalSearch, &[Some(&source)]);
    let data = &out["results"][0]["data"];
    for (name, row) in [
        ("nextPage", &data["next"]["nextPage"]["query"]["queries"][0]),
        (
            "readFile",
            &data["hints"]["readFile"]["query"]["queries"][0],
        ),
    ] {
        assert!(
            row.get("mainGoal").is_none() && row.get("reasoning").is_none(),
            "{name} carries no brief: {row}"
        );
    }
}

/// The brief is request-level: copied into every row of a page it was
/// the largest token sink of a walk. Measured on this five-row page with
/// an 87-char `mainGoal` and a 97-char `reasoning` (serde_json bytes of
/// the page's `query`): 1343 with the brief on every row, 298 without it
/// (-1045 bytes, 78%); the inherited `extras` is 19 of those bytes per
/// row, and a 500-char brief would have cost ~5 KB per five-row page.
#[test]
fn a_five_row_page_carries_no_brief() {
    let source = json!({
        "mainGoal":"Find where the response stage inherits the brief into continuation rows across tools.",
        "reasoning":"The walk pages a wide search; every page repeated the brief in each of its rows, so measure it.",
        "extras":["stats"]
    });
    let rows: Vec<Value> = (1..=5)
        .map(|index| json!({"path":format!("src/f{index}.rs"),"ranges":["1-9"]}))
        .collect();
    let mut out = json!({"results":[{"index":0,"data":{"next":{
        "readHits":{"tool":"localFetch","query":{"queries":rows.clone()}}
    }}}]});
    run(&mut out, ToolId::LocalFetch, &[Some(&source)]);
    let page = &out["results"][0]["data"]["next"]["readHits"]["query"];
    let without = serde_json::to_vec(page).expect("json").len();
    let mut briefed = page.clone();
    for row in briefed["queries"].as_array_mut().expect("rows") {
        row["mainGoal"] = source["mainGoal"].clone();
        row["reasoning"] = source["reasoning"].clone();
    }
    let with = serde_json::to_vec(&briefed).expect("json").len();
    for row in page["queries"].as_array().expect("rows") {
        assert_eq!(row["extras"], json!(["stats"]), "{page}");
        assert!(row.get("mainGoal").is_none(), "{page}");
    }
    assert!(
        with > without * 4,
        "brief on every row: {with} bytes, extras only: {without} bytes"
    );
}

/// A successful row offers only the contract's default leads unless its
/// `extras` names `hints`; empty and error rows keep every lead, and an
/// item's leads (inside a row's array) always stay.
#[test]
fn row_level_leads_of_a_successful_row_are_opt_in() {
    let next = || {
        json!({
            "nextPage":call("ghSearchCode", json!({"owner":"o","keywords":["a"],"page":2})),
            "viewRepo":call("ghStructure", json!({"owner":"o","repo":"r"})),
            "includeArchived":call("ghSearchCode", json!({"owner":"o","keywords":["a"]}))
        })
    };
    let plain = json!({"mainGoal":"Find a."});
    let hinted = json!({"mainGoal":"Find a.","extras":["hints"]});
    let mut out = json!({"results":[
        {"index":0,"data":{"next":next()}},
        {"index":1,"data":{"next":next()}},
        {"index":2,"status":"empty","data":{"next":next()}},
        {"index":3,"data":{"items":[{"next":next()}]}}
    ]});
    run(
        &mut out,
        ToolId::GhSearchCode,
        &[Some(&plain), Some(&hinted), Some(&plain), Some(&plain)],
    );
    let default = &out["results"][0]["data"];
    assert!(
        default["next"]["nextPage"].is_object(),
        "pages stay: {default}"
    );
    assert!(
        default["hints"].get("viewRepo").is_none(),
        "a row-level lead is opt-in: {default}"
    );
    assert!(
        default["hints"]["includeArchived"].is_object(),
        "a contract default lead stays: {default}"
    );
    assert!(
        out["results"][1]["data"]["hints"]["viewRepo"].is_object(),
        "extras:[hints] keeps the lead: {}",
        out["results"][1]
    );
    assert!(
        out["results"][2]["data"]["hints"]["viewRepo"].is_object(),
        "an empty row keeps its leads: {}",
        out["results"][2]
    );
    assert!(
        out["results"][3]["data"]["items"][0]["hints"]["viewRepo"].is_object(),
        "an item's leads stay: {}",
        out["results"][3]
    );
}

#[test]
fn a_clasify_read_lead_is_its_result_and_always_shows() {
    let input = json!({"queries":[{"id":"m1","mainGoal":"G1"},{"id":"m2","mainGoal":"G2"}]});
    let mut out = json!({"queries":[{"id":"m2","answers":{},
            "hints":{"read":call("localFetch", json!({"path":"/a","ranges":["1-2"]}))}}]});
    finalize(
        &mut out,
        ToolId::Clasify,
        &Sources::Matrices(&input),
        &Scope::everything(),
    )
    .expect("valid");
    let read = &out["queries"][0]["hints"]["read"];
    assert_eq!(
        read["query"]["queries"][0]["path"], "/a",
        "shown without extras hints: {read}"
    );
    assert!(
        read["query"]["queries"][0].get("mainGoal").is_none(),
        "{read}"
    );
}
