use super::*;

/// `isPartial` is the umbrella flag for a row that does not cover its
/// scope. A row whose own `pagination.hasMore` and `next` page already say
/// so, with no other reason, states it once; any other cause keeps it.
#[test]
fn is_partial_is_dropped_only_where_pagination_and_next_already_say_it() {
    let page =
        json!({"tool":"localSearch","query":{"queries":[{"path":".","matchString":"x","page":2}]}});
    let paged = |extra: Value| {
        let mut data = json!({"files":[{"path":"a"}],"isPartial":true,
            "pagination":{"currentPage":1,"totalPages":2,"hasMore":true},
            "next":{"nextPage":page}});
        for (key, value) in extra.as_object().expect("object") {
            data[key] = value.clone();
        }
        result_row(ToolId::LocalSearch, 0, &json!({}), data, None)
    };
    let plain = paged(json!({}));
    assert!(plain["data"].get("isPartial").is_none(), "{plain}");
    assert_eq!(plain["data"]["pagination"]["hasMore"], true, "{plain}");
    assert!(plain["data"]["next"]["nextPage"].is_object(), "{plain}");
    let injected = result_row(
        ToolId::LocalSearch,
        0,
        &json!({}),
        json!({"files":[{"path":"a"}],"pagination":{"currentPage":1,"hasMore":true},
            "next":{"nextPage":page}}),
        None,
    );
    assert!(injected["data"].get("isPartial").is_none(), "{injected}");
    for (name, extra) in [
        (
            "partialReasons",
            json!({"partialReasons":["providerResultCap"]}),
        ),
        ("terminalLimit", json!({"terminalLimit":true})),
    ] {
        let row = paged(extra);
        assert_eq!(row["data"]["isPartial"], true, "{name}: {row}");
    }
    // A file's own match pagination with its `next` page says more too
    // (SH-4: nested blocks count as the row's own).
    let nested = result_row(
        ToolId::LocalSearch,
        0,
        &json!({}),
        json!({"files":[{"path":"a","pagination":{"hasMore":true}}],
            "next":{"nextMatchPage":page}}),
        None,
    );
    assert!(nested["data"].get("isPartial").is_none(), "{nested}");
    // A partial row with no page to read keeps the flag.
    let stopped = result_row(
        ToolId::LocalSearch,
        0,
        &json!({}),
        json!({"files":[{"path":"a"}],"isPartial":true}),
        None,
    );
    assert_eq!(stopped["data"]["isPartial"], true, "{stopped}");
}

#[test]
fn result_rows_normalize_null_is_partial_to_false() {
    let row = result_row(
        ToolId::from_name("ghGetFileContent").expect("known tool"),
        0,
        &json!({}),
        json!({"path":"a","content":"x","isPartial":null}),
        None,
    );
    assert_eq!(row.pointer("/data/isPartial"), Some(&json!(false)));
}

/// `empty` is decided per tool from the rows a page holds; a row that
/// can still be read on (an executable page, or a pagination block with
/// `hasMore`) is never `empty`, for every tool. A row with nothing more
/// keeps the tool's `empty`.
#[test]
fn a_row_with_more_to_read_is_never_empty() {
    let page = |tool: ToolId| json!({"tool": tool.as_str(), "query": {"queries": [{"page": 2}]}});
    for tool in ToolId::ALL
        .into_iter()
        .filter(|tool| !tool.output().resource_major())
    {
        for data in [
            json!({"files":[],"next":{"nextPage":page(tool)}}),
            json!({"files":[],"pagination":{"currentPage":1,"hasMore":true}}),
        ] {
            let row = result_row(tool, 0, &json!({}), data.clone(), Some("empty"));
            assert!(row.get("status").is_none(), "{tool:?} {data}: {row}");
        }
        let finished = result_row(
            tool,
            0,
            &json!({}),
            json!({"files":[],"pagination":{"currentPage":1,"hasMore":false}}),
            Some("empty"),
        );
        assert_eq!(finished["status"], "empty", "{tool:?}: {finished}");
    }
}

#[test]
fn result_rows_preserve_compare_status_as_contract_data() {
    let row = result_row(
        ToolId::from_name("ghGetHistoryItem").expect("known tool"),
        0,
        &json!({"operation":"compare"}),
        json!({"aheadBy":2,"commits":[]}),
        None,
    );
    assert_eq!(row.pointer("/data/aheadBy"), Some(&json!(2)));
}

#[test]
fn result_metadata_requires_diagnostics_extras() {
    let mut normal = result_row(
        ToolId::from_name("localSearch").expect("known tool"),
        0,
        &json!({}),
        json!({"files":[]}),
        None,
    );
    super::super::super::extras::prune_row(&mut normal, ToolId::LocalSearch, &json!({}));
    assert!(normal.get("meta").is_none());

    let query = json!({"extras":["diagnostics"]});
    let mut diagnostics = result_row(
        ToolId::from_name("localSearch").expect("known tool"),
        0,
        &query,
        json!({"files":[]}),
        None,
    );
    super::super::super::extras::prune_row(&mut diagnostics, ToolId::LocalSearch, &query);
    assert_eq!(diagnostics["meta"]["evidence"]["kind"], "lexical");
}

/// A page past the end flagged on a resource item's section (a pull
/// request's files) is the row's outcome: the page cursor is the row's.
#[test]
fn an_item_section_past_its_end_settles_the_row() {
    let query = json!({"operation":"pullRequest","owner":"o","repo":"r","number":4,
        "sections":["files"],"page":5});
    let data = json!({"pullRequests":[{"number":4,"files":[],
        "contentPagination":{"files":{"currentPage":5,"totalPages":1,"hasMore":false,"outOfRange":true}}}]});
    let row = result_row(ToolId::GhGetHistoryItem, 0, &query, data, None);
    assert_eq!(row["status"], "outOfRange", "{row}");
    assert_eq!(row["data"]["reason"], "page 5 of 1", "{row}");
    assert_eq!(
        row["data"]["next"]["restart"]["tool"], "ghGetHistoryItem",
        "{row}"
    );
    assert!(
        row["data"]["next"]["restart"]["query"]["queries"][0]
            .get("page")
            .is_none(),
        "{row}"
    );
}

/// A resource item's section blocks (a pull request's `contentPagination`)
/// carry no cursor, as the row's own do: `next` holds every page. The row's
/// `isPartial` does not restate the `hasMore` that its `next` page answers.
#[test]
fn nested_page_blocks_drop_cursors_and_the_restated_partial_flag() {
    let query = json!({"operation":"pullRequest","owner":"o","repo":"r","number":4,
        "sections":["files"],"extras":["stats"]});
    let next = crate::response::result::Continuation::new(
        ToolId::GhGetHistoryItem,
        json!({"operation":"pullRequest","owner":"o","repo":"r","number":4,
            "sections":["files"],"page":2}),
    )
    .build();
    let data = json!({"pullRequests":[{"number":4,"files":["M +1 -1 a.rs"],
        "contentPagination":{"files":{"currentPage":1,"totalPages":2,"pageSize":1,
            "totalItems":2,"hasMore":true,"nextPage":2,"snapshot":"s"}}}],
        "isPartial":true,"next":{"nextFilePage":next}});
    let mut row = result_row(ToolId::GhGetHistoryItem, 0, &query, data, None);
    minimize_row(&mut row, ToolId::GhGetHistoryItem, &query);
    let block = &row["data"]["pullRequests"][0]["contentPagination"]["files"];
    assert_eq!(block["hasMore"], true, "{row}");
    assert!(
        block.get("nextPage").is_none() && block.get("snapshot").is_none(),
        "{row}"
    );
    assert!(row["data"].get("isPartial").is_none(), "{row}");
    assert!(row.get("status").is_none(), "{row}");
    assert_eq!(row["data"]["next"]["nextFilePage"], next, "{row}");
}

/// The one row-finishing sequence: a finished row carries its diagnostics,
/// cache flag and hint policy, and equals the steps it folds, in order.
#[test]
fn finish_row_runs_the_one_publishing_sequence() {
    let query = json!({"path": "a.rs"});
    let data = json!({"path": "a.rs", "content": "1\tx"});
    let plain = finish_row(
        ToolId::LocalFetch,
        3,
        &query,
        data.clone(),
        None,
        RowParts::default(),
    );
    let mut by_hand = result_row(ToolId::LocalFetch, 3, &query, data.clone(), None);
    apply_hint_policy(&mut by_hand, ToolId::LocalFetch, &query);
    crate::response::pages::note_remaining(&mut by_hand, ToolId::LocalFetch, Vec::new());
    crate::response::extras::prune_row(&mut by_hand, ToolId::LocalFetch, &query);
    minimize_row(&mut by_hand, ToolId::LocalFetch, &query);
    assert_eq!(plain, by_hand);
    // The cache flag is a `receipts` extra: a default row never shows it.
    let receipts = json!({"path": "a.rs", "extras": ["receipts"]});
    let cached = finish_row(
        ToolId::LocalFetch,
        3,
        &receipts,
        data,
        None,
        RowParts {
            cache: true,
            ..RowParts::default()
        },
    );
    assert_eq!(cached["cache"], 1, "{cached}");
}

/// A block flagged `outOfRange` routes (the page asked for is not there),
/// even where nothing else about it does; a single complete page does not.
#[test]
fn an_out_of_range_page_block_is_kept_and_a_complete_single_page_is_not() {
    let query = json!({"path": "a.rs"});
    let kept = |block: Value| {
        let row = finish_row(
            ToolId::LocalFetch,
            0,
            &query,
            json!({"path": "a.rs", "content": "", "pagination": block}),
            None,
            RowParts::default(),
        );
        row["data"].get("pagination").is_some()
    };
    assert!(kept(
        json!({"unit":"lines","offset":0,"length":0,"hasMore":false,"outOfRange":true})
    ));
    assert!(!kept(
        json!({"unit":"lines","offset":0,"length":5,"hasMore":false})
    ));
    assert!(kept(
        json!({"unit":"lines","offset":0,"length":5,"hasMore":true,"nextOffset":5})
    ));
}
