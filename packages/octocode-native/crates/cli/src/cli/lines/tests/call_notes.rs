//! Field and match notes, and which calls print as commands.
use super::*;

#[test]
fn nested_file_fields_are_words() {
    let paged = json!({"results":[{"index":0,"data":{"files":[{"path":"a.rs","matches":[{"line":3,"value":"x"}],
            "pagination":{"totalItems":471,"hasMore":true,"moreLines":[44,49],"moreLinesUnlisted":437}}]}}]});
    let lines = render(ToolId::LocalSearch, &paged, &json!({}), None).expect("lines");
    assert_eq!(
        lines.notes,
        "# file a.rs: pagination (totalItems 471 hasMore moreLines 44,49 moreLinesUnlisted 437)"
    );
}

#[test]
fn match_notes_are_words() {
    let hit = json!({"results":[{"index":0,"data":{"files":[{"path":"a.rs","matches":[
            {"line":3,"value":"fn a()","declaration":true}]}]}}]});
    let lines = render(ToolId::LocalSearch, &hit, &json!({}), None).expect("lines");
    assert_eq!(lines.out, "a.rs:3:fn a()  [declaration]");
}

#[test]
fn only_schema_declared_calls_become_commands_and_source_evidence_stays_exact() {
    let source =
        json!({"kind":"continue","tool":"localFetch","input":{"cursor":"source-lookalike"}});
    let structured = json!({"results":[{"index":0,"data":{
        "content":"localSearch {\"queries\":[{\"path\":\".\"}]}",
        "hints":["localSearch {\"queries\":[]}"],
        "source":source,
        "next":{"read":{"kind":"continue","tool":"localFetch","input":{"cursor":"issued"}}},
        "related":[{"kind":"related","tool":"localSearch","input":{"queries":[{"path":".","matchString":["it's"]}]}}]
    }}]});
    let (display, calls) = follow::separate_calls(ToolId::LocalFetch, &structured).expect("schema");
    assert_eq!(calls.len(), 2);
    assert!(calls[0].contains("issued"));
    assert!(calls[1].contains("octocode localSearch"));
    assert!(calls.iter().all(|call| !call.contains("source-lookalike")));
    assert_eq!(display["results"][0]["data"]["source"], source);
    assert_eq!(
        display["results"][0]["data"]["content"],
        structured["results"][0]["data"]["content"]
    );
    assert_eq!(
        display["results"][0]["data"]["hints"],
        structured["results"][0]["data"]["hints"]
    );
    assert!(display["results"][0]["data"].get("next").is_none());
    assert!(display["results"][0]["data"].get("related").is_none());
}

#[test]
fn captured_resource_reads_are_commands_but_their_sources_are_evidence() {
    let source = json!({"tool":"localFetch","query":{"path":"a.rs"}});
    let read = json!({"kind":"continue","tool":"localFetch","input":{"cursor":"captured-source"}});
    let sections = json!({"results":[{"index":0,"data":{"operation":"sections","resources":{"file":{
            "status":"ready","source":source,"read":read}}}}]});
    let (display, calls) = follow::separate_calls(ToolId::Clasify, &sections).expect("schema");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].contains("captured-source"));
    assert_eq!(
        display["results"][0]["data"]["resources"]["file"]["source"],
        source
    );
}

#[test]
fn opaque_search_resumes_print_every_field_when_the_context_mode_is_unknown() {
    let lines = render(
        ToolId::LocalSearch,
        &sample(),
        &json!({"cursor":"issued"}),
        None,
    )
    .expect("text");
    for evidence in ["25\tuse a;", "26\tuse SearcherBuilder;", "997\t  let sb"] {
        assert!(lines.out.contains(evidence), "{}", lines.out);
    }
    for token in ["issued-tool-page", "issued-response-page"] {
        assert_eq!(lines.notes.matches(token).count(), 1, "{}", lines.notes);
    }
}

#[test]
fn batches_and_other_tools_print_text() {
    let two = json!({"results":[{"index":0,"data":{"files":[]}},{"index":1,"data":{"files":[]}}]});
    let lines = render(ToolId::LocalSearch, &two, &json!({}), None).expect("text");
    assert_eq!(lines.out, "## results[0]\n## results[1]");
    let lines = render(ToolId::LocalFetch, &sample(), &json!({}), None).expect("text");
    assert!(
        lines.out.contains("26\tuse SearcherBuilder;"),
        "{}",
        lines.out
    );
    assert!(all_rows_failed(&json!({"results":[{"status":"error"}]})));
    assert!(!all_rows_failed(
        &json!({"results":[{"status":"error"},{"data":{}}]})
    ));
}
