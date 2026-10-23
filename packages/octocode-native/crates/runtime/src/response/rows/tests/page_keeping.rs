use super::*;

#[test]
fn open_pages_keep_pagination_and_totals() {
    let row = minimized(
        ToolId::LocalSearch,
        json!({}),
        json!({"files":[{"path":"a.rs"}],"matchCount":40,"matchedLineCount":31,"filesScanned":9,"bytesSearched":7,
            "pagination":{"currentPage":1,"hasMore":true},"next":{"nextPage":{"tool":"localSearch","query":{"queries":[{"snapshot":"s"}]}}}}),
    );
    let data = &row["data"];
    // An open page shows its
    // position by default, beside the page in `next`.
    assert_eq!(
        data["pagination"],
        json!({"currentPage":1,"hasMore":true}),
        "{data}"
    );
    // Occurrences and matched lines (the rows a walk shows) are both
    // named, so the two totals never read as one disagreeing count.
    assert_eq!(data["matchCount"], 40, "{data}");
    assert_eq!(data["matchedLineCount"], 31, "{data}");
    for gone in ["filesScanned", "bytesSearched"] {
        assert!(data.get(gone).is_none(), "{gone}: {data}");
    }
    assert_eq!(
        data["next"]["nextPage"]["query"]["queries"][0]["snapshot"],
        "s"
    );
    // A later page stays located without any extra.
    let later = minimized(
        ToolId::LocalSearch,
        json!({}),
        json!({"files":[],"pagination":{"currentPage":3,"hasMore":false}}),
    );
    assert_eq!(
        later["data"]["pagination"]["currentPage"], 3,
        "last page stays located"
    );
}

#[test]
fn empty_searches_keep_their_scan_scope() {
    let row = minimized(
        ToolId::LocalSearch,
        json!({}),
        json!({"matchCount":0,"filesScanned":176,"bytesSearched":9}),
    );
    assert_eq!(row["data"]["matchCount"], 0, "{row}");
    assert_eq!(row["data"]["filesScanned"], 176, "{row}");
    assert!(row["data"].get("bytesSearched").is_none(), "{row}");
}

/// countLines states no `matchCount` and filesWithout neither total; their
/// pages minimize like every other view: rows drop the scan scope, an empty
/// page keeps it.
#[test]
fn views_without_an_occurrence_total_minimize_like_every_view() {
    let lines = minimized(
        ToolId::LocalSearch,
        json!({"resultView":"countLines"}),
        json!({"files":[{"path":"a.rs","matchCount":2}],"matchedLineCount":2,"filesScanned":9}),
    );
    assert_eq!(lines["data"]["matchedLineCount"], 2, "{lines}");
    assert!(lines["data"].get("matchCount").is_none(), "{lines}");
    assert!(lines["data"].get("filesScanned").is_none(), "{lines}");
    let without = minimized(
        ToolId::LocalSearch,
        json!({"resultView":"filesWithout"}),
        json!({"files":[{"path":"b.rs"}],"filesScanned":9}),
    );
    assert_eq!(without["data"]["files"][0]["path"], "b.rs", "{without}");
    assert!(without["data"].get("filesScanned").is_none(), "{without}");
    let empty = minimized(
        ToolId::LocalSearch,
        json!({"resultView":"filesWithout"}),
        json!({"files":[],"filesScanned":9}),
    );
    assert_eq!(empty["data"]["filesScanned"], 9, "{empty}");
}

#[test]
fn capped_searches_keep_limit_diagnostics_on_first_final_and_empty_pages() {
    for (files, more) in [
        (json!([{ "path": "a.rs" }]), true),
        (json!([{ "path": "a.rs" }]), false),
        (json!([]), false),
    ] {
        let row = minimized(
            ToolId::LocalSearch,
            json!({}),
            json!({
                "files": files,
                "partialReasons": ["maxCollectedFiles"], "matchCount": 10002,
                "filesScanned": 10002, "bytesSearched": 70014,
                "pagination": {"currentPage": if more { 1 } else { 20 }, "hasMore": more},
                "isPartial": true,
                "terminalLimit": !more,
            }),
        );
        assert_eq!(
            row["data"]["partialReasons"],
            json!(["maxCollectedFiles"]),
            "{row}"
        );
        assert_eq!(row["data"]["isPartial"], true, "{row}");
        assert_eq!(row["data"]["matchCount"], 10002, "{row}");
        assert_eq!(row["data"]["terminalLimit"] == true, !more, "{row}");
        assert!(row["data"].get("bytesSearched").is_none(), "{row}");
        crate::contracts::validate_output("localSearch", &json!({"results": [row]}))
            .expect("minimized cap diagnostics obey the canonical output schema");
    }
}

/// The unreadable-path warning names `fileErrors.first`: the failure
/// count and first failure stay public on every page shape.
#[test]
fn unreadable_paths_keep_their_error_stats_on_every_page() {
    for (files, more) in [
        (json!([{ "path": "a.rs" }]), true),
        (json!([{ "path": "a.rs" }]), false),
        (json!([]), false),
    ] {
        let row = minimized(
            ToolId::LocalSearch,
            json!({}),
            json!({
                "files": files,
                "matchCount": 1, "filesScanned": 2, "bytesSearched": 9,
                "fileErrors": {"count": 1,
                    "first": "/r/b.txt: Permission denied (os error 13)"},
                "pagination": {"currentPage": 1, "hasMore": more},
                "warnings": ["1 path(s) could not be read (see fileErrors.first), so absence is not proven."],
                "isPartial": true,
                "terminalLimit": !more,
            }),
        );
        let errors = &row["data"]["fileErrors"];
        assert_eq!(errors["count"], 1, "{row}");
        assert_eq!(
            errors["first"], "/r/b.txt: Permission denied (os error 13)",
            "{row}"
        );
        assert!(row["data"].get("bytesSearched").is_none(), "{row}");
        crate::contracts::validate_output("localSearch", &json!({"results": [row]}))
            .expect("minimized error stats obey the canonical output schema");
    }
}

#[test]
fn a_complete_page_that_keeps_binary_skipped_keeps_the_required_match_count() {
    let row = minimized(
        ToolId::LocalSearch,
        json!({}),
        json!({
            "files": [{ "path": "a.rs", "matches": [{"line": 1, "value": "fn parse()"}] }],
            "matchCount": 1, "matchedLineCount": 1,
            "filesScanned": 3, "bytesSearched": 9, "binarySkipped": 2,
            "pagination": {"currentPage": 1, "hasMore": false},
        }),
    );
    let data = &row["data"];
    assert_eq!(data["binarySkipped"], 2, "{row}");
    assert_eq!(data["matchCount"], 1, "{row}");
    assert!(data.get("bytesSearched").is_none(), "{row}");
    crate::contracts::validate_output("localSearch", &json!({"results": [row]}))
        .expect("surviving totals obey the canonical output schema");
}

#[test]
fn read_rows_leave_byte_accounting_to_stats_extras() {
    let fetch = minimized(
        ToolId::LocalFetch,
        json!({}),
        json!({"content":"x","modified":"t","sourceBytes":1,"totalLines":9}),
    );
    assert_eq!(fetch["data"], json!({"content":"x","totalLines":9}));
    // A GitHub read row is the same read: its byte accounting is a stats extra too.
    let file = json!({"path":"a","content":"x","sourceBytes":1,"returnedBytes":1,"selectedMatchCount":1,"totalLines":9});
    let gh = minimized(ToolId::GhGetFileContent, json!({}), file.clone());
    assert_eq!(gh["data"], json!({"path":"a","content":"x","totalLines":9}));
    let stats = minimized(
        ToolId::GhGetFileContent,
        json!({"extras":["stats"]}),
        file.clone(),
    );
    assert_eq!(stats["data"], file);
}

/// `extras: ["stats"]` returns the whole statistics block, also on a
/// complete page where the default keeps only the match totals.
#[test]
fn the_stats_extra_keeps_full_stats() {
    let totals = json!({"matchCount":1,"matchedLineCount":1,
        "filesScanned":24,"bytesSearched":442552});
    let mut data = json!({"path":"/r","files":[{"path":"a","matches":[{"line":1}]}]});
    data.as_object_mut()
        .expect("object")
        .extend(totals.as_object().expect("object").clone());
    let row = minimized(
        ToolId::LocalSearch,
        json!({"path":"/r","matchString":"x","extras":["stats"]}),
        data.clone(),
    );
    for (key, value) in totals.as_object().expect("object") {
        assert_eq!(&row["data"][key], value, "{key}: {row}");
    }
    let default = minimized(
        ToolId::LocalSearch,
        json!({"path":"/r","matchString":"x"}),
        data,
    );
    // The occurrence and matched-line totals show on every view with
    // rows; only the scan accounting is a stats extra.
    assert_eq!(default["data"]["matchCount"], 1, "{default}");
    assert_eq!(default["data"]["matchedLineCount"], 1, "{default}");
    for gone in ["filesScanned", "bytesSearched"] {
        assert!(default["data"].get(gone).is_none(), "{gone}: {default}");
    }
}

#[test]
fn echoes_go_only_when_they_equal_the_request_and_echo_extras_keep_them() {
    let data = json!({"owner":"o","repo":"other","path":"a.rs"});
    let row = minimized(
        ToolId::GhGetFileContent,
        json!({"owner":"o","repo":"r"}),
        data.clone(),
    );
    assert!(row["data"].get("owner").is_none());
    assert_eq!(
        row["data"]["repo"], "other",
        "a different repo is information"
    );
    let echo = minimized(
        ToolId::GhGetFileContent,
        json!({"owner":"o","extras":["echo"]}),
        data.clone(),
    );
    assert_eq!(echo["data"], data);
    // Echo keeps request echoes only: snapshots and finished stats that
    // restate nothing of the request still go.
    let echo = minimized(
        ToolId::LocalSearch,
        json!({"path":"/r","matchString":"x","extras":["echo"]}),
        json!({"path":"/r","snapshot":"s","files":[{"path":"a","matches":[{"line":1}]}],
            "matchCount":1,"bytesSearched":9}),
    );
    assert!(echo["data"].get("snapshot").is_none(), "{echo}");
    assert!(echo["data"].get("bytesSearched").is_none(), "{echo}");
    // A finished single page drops its pagination by default and
    // states it (`hasMore:false`) when the caller asks for echoes.
    let finished = json!({"commits":[{"commitSha":"a"}],
        "pagination":{"currentPage":1,"pageSize":100,"hasMore":false}});
    let query = json!({"operation":"commit","owner":"o","repo":"r","pageSize":100});
    let default = minimized(ToolId::GhSearchHistory, query.clone(), finished.clone());
    assert!(default["data"].get("pagination").is_none(), "{default}");
    let mut asked = query;
    asked["extras"] = json!(["echo"]);
    let echo = minimized(ToolId::GhSearchHistory, asked, finished.clone());
    assert_eq!(echo["data"]["pagination"], finished["pagination"], "{echo}");
    let mut error = json!({"index":0,"status":"error","data":{"error":"x","snapshot":"s"}});
    minimize_row(&mut error, ToolId::LocalSearch, &json!({}));
    assert!(
        error["data"].get("snapshot").is_none(),
        "an error row has no pages to pin: {error}"
    );
    assert_eq!(error["data"]["error"], "x", "{error}");
}
