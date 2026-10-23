use super::*;

#[test]
fn envelope_unions_keep_each_data_requirement_combination() {
    let schema = json!({
        "$defs":{"rows":{"type":"array","items":{"properties":{"data":{
            "allOf":[
                {"required":["base"]},
                {"oneOf":[{"required":["left"]},{"required":["right"]}]}
            ]
        }}}}},
        "anyOf":[
            {"required":["error","errorCode"]},
            {"properties":{"results":{"$ref":"#/$defs/rows"}}}
        ]
    });
    let mut variants = data_variants_of(&schema)
        .into_iter()
        .map(|fields| fields.into_iter().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    variants.sort();
    assert_eq!(variants, vec![vec!["base", "left"], vec!["base", "right"]]);
}

#[test]
fn minimal_rows_keep_every_field_the_contract_requires() {
    let history = contract_data_variants(ToolId::GhSearchHistory);
    assert!(
        history
            .iter()
            .all(|variant| variant.contains("pullRequests")
                || variant.contains("issues")
                || variant.contains("commits")
                || variant.contains("error")),
        "{history:?}"
    );
    // A clone row requires its checkout, not the caller's own owner/repo.
    let clone = contract_data_variants(ToolId::GhCloneRepo);
    assert!(
        clone
            .iter()
            .any(|variant| variant.contains("location") && variant.contains("bytes")),
        "{clone:?}"
    );
    let row = minimized(
        ToolId::GhSearchHistory,
        json!({"operation":"issue","owner":"o","repo":"r"}),
        json!({"owner":"o","repo":"r","issues":[],"effectiveQuery":"is:issue"}),
    );
    assert_eq!(row["data"]["issues"], json!([]), "{row}");
    assert!(row["data"].get("effectiveQuery").is_none(), "{row}");
    let clone_row = minimized(
        ToolId::GhCloneRepo,
        json!({"owner":"o","repo":"r"}),
        json!({"bytes":1,"location":{"localPath":"/x"}}),
    );
    assert_eq!(
        clone_row["data"]["location"]["localPath"], "/x",
        "{clone_row}"
    );
    assert_eq!(clone_row["data"]["bytes"], 1, "{clone_row}");
}

/// The last page of a window read keeps its pagination: its offset
/// places the text, and a byte unit keeps that text verbatim.
#[test]
fn a_later_window_page_keeps_its_pagination() {
    let page = json!({"unit":"bytes","offset":8000,"length":12,"hasMore":false});
    let last = minimized(
        ToolId::GhGetFileContent,
        json!({}),
        json!({"path":"a","content":"x","pagination":page.clone()}),
    );
    assert_eq!(last["data"]["pagination"], page, "{last}");
    let whole = minimized(
        ToolId::LocalFetch,
        json!({}),
        json!({"path":"a","content":"x","pagination":{"unit":"lines","offset":0,"length":1,"hasMore":false}}),
    );
    assert!(whole["data"].get("pagination").is_none(), "{whole}");
}

/// Every tool's pagination block keeps the same keys on every page
/// (where it stands, its size, what is known of the total, whether more
/// exists) and drops only what `next` carries or what asserts nothing.
#[test]
fn pagination_blocks_keep_stable_keys_on_every_tool() {
    let next =
        json!({"nextPage":{"tool":"ghSearchCode","query":{"queries":[{"owner":"o","page":2}]}}});
    for tool in [ToolId::GhSearchCode, ToolId::LocalSearch, ToolId::LspSearch] {
        // The block is a stats extra; asked for, its keys are stable.
        let row = minimized(
            tool,
            json!({"extras":["stats"]}),
            json!({"files":[{"path":"a"}],"next":next.clone(),"pagination":{"currentPage":1,"totalPages":2,"pageSize":5,"totalItems":7,
                "totalItemsCapped":false,"hasMore":true,"nextPage":2,"snapshot":"s","resultId":"r"}}),
        );
        assert_eq!(
            row["data"]["pagination"],
            json!({"currentPage":1,"totalPages":2,"pageSize":5,"totalItems":7,"hasMore":true}),
            "{tool}"
        );
    }
    let capped = minimized(
        ToolId::GhSearchRepo,
        json!({"extras":["stats"]}),
        json!({"repositories":[],"pagination":{"totalItems":1000,"totalItemsCapped":true,"hasMore":true,"currentPage":3},
            "providerLimit":{"reason":"providerResultCap","maxResults":1000}}),
    );
    assert_eq!(
        capped["data"]["pagination"],
        json!({"totalItems":1000,"hasMore":true,"currentPage":3}),
        "a true cap repeats providerLimit"
    );
}

/// Which rows carry scan totals is the contract's `scanTotals` for the tool,
/// not a name or trait branch: every localSearch row, an astSearch row only
/// beside `matchCount`, and no row of a tool without the marker.
#[test]
fn scan_totals_follow_the_contract_marker() {
    let totals = |tool, data: Value| reports_scan_totals(tool, data.as_object().expect("row"));
    assert!(totals(ToolId::LocalSearch, json!({"filesScanned": 3})));
    assert!(totals(ToolId::AstSearch, json!({"matchCount": 1})));
    assert!(!totals(ToolId::AstSearch, json!({"filesScanned": 3})));
    assert!(!totals(ToolId::StructureSearch, json!({"matchCount": 1})));
}

#[test]
fn minimal_rows_drop_what_asserts_nothing_and_keep_what_continues() {
    let row = minimized(
        ToolId::LocalSearch,
        json!({"path":"/r","matchString":"x"}),
        json!({"files":[{"path":"a.rs"}],"matchCount":1,"filesScanned":9,
            "pagination":{"currentPage":1,"totalPages":1,"hasMore":false},
            "snapshot":"s",
            "diagnostics":[{"severity":"info","message":"routine"},{"severity":"warning","message":"keep"}]}),
    );
    assert!(row.get("cache").is_none(), "{row}");
    let data = &row["data"];
    // The occurrence total stays on every view; scan accounting goes.
    assert_eq!(data["matchCount"], 1, "{row}");
    for gone in ["filesScanned", "pagination", "snapshot"] {
        assert!(data.get(gone).is_none(), "{gone}: {row}");
    }
    assert_eq!(
        data["diagnostics"],
        json!([{"severity":"warning","message":"keep"}])
    );
    assert_eq!(data["files"][0]["path"], "a.rs");
}

/// The parent list and emails are `extras: ["details"]`; a committer
/// row field the tool emitted (it differs from the author) stays.
/// A failed row states the error and what is evidence; empty arrays, a
/// finished single page and the snapshot assert nothing there.
#[test]
fn terminal_error_rows_drop_empty_single_page_and_snapshot_fields() {
    for tool in [ToolId::AstSearch, ToolId::LocalSearch, ToolId::GhSearchCode] {
        let mut row = json!({"index":0,"status":"error","data":{
            "error":"boom","errorCode":"executionFailed","path":"a.rs",
            "nodes":[],"files":[],"snapshot":"s",
            "pagination":{"currentPage":1,"totalPages":1,"pageSize":100,"totalItems":0,"hasMore":false},
            "diagnostics":[{"severity":"error","message":"kept evidence"}],
            "warnings":["kept"]}});
        minimize_row(&mut row, tool, &json!({"path":"a.rs"}));
        let data = row["data"].as_object().expect("data");
        let mut keys: Vec<&str> = data.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["diagnostics", "error", "errorCode", "path", "warnings"],
            "{tool:?}: {row}"
        );
    }
}

/// An error row that still has a page to walk keeps its pagination.
#[test]
fn terminal_error_rows_keep_an_open_pagination() {
    let mut row = json!({"index":0,"status":"error","data":{
        "error":"boom","errorCode":"executionFailed",
        "pagination":{"currentPage":1,"totalPages":3,"hasMore":true}}});
    minimize_row(&mut row, ToolId::LocalSearch, &json!({"path":"a"}));
    assert_eq!(row["data"]["pagination"]["hasMore"], true, "{row}");
}

/// An error row is not a partial result: the page it offers (a bounded
/// read of a file too large to read whole) is the way past the error.
#[test]
fn an_error_row_with_a_page_is_not_partial() {
    let query = json!({"path":"big.log","matchString":"needle"});
    let data = json!({"error":"too large","errorCode":"largeSourceWindowOnly","path":"big.log",
        "next":{"readBoundedLines":{"tool":"localFetch","query":{"queries":[
            {"path":"big.log","matchString":"needle","offset":0}]}}}});
    let row = result_row(ToolId::LocalFetch, 0, &query, data, Some("error"));
    assert_eq!(row["status"], "error", "{row}");
    assert!(
        row["data"]
            .get("isPartial")
            .is_none_or(|flag| flag == false),
        "{row}"
    );
}

/// The parent list and emails are `extras: ["details"]`; a committer
/// row field the tool emitted (it differs from the author) stays.
#[test]
fn commit_rows_keep_a_differing_committer_and_detail_the_parents() {
    let author = json!({"name":"Ada","email":"ada@x.dev","date":"2024-01-01T00:00:00Z"});
    let commit = |committer: Value, extras: Value| {
        minimized(
            ToolId::GhGetHistoryItem,
            json!({"operation":"commit","owner":"o","repo":"r","ref":"abc","extras":extras}),
            json!({"type":"commit","sha":"abc","message":"m","author":author,
                "committer":committer,"parents":["p1","p2"]}),
        )
    };
    let mut other_email = author.clone();
    other_email["email"] = json!("ada@y.dev");
    let same = commit(other_email, json!([]));
    assert!(same["data"]["committer"].get("email").is_none(), "{same}");
    assert!(same["data"].get("parents").is_none(), "{same}");
    assert!(same["data"]["author"].get("email").is_none(), "{same}");
    let detailed = commit(author.clone(), json!(["details"]));
    assert_eq!(
        detailed["data"]["parents"],
        json!(["p1", "p2"]),
        "{detailed}"
    );
    assert_eq!(
        detailed["data"]["author"]["email"], "ada@x.dev",
        "{detailed}"
    );
    let mut rebased = author.clone();
    rebased["date"] = json!("2024-02-01T00:00:00Z");
    let kept = commit(rebased, json!([]));
    assert_eq!(
        kept["data"]["committer"]["date"], "2024-02-01T00:00:00Z",
        "{kept}"
    );
}
