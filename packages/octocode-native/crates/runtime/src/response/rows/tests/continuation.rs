use super::*;

/// Never-trim invariant: a clipped value must carry a continuation that
/// reaches it. A read of another file, or a page of more rows, does not.
#[test]
fn a_clipped_value_needs_a_continuation_that_reaches_it() {
    let read = |path: &str| json!({"tool":"localFetch","query":{"queries":[{"path":path,"ranges":["1-2"]}]}});
    let page = |next: Value| {
        json!({"files":[
            {"path":"a.rs","matches":[{"line":1,"value":"short"}]},
            {"path":"b.rs","matches":[{"line":4,"value":"long…","clipped":true,"originalChars":900}]}
        ],"next":next})
    };
    let codes = |data: Value| {
        result_row(
            ToolId::from_name("localSearch").expect("known tool"),
            0,
            &json!({"extras":["diagnostics"]}),
            data,
            None,
        )
        .pointer("/meta/diagnostics/codes")
        .cloned()
    };
    let missing = Some(json!(["continuationMissing"]));
    assert_eq!(codes(page(json!({"read":read("a.rs")}))), missing);
    assert_eq!(
        codes(page(
            json!({"read":read("a.rs"),"nextPage":{"tool":"localSearch","query":{"queries":[{"path":".","matchString":"x","page":2}]}}})
        )),
        missing
    );
    assert_eq!(codes(page(json!({"read":read("b.rs")}))), None);
    // An absolute row path and a relative continuation name one file.
    let absolute = |next: Value| {
        let mut data = page(next);
        data["files"][1]["path"] = json!("/repo/src/b.rs");
        data
    };
    assert_eq!(codes(absolute(json!({"read":read("src/b.rs")}))), None);
    assert_eq!(codes(absolute(json!({"read":read("c/b.rs")}))), missing);
    assert_eq!(codes(absolute(json!({"read":read("rc/b.rs")}))), missing);
    assert_eq!(
        codes(page(
            json!({"expandValues":{"tool":"localSearch","query":{"queries":[{"path":".","matchString":"x","matchContentLength":900}]}}})
        )),
        None
    );
}

/// Every tool's stale snapshot is the same failure: one text and a
/// `next.restart` that reads page 1 of the same query on the current
/// source. A restart the tool built is kept.
#[test]
fn a_stale_snapshot_always_restarts_with_one_text() {
    let query =
        json!({"path":"src","operation":"match","pattern":"f($A)","page":3,"snapshot":"s1"});
    let row = result_row(
        ToolId::AstSearch,
        0,
        &query,
        json!({"errorCode":"staleSnapshot","error":"tool wording","snapshot":"s2","isPartial":true}),
        Some("error"),
    );
    let data = &row["data"];
    assert_eq!(
        data["error"],
        super::super::super::pages::STALE_SNAPSHOT_ERROR,
        "{data}"
    );
    assert!(
        data.get("isPartial").is_none(),
        "a restart is not a remaining page: {data}"
    );
    let restart = &data["next"]["restart"];
    assert_eq!(restart["tool"], "astSearch", "{data}");
    assert_eq!(
        restart["query"]["queries"][0],
        json!({"path":"src","operation":"match","pattern":"f($A)"}),
        "page 1 of the same query: {data}"
    );
    let own = json!({"tool":"localFetch","query":{"queries":[{"path":"a","offset":0}]}});
    let row = result_row(
        ToolId::LocalFetch,
        0,
        &json!({"path":"a","offset":90,"snapshot":"s"}),
        json!({"errorCode":"staleSnapshot","error":"x","next":{"restart":own.clone()}}),
        Some("error"),
    );
    assert_eq!(row["data"]["next"]["restart"], own);
    assert_eq!(
        row["data"]["error"],
        super::super::super::pages::STALE_SNAPSHOT_ERROR
    );
}

/// A stale row ends the page: its pagination says `hasMore:false` and no
/// page continuation survives beside the restart.
#[test]
fn a_stale_snapshot_row_ends_the_page() {
    let query = json!({"path":"src","matchString":"f","page":2,"snapshot":"s1"});
    let row = result_row(
        ToolId::LocalSearch,
        0,
        &query,
        json!({
            "errorCode":"staleSnapshot","error":"x",
            "pagination":{"currentPage":2,"totalPages":5,"hasMore":true},
            "next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"page":3}]}}}
        }),
        Some("error"),
    );
    let data = &row["data"];
    assert_eq!(data["pagination"]["hasMore"], false, "{data}");
    assert!(data["next"].get("nextPage").is_none(), "{data}");
    assert_eq!(data["next"]["restart"]["tool"], "localSearch", "{data}");
}

#[test]
fn incomplete_evidence_requires_executable_continuation_or_terminal_diagnostic() {
    let missing = result_row(
        ToolId::from_name("astSearch").expect("known tool"),
        0,
        &json!({"operation":"symbols","extras":["diagnostics"]}),
        json!({"hasMore":true,"nextPage":2}),
        None,
    );
    assert_eq!(
        missing.pointer("/meta/diagnostics/codes"),
        Some(&json!(["continuationMissing"]))
    );
    let terminal = result_row(
        ToolId::from_name("localFetch").expect("known tool"),
        0,
        &json!({"extras":["diagnostics"]}),
        json!({"isPartial":true,"terminalLimit":true}),
        None,
    );
    assert_eq!(
        terminal.pointer("/meta/diagnostics/codes"),
        Some(&json!(["terminalLimitReached"]))
    );
}

/// A row that offers more of its own result (`next.next*`, `continue*`,
/// `expand*`, `retry*`) is not complete, whatever the tool reported.
#[test]
fn a_row_with_a_next_page_is_never_complete() {
    let page = json!({"tool":"astSearch","query":{"queries":[{"path":".","pattern":"f($A)","scanOffset":100}]}});
    let row = result_row(
        ToolId::AstSearch,
        0,
        &json!({"extras":["diagnostics"]}),
        json!({"files":[],"next":{"expandScan":page}}),
        None,
    );
    assert_eq!(row.pointer("/data/isPartial"), Some(&json!(true)), "{row}");
    assert_eq!(
        row.pointer("/meta/diagnostics/partial"),
        Some(&json!(true)),
        "{row}"
    );
    let flagless = result_row(
        ToolId::LocalFetch,
        0,
        &json!({}),
        json!({"path":"a","isPartial":false,"next":{"continueWindow":page}}),
        None,
    );
    assert_eq!(
        flagless.pointer("/data/isPartial"),
        Some(&json!(true)),
        "{flagless}"
    );
    // A file's own match pagination says more beside the `next` page that
    // answers it (SH-4: nested blocks count): the row need not restate it.
    let nested = result_row(
        ToolId::AstSearch,
        0,
        &json!({}),
        json!({"files":[{"path":"a","matches":[],"pagination":{"hasMore":true}}],
            "next":{"nextMatchPage":page}}),
        None,
    );
    assert!(nested.pointer("/data/isPartial").is_none(), "{nested}");
    assert_eq!(
        nested.pointer("/data/files/0/pagination/hasMore"),
        Some(&json!(true)),
        "{nested}"
    );
    // A restart is a follow-up, not unread rest.
    let preview = result_row(
        ToolId::AstSearch,
        0,
        &json!({}),
        json!({"coverage":{"state":"complete"},"next":{"restart":page}}),
        None,
    );
    assert_eq!(
        preview.pointer("/data/coverage/state"),
        Some(&json!("complete")),
        "{preview}"
    );
    assert!(preview.pointer("/data/isPartial").is_none(), "{preview}");
}

#[test]
fn an_out_of_range_page_is_one_shared_outcome() {
    let lsp = json!({"operation":"references","path":"a.rs","symbolName":"x","line":1});
    for (tool, base) in [
        (ToolId::LocalSearch, json!({"path":".","matchString":"x"})),
        (ToolId::LspSearch, lsp),
        (ToolId::StructureSearch, json!({"path":"."})),
        (
            ToolId::AstSearch,
            json!({"path":".","operation":"match","pattern":"x"}),
        ),
        (ToolId::LocalFetch, json!({"path":"a.rs"})),
    ] {
        let mut query = base.clone();
        query["page"] = json!(99);
        query["snapshot"] = json!("s");
        // A tool's own partial signals on a page past the end restate
        // nothing: no page is left to read, `next.restart` is the lead.
        let data = json!({"files":[],"isPartial":true,"terminalLimit":true,
            "partialReasons":["pageOutOfRange"],"pagination":{"currentPage":99,
            "totalPages":4,"totalItems":10,"hasMore":false,"outOfRange":true}});
        let row = result_row(tool, 0, &query, data, None);
        assert_eq!(row["status"], "outOfRange", "{tool}: {row}");
        for signal in ["isPartial", "terminalLimit", "partialReasons"] {
            assert!(row["data"].get(signal).is_none(), "{tool} {signal}: {row}");
        }
        assert_eq!(row["data"]["reason"], "page 99 of 4", "{tool}: {row}");
        assert_eq!(
            row["data"]["next"]["restart"]["query"]["queries"][0], base,
            "{tool}: {row}"
        );
        assert!(row["data"].get("error").is_none(), "{tool}: {row}");
    }
    // A tool that already led with its own restart keeps it.
    let own = json!({"files":[],"next":{"restart":{"tool":"localFetch","query":{"queries":[{"path":"x"}]}}},
        "pagination":{"currentPage":3,"totalPages":2,"hasMore":false,"outOfRange":true}});
    let row = result_row(ToolId::LocalFetch, 0, &json!({"path":"a.rs"}), own, None);
    assert_eq!(
        row["data"]["next"]["restart"]["query"]["queries"][0]["path"], "x",
        "{row}"
    );
    // An in-range last page is a plain result, never this outcome.
    let last = json!({"files":[{"path":"a"}],"pagination":{"currentPage":4,"totalPages":4,"hasMore":false}});
    let row = result_row(
        ToolId::LocalSearch,
        0,
        &json!({"path":".","matchString":"x"}),
        last,
        None,
    );
    assert!(row.get("status").is_none(), "{row}");
    assert!(row["data"].get("reason").is_none(), "{row}");
}

/// A section page past the end (`contentPagination.files`) settles like the row's
/// own `pagination`: an empty row, the page named, and a restart.
#[test]
fn a_file_page_past_the_end_is_the_same_shared_out_of_range_outcome() {
    let query = json!({"operation":"commit","owner":"o","repo":"r","ref":"abc","page":99});
    let data = json!({"commitSha":"abc","files":[],"isPartial":true,
        "contentPagination":{"files":{"currentPage":99,"totalPages":1,"pageSize":30,"totalItems":2,
            "hasMore":false,"outOfRange":true}}});
    let row = result_row(ToolId::GhGetHistoryItem, 0, &query, data, None);
    assert_eq!(row["status"], "outOfRange", "{row}");
    assert_eq!(row["data"]["reason"], "page 99 of 1", "{row}");
    assert!(row["data"].get("isPartial").is_none(), "{row}");
    let restart = &row["data"]["next"]["restart"]["query"]["queries"][0];
    assert_eq!(restart["ref"], "abc", "{row}");
    assert!(restart.get("page").is_none(), "{row}");
}
