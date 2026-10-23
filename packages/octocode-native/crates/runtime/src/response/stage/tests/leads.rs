use super::*;

#[test]
fn a_continuing_row_keeps_its_search_lead() {
    let mut row = fetch_row(0);
    row["data"]["next"]["findText"] = json!({"tool":"localSearch",
            "query":{"queries":[{"path":"/tmp/a.txt","matchString":"hit"}]}});
    let hinted = json!({"path":"/tmp/a.txt","extras":["hints"]});
    let outcome = stage_from(
        "localFetch",
        json!({"results":[row.clone()]}),
        Some(&hinted),
    );
    let data = &outcome.structured_content["results"][0]["data"];
    let input = suggested_input(data, "findText", "localSearch");
    assert_eq!(input["queries"][0]["matchString"], "hit");
    assert!(data["next"].get("findText").is_none());
    assert_eq!(
        data["next"]["continueWindow"]["tool"], "localFetch",
        "{data}"
    );
    let outcome = stage("localFetch", json!({"results":[row]}));
    let data = &outcome.structured_content["results"][0]["data"];
    assert!(data.get("hints").is_none(), "{data}");
    assert_eq!(
        data["next"]["continueWindow"]["tool"], "localFetch",
        "{data}"
    );
}

/// A partial row with no page has no other route to what it withholds:
/// its lead stays in the default output with every row.
#[test]
fn a_partial_row_without_a_page_keeps_its_lead_whole() {
    let outcome = stage("lspSearch", lsp_partial_row(false));
    let data = &outcome.structured_content["results"][0]["data"];
    let input = suggested_input(data, "findText", "localSearch");
    let rows = input["queries"]
        .as_array()
        .unwrap_or_else(|| panic!("lead stays: {data}"));
    let paths: Vec<_> = rows.iter().filter_map(|row| row["path"].as_str()).collect();
    assert_eq!(
        paths,
        [
            "/tmp/f1.ts",
            "/tmp/f2.ts",
            "/tmp/f3.ts",
            "/tmp/f4.ts",
            "/tmp/f5.ts"
        ],
        "{data}"
    );
}

/// A partial row whose `next` pages on reaches the rest by that page,
/// so its optional lead is opt-in like any successful row's.
#[test]
fn a_partial_row_with_a_page_drops_its_optional_lead_by_default() {
    let outcome = stage("lspSearch", lsp_partial_row(true));
    let data = &outcome.structured_content["results"][0]["data"];
    assert_eq!(
        data["next"]["nextImporterPage"]["tool"], "lspSearch",
        "{data}"
    );
    assert!(data.get("hints").is_none(), "{data}");
}

#[test]
fn continuations_stay_compact() {
    let cli = stage("localFetch", json!({"results":[fetch_row(0)]}));
    let next = &cli.structured_content["results"][0]["data"]["next"]["continueWindow"]["query"]["queries"]
        [0];
    assert_eq!(
        next,
        &json!({"path":"/tmp/a.txt","offset":1}),
        "no default is injected into a continuation"
    );
    assert!(!cli.all_failed);
}

/// Hosts show agents structuredContent, so source reads carry their
/// line numbers there; CLI views print the same numbers.
#[test]
fn source_reads_carry_line_numbers_in_structured_content() {
    let row = json!({"index":0,"data":{"path":"a.txt","content":"one\ntwo\n","totalLines":9,
            "sourceLineRanges":[{"line":4,"endLine":5}]}});
    let outcome = stage("localFetch", json!({"results":[row]}));
    let data = &outcome.structured_content["results"][0]["data"];
    assert_eq!(data["content"], "4\tone\n5\ttwo\n", "{data}");
    assert!(data.get("sourceLineRanges").is_none(), "{data}");
    let gh = json!({"results":[{"index":0,"data":{"owner":"o","repo":"r",
            "path":"a.py","content":"x\n","totalLines":3,"commitSha":"abc",
             "sourceLineRanges":[{"line":2,"endLine":2}]}}]});
    let outcome = stage("ghGetFileContent", gh);
    assert_eq!(
        outcome.structured_content["results"][0]["data"]["content"],
        "2\tx\n"
    );
}

/// A classified row's two unfiltered routes (the unknown evidence only,
/// and the whole view) reach different scopes: after channel shaping
/// they are still two separately callable pages with their rows intact.
#[test]
fn the_unknown_and_whole_unfiltered_routes_stay_two_calls() {
    let call = |tool: &str, row: Value| json!({"tool":tool,"query":{"queries":[row]}});
    let cases = [
        (
            "localSearch",
            json!({"path":"/tmp/src","matchString":"fn","include":["/a.rs"]}),
            json!({"path":"/tmp/src","matchString":"fn","snapshot":"s1"}),
            json!({"files":[{"path":"a.rs","matches":[{"line":1,"value":"fn a()"}]}],
                    "matchCount":1,"matchedLineCount":1,"filesScanned":2}),
        ),
        (
            "localFetch",
            json!({"path":"/tmp/a.txt","ranges":["9-20","30-31"]}),
            json!({"path":"/tmp/a.txt","fullContent":true}),
            json!({"path":"a.txt","content":"one\n","totalLines":40}),
        ),
    ];
    for (tool, unknown, whole, mut data) in cases {
        data["next"] = json!({"unfilteredUnknown": call(tool, unknown.clone()),
                "unfiltered": call(tool, whole.clone())});
        let outcome = stage(tool, json!({"results":[{"index":0,"data":data}]}));
        let next = &outcome.structured_content["results"][0]["data"]["next"];
        assert_eq!(
            next["unfilteredUnknown"]["query"]["queries"],
            json!([unknown]),
            "{tool}: {next}"
        );
        assert_eq!(
            next["unfiltered"]["query"]["queries"],
            json!([whole]),
            "{tool}: {next}"
        );
    }
}

#[test]
fn clasify_receipts_keep_their_read_lead_executable() {
    let receipt = json!({"queries":[{"id":"q","model":"jev-1.13.0",
            "answers":{"a":{"type":"noul","noul":0.5}},
            "usage":{"input_tokens":3,"output_tokens":1},
            "hints":{"read":{"tool":"localFetch","query":{"queries":[
                {"path":"/tmp/a","ranges":["1-2"]}]}}}}]});
    let mut structured = receipt.clone();
    super::super::super::continuations::finalize(
        &mut structured,
        ToolId::Clasify,
        &super::super::super::continuations::Sources::Matrices(&json!({"queries":[]})),
        &super::super::super::continuations::Scope::everything(),
    )
    .expect("valid continuations");
    let outcome = finish(
        StageInput {
            allow_auto_paging: false,
            ..input(ToolId::Clasify, structured, ResponsePageOptions::default())
        },
        &json!({"queries":[]}),
        &context(),
    )
    .expect("stage runs")
    .expect("valid envelope");
    // The read of exactly what was judged survives the stage unchanged.
    let returned = &outcome.structured_content["queries"][0];
    assert_eq!(
        suggested_input(returned, "read", "localFetch"),
        receipt["queries"][0]["hints"]["read"]["query"]
    );
    for key in ["id", "model", "answers", "usage"] {
        assert_eq!(returned[key], receipt["queries"][0][key]);
    }
}
