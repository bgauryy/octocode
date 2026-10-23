use super::*;
use crate::contracts::validate_query;

#[test]
fn compaction_drops_only_fields_validation_restores() {
    let row = json!({"matchString":"foo","path":"/tmp","mainGoal":"Find foo.","reasoning":"r","extras":["stats"],
            "caseMode":"smart","matchContentLength":200,"page":2,"contextLines":0});
    let mut out =
        json!({"results":[{"index":0,"data":{"next":{"nextPage":call("localSearch", row)}}}]});
    run(&mut out, ToolId::LocalSearch, &[None]);
    let query = &out["results"][0]["data"]["next"]["nextPage"]["query"]["queries"][0];
    // matchContentLength has no schema default (it scales with
    // contextLines), so validation cannot restore it and it is kept. The
    // brief is request-level: no continuation carries it.
    assert_eq!(
        query,
        &json!({"matchString":"foo","path":"/tmp","extras":["stats"],"matchContentLength":200,"page":2,"contextLines":0})
    );
}

#[test]
fn an_invalid_call_is_dropped_and_its_row_kept() {
    let read = call("localFetch", json!({"path":"/a"}));
    let mut out = json!({"results":[{"index":0,"data":{"files":["a.rs"],
            "next":{"nextPage":call("localSearch", json!({"bogus":true}))},
            "hints":{"read":read}}},
            {"index":1,"data":{"files":["b.rs"],"next":{"nextPage":call("localSearch", json!({"bogus":true}))}}}]});
    finalize(
        &mut out,
        ToolId::LocalSearch,
        &Sources::Rows(&[None, None]),
        &Scope::everything(),
    )
    .expect_err("invalid call");
    for row in out["results"].as_array().expect("rows") {
        let data = &row["data"];
        assert!(data["files"].is_array(), "the evidence stays: {row}");
        assert!(
            data.get("next").is_none(),
            "the invalid lead is gone: {row}"
        );
        assert_eq!(data["partialReasons"], json!([LEAD_INVALID]), "{row}");
        assert_eq!(data["isPartial"], true, "{row}");
        let warning = data["warnings"][0].as_str().expect("warning");
        assert!(warning.len() <= 120, "{warning}");
    }
}

#[test]
fn an_invalid_call_is_an_error_with_its_path() {
    let mut out = json!({"results":[{"index":0,"data":{"next":{"nextPage":call("localSearch", json!({"bogus":true}))}}}]});
    let error = finalize(
        &mut out,
        ToolId::LocalSearch,
        &Sources::Rows(&[None]),
        &Scope::everything(),
    )
    .expect_err("invalid call");
    let path = &error.issues[0].path;
    assert_eq!(
        &path[..6],
        ["results", "0", "data", "next", "nextPage", "query"]
    );
}

/// Trying only restorable or dropped fields compacts exactly like trying
/// every field.
/// A restart restates no contract default (`pageSize` of an issue read).
#[test]
fn a_restart_drops_the_default_page_size() {
    let query = json!({"operation":"issue","owner":"o","repo":"r","number":7,
            "sections":["comments"],"page":40});
    let full = validate_query("ghGetHistoryItem", query).expect("valid");
    let mut out = json!({"results":[{"index":0,"data":{"next":{
        "restart": crate::response::result::restart_query(ToolId::GhGetHistoryItem, &full)
    }}}]});
    run(&mut out, ToolId::GhGetHistoryItem, &[Some(&full)]);
    let row = &out["results"][0]["data"]["next"]["restart"]["query"]["queries"][0];
    assert!(row.get("pageSize").is_none(), "{row}");
}

#[test]
fn restorable_fields_compact_like_trying_every_field() {
    fn every_field(tool: &str, query: &Value) -> Value {
        let full = validate_query(tool, query.clone()).expect("valid");
        let mut compact = query.as_object().cloned().unwrap_or_default();
        for field in query
            .as_object()
            .into_iter()
            .flatten()
            .map(|(field, _)| field)
        {
            let Some(removed) = compact.remove(field) else {
                continue;
            };
            if validate_query(tool, Value::Object(compact.clone())).ok() != Some(full.clone()) {
                compact.insert(field.clone(), removed);
            }
        }
        query
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(field, _)| compact.get(field).map(|v| (field.clone(), v.clone())))
            .collect::<Map<_, _>>()
            .into()
    }
    for (tool, query) in [
        (
            "localSearch",
            json!({"matchString":"foo","path":"/tmp","mainGoal":"g","extras":["stats"],
                "caseMode":"smart","page":1,"matchContentLength":200,"contextLines":0,"sort":"relevance"}),
        ),
        (
            "localFetch",
            json!({"path":"/a","matchString":"x","reasoning":" ","caseMode":"smart"}),
        ),
        (
            "ghSearchCode",
            json!({"owner":"o","repo":"r","keywords":["a"],"page":1,"extras":["stats"]}),
        ),
        (
            "structureSearch",
            json!({"operation":"tree","path":"/a","extras":["receipts","echo"]}),
        ),
    ] {
        let mut compact = query.clone();
        compact_query(tool, &mut compact, &mut Memo::new()).expect("valid");
        assert_eq!(compact, every_field(tool, &query), "{tool}");
    }
}

#[test]
fn unavailable_cross_tool_calls_drop_silently_and_own_pages_stay() {
    let mut out = json!({"results":[
        {"index":0,"data":{"next":{
            "nextPage":call("ghStructure", json!({"owner":"o","repo":"r","page":2})),
            "materialized":call("localSearch", json!({"path":"/x","matchString":"a"}))
        },"warnings":["tool warning"]}},
        {"index":1,"data":{"next":{"read":call("localFetch", json!({"path":"/a"}))}}}
    ]});
    let scope = Scope {
        is_available: &|_| false,
    };
    finalize(
        &mut out,
        ToolId::GhStructure,
        &Sources::Rows(&[None, None]),
        &scope,
    )
    .expect("valid");
    let first = &out["results"][0]["data"];
    assert!(
        first["next"]["nextPage"].is_object(),
        "own pages stay: {first}"
    );
    assert!(first.get("hints").is_none(), "{first}");
    assert_eq!(first["warnings"], json!(["tool warning"]));
    assert!(
        out["results"][1]["data"].get("warnings").is_none(),
        "{}",
        out["results"][1]
    );
}

#[test]
fn an_empty_row_that_loses_its_only_lead_to_the_surface_keeps_a_recovery_hint() {
    let query = json!({"owner":"o","repo":"r","keywords":["zzz"]});
    let mut out = json!({"results":[{"index":0,"status":"empty","data":{"next":{
        "cloneRepo":call("ghCloneRepo", json!({"owner":"o","repo":"r"}))
    }}}]});
    let scope = Scope {
        is_available: &|_| false,
    };
    finalize(
        &mut out,
        ToolId::GhSearchCode,
        &Sources::Rows(&[Some(&query)]),
        &scope,
    )
    .expect("valid");
    let data = &out["results"][0]["data"];
    assert!(data.get("next").is_none(), "{data}");
    assert_eq!(
        data["hints"],
        json!({(crate::id::continuation_channels::HINT_TEXT_KEY): [ToolId::GhSearchCode.output().fallback_hint(&query)]}),
        "{data}"
    );
}

#[test]
/// A recovery lead stays only on an empty or failed row. A lead carries
/// no prose: the tool description states what each lead kind is for.
fn recovery_leads_stay_only_on_empty_or_failed_rows() {
    let tree = call("ghStructure", json!({"owner":"o","repo":"r","path":"src"}));
    let read = call(
        "ghGetFileContent",
        json!({"owner":"o","repo":"r","path":"a"}),
    );
    let row = |status: &str| json!({"index":0,"status":status,"data":{"next":{"viewTree":tree.clone(),"read":read.clone()}}});
    let mut ok = json!({"results":[row("ok")]});
    run(&mut ok, ToolId::GhSearchCode, &[None]);
    let data = &ok["results"][0]["data"];
    assert!(
        data.get("hints")
            .and_then(|hints| hints.get("viewTree"))
            .is_none(),
        "{data}"
    );
    let mut empty = json!({"results":[row("empty")]});
    run(&mut empty, ToolId::GhSearchCode, &[None]);
    let hints = &empty["results"][0]["data"]["hints"];
    for name in ["viewTree", "read"] {
        assert_eq!(
            hints[name]
                .as_object()
                .map(|call| call.keys().cloned().collect::<Vec<_>>()),
            Some(vec!["tool".to_owned(), "query".to_owned()]),
            "{name}: {hints}"
        );
    }
}
