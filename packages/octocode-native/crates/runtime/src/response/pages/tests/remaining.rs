use super::*;

/// A tool's typed note stands for its page, in `next` order beside the
/// pages the public facts count; a typed note of a page the row does not
/// offer, or of a complete row, is dropped.
#[test]
fn typed_notes_stand_for_their_pages_in_next_order() {
    let note = |page: &str, left: &str| Remaining {
        page: page.into(),
        left: left.into(),
    };
    let data = json!({
        "pullRequests": [{"number": 1}],
        "pagination": {"currentPage": 1, "pageSize": 10, "totalItems": 25, "hasMore": true},
        "next": {
            "continuePatch": page("ghGetHistoryItem"),
            "nextPage": page("ghSearchHistory"),
            "nextFilePage": page("ghGetHistoryItem")
        }
    });
    let typed = vec![
        note("nextFilePage", "7 more changed files"),
        note("continuePatch", "12 unfinished patches"),
        note("nextCommentPage", "3 more comments"),
    ];
    let mut row = json!({"index": 0, "data": data});
    note_remaining(&mut row, ToolId::GhGetHistoryItem, typed.clone());
    let mut structured = json!({"results": [row]});
    assert_eq!(
        take_remaining_notes(&mut structured),
        vec![vec![
            note("continuePatch", "12 unfinished patches"),
            note("nextPage", "15 more pull requests"),
            note("nextFilePage", "7 more changed files"),
        ]]
    );
    let mut complete = json!({"index": 0, "data": {"coverage": {"state": "complete"},
            "next": {"nextFilePage": page("ghGetHistoryItem")}}});
    note_remaining(&mut complete, ToolId::GhGetHistoryItem, typed);
    assert!(complete.get(super::REMAINING).is_none(), "{complete}");
}

#[test]
fn listings_reads_and_bodies_count_what_is_left() {
    let listing = disclosed(json!({
        "files": [{"path": "a"}],
        "pagination": {"currentPage": 1, "totalPages": 2, "totalItems": 2, "hasMore": true},
        "warnings": ["Regex trap."],
        "next": {"nextPage": page("localSearch")}
    }));
    assert_eq!(
        listing["warnings"],
        json!(["Regex trap.", "1 more page (next.nextPage)"]),
        "{listing}"
    );
    let files = disclosed(json!({
        "files": [{"path": "a"}],
        "pagination": {"currentPage": 1, "totalPages": 7, "pageSize": 3,
            "totalItems": 20, "hasMore": true},
        "next": {"nextPage": page("localSearch")}
    }));
    assert_eq!(files["warnings"], json!(["17 more files (next.nextPage)"]));
    let deduplicated = disclosed(json!({
        "commits": [{"sha": "a"}],
        "pagination": {"totalItems": 99, "totalPages": 20, "hasMore": true},
        "next": {"nextPage": {"tool": "ghSearchHistory",
            "query": {"queries":[{"keywords": ["miri"], "page": 2, "pageSize": 5}]}}}
    }));
    assert_eq!(
        deduplicated["warnings"],
        json!(["94 more commits (next.nextPage)"])
    );
    assert!(deduplicated["pagination"].get("currentPage").is_none());
    let search = disclosed(json!({
        "pullRequests": [{"number": 1}],
        "pagination": {"currentPage": 1, "pageSize": 10, "totalItems": 25, "hasMore": true},
        "next": {"nextPage": page("ghSearchHistory")}
    }));
    assert_eq!(
        search["warnings"],
        json!(["15 more pull requests (next.nextPage)"])
    );
    let read = disclosed(json!({
        "content": "1\tx",
        "pagination": {"unit": "lines", "offset": 0, "length": 381,
            "total": 386, "hasMore": true, "nextOffset": 381},
        "next": {"continueWindow": page("localFetch")}
    }));
    assert_eq!(
        read["warnings"],
        json!(["5 more lines (next.continueWindow)"])
    );
    let whole = disclosed(json!({
        "path": "a.py",
        "totalLines": 3137,
        "next": {"continueWindow": page("localFetch")},
        "pagination": {"unit": "lines", "offset": 0, "length": 471,
            "hasMore": true, "nextOffset": 471},
        "isPartial": true
    }));
    assert_eq!(
        whole["warnings"],
        json!(["2666 more lines (next.continueWindow)"])
    );
    // A symbols view pages outline lines, not source lines.
    let outline = disclosed(json!({
        "path": "a.rs",
        "contentView": "symbols",
        "content": "1\tfn a()",
        "pagination": {"unit": "lines", "offset": 0, "length": 40,
            "total": 52, "hasMore": true, "nextOffset": 40},
        "next": {"continueWindow": page("localFetch")}
    }));
    assert_eq!(
        outline["warnings"],
        json!(["12 more outline lines (next.continueWindow)"])
    );
    let bytes = disclosed(json!({
        "content": "x",
        "pagination": {"unit": "bytes", "offset": 0, "length": 100,
            "total": 250, "hasMore": true, "nextOffset": 100},
        "next": {"continueWindow": page("localFetch")}
    }));
    assert_eq!(
        bytes["warnings"],
        json!(["150 more bytes (next.continueWindow)"])
    );
}

#[test]
fn restarts_and_finished_pages_add_nothing() {
    let restart = json!({
        "warnings": ["Snapshot changed."],
        "pagination": {"currentPage": 1, "totalPages": 1, "hasMore": false},
        "next": {"restart": page("localSearch")}
    });
    assert_eq!(disclosed(restart.clone()), restart);
    let finished = json!({
        "pagination": {"currentPage": 2, "totalPages": 2, "hasMore": false},
        "next": {"nextPage": page("localSearch")}
    });
    assert_eq!(disclosed(finished.clone()), finished);
    let bare = json!({"files": []});
    assert_eq!(disclosed(bare.clone()), bare);
}

/// A page whose count is unknown gets no line: `next.<name>` already
/// says more is left (the instructions say next.* is the unread rest).
/// A restart page starts over and gets none either.
#[test]
fn a_page_without_a_count_adds_no_line() {
    let commits = json!({
        "commits": [{"sha": "a"}],
        "pagination": {"hasMore": true},
        "next": {"nextPage": page("ghSearchHistory"),
            "continueMaterialize": page("ghStructure")}
    });
    assert_eq!(disclosed(commits.clone()), commits);
    let bytes = json!({
        "content": "x",
        "pagination": {"unit": "bytes", "offset": 0, "length": 100,
            "hasMore": true, "nextOffset": 100},
        "next": {"continueWindow": page("localFetch")}
    });
    assert_eq!(disclosed(bytes.clone()), bytes);
    let restart = json!({"next": {"restart": page("localSearch")}});
    assert_eq!(disclosed(restart.clone()), restart);
}

/// A whole row shows no remaining-page warning; a split part
/// words only the pages its split moved to the response pager.
#[test]
fn only_moved_pages_are_worded_on_a_split_part() {
    let notes = vec![
        Remaining {
            page: "nextPage".into(),
            left: "17 more files".into(),
        },
        Remaining {
            page: "nextMatchPage".into(),
            left: "26 more matches".into(),
        },
    ];
    let mut part = json!({"files": [], "next": {"nextPage": page("localSearch")}});
    redisclose(part.as_object_mut().unwrap(), &notes, &[]);
    assert!(part.get("warnings").is_none(), "{part}");
    redisclose(
        part.as_object_mut().unwrap(),
        &notes,
        &["nextMatchPage".to_owned()],
    );
    assert_eq!(
        part["warnings"],
        json!(["26 more matches (next.nextMatchPage on the row's last part)"]),
        "{part}"
    );
}

/// A page size the continuation leaves at its contract default still
/// counts: the default is the page size.
#[test]
fn a_default_page_size_comes_from_the_contract() {
    let repos = disclosed(json!({
        "repositories": [{"repo": "a"}],
        "pagination": {"totalItems": 68, "hasMore": true},
        "next": {"nextPage": {"tool": "ghSearchRepo",
            "query": {"queries": [{"keywords": ["mcp"], "page": 2}]}}}
    }));
    assert_eq!(
        repos["warnings"],
        json!(["18 more repositories (next.nextPage)"])
    );
    // A directory outline groups its symbols by file: the continued
    // operation names what a page holds.
    let symbols = disclosed(json!({
        "files": [{"path": "a.ts", "symbols": ["fn a"]}],
        "pagination": {"currentPage": 1, "totalPages": 15, "pageSize": 500, "totalItems": 7194, "hasMore": true},
        "next": {"nextPage": {"tool": "astSearch",
            "query": {"queries": [{"operation": "symbols", "path": "src", "page": 2}]}}}
    }));
    assert_eq!(
        symbols["warnings"],
        json!(["6694 more symbols (next.nextPage)"])
    );
}

/// `nextMatchPage` continues the matches of the page's files, counted by
/// each file's own pagination, never the row's file pagination.
#[test]
fn match_pages_count_the_matches_left_in_the_page_files() {
    let data = disclosed(json!({
        "files": [
            {"path": "a", "matches": ["x"], "pagination": {"currentPage": 1,
                "pageSize": 20, "totalItems": 45, "hasMore": true}},
            {"path": "b", "matches": ["y"]},
            {"path": "c", "matches": ["z"], "pagination": {"currentPage": 1,
                "pageSize": 20, "totalItems": 21, "hasMore": true}}
        ],
        "pagination": {"currentPage": 1, "pageSize": 5, "totalItems": 79, "hasMore": true},
        "next": {"nextPage": {"tool": "astSearch",
                "query": {"queries": [{"operation": "match", "page": 2, "pageSize": 5}]}},
            "nextMatchPage": {"tool": "astSearch",
                "query": {"queries": [{"operation": "match", "matchPage": 2, "pageSize": 5}]}}}
    }));
    assert_eq!(
        data["warnings"],
        json!(["74 more files (next.nextPage); 26 more matches (next.nextMatchPage)"])
    );
}

/// The first part of a split row words the pages that moved to the
/// response pager (once per line) after the tool's own warnings; a row
/// without moved pages gains nothing.
#[test]
fn split_parts_word_only_the_pages_that_moved() {
    let note = |page: &str, left: &str| Remaining {
        page: page.into(),
        left: left.into(),
    };
    let notes = [
        note("nextPage", "4 more pages"),
        note("nextFilePage", "7 more changed files"),
        note("continuePatch", "3 unfinished patches"),
    ];
    let mut first = json!({"warnings": ["Regex trap."]});
    let moved = ["nextPage", "nextFilePage"].map(String::from);
    super::super::redisclose(first.as_object_mut().expect("object"), &notes, &moved);
    assert_eq!(
        first["warnings"],
        json!([
            "Regex trap.",
            "4 more pages (next.nextPage on the row's last part); 7 more changed files (next.nextFilePage on the row's last part)"
        ])
    );
    let mut kept = json!({"warnings": ["Regex trap."]});
    super::super::redisclose(kept.as_object_mut().expect("object"), &notes, &[]);
    assert_eq!(kept["warnings"], json!(["Regex trap."]));
    let mut bare = json!({"files": [], "next": {"nextPage": page("astSearch")}});
    super::super::redisclose(bare.as_object_mut().expect("object"), &notes, &[]);
    assert!(bare.get("warnings").is_none(), "{bare}");
}

/// A tool's own warning is data the stage never reads: the count line
/// follows it whatever it says.
#[test]
fn a_tool_warning_never_hides_the_count() {
    let data = disclosed(json!({
        "warnings": ["Some match values were truncated."],
        "files": [{"path": "a"}],
        "pagination": {"currentPage": 1, "pageSize": 5, "totalItems": 7, "hasMore": true},
        "next": {"nextPage": page("localSearch"), "expandValues": page("localSearch")}
    }));
    assert_eq!(
        data["warnings"],
        json!([
            "Some match values were truncated.",
            "2 more files (next.nextPage)"
        ]),
        "{data}"
    );
}

/// `partialReasons` name why a row is partial (a provider cap, a size
/// limit, content paging), not how much a page has left: the count still
/// leads the row.
#[test]
fn partial_reasons_do_not_hide_the_remaining_count() {
    let repos = disclosed(json!({
        "repositories": [{"repo": "a"}],
        "pagination": {"totalItems": 1000, "hasMore": true},
        "next": {"nextPage": {"tool": "ghSearchRepo",
            "query": {"queries":[{"keywords": ["mcp"], "page": 2, "pageSize": 3}]}}},
        "providerLimit": {"maxResults": 1000},
        "isPartial": true,
        "partialReasons": ["providerResultCap"]
    }));
    assert_eq!(
        repos["warnings"],
        json!(["997 more repositories (next.nextPage)"])
    );
    let read = disclosed(json!({
        "path": "a.rs",
        "totalLines": 2270,
        "pagination": {"unit": "lines", "offset": 0, "length": 490,
            "hasMore": true, "nextOffset": 490},
        "isPartial": true,
        "partialReasons": ["regexMatchLimit"],
        "next": {"continueWindow": page("ghGetFileContent")}
    }));
    assert_eq!(
        read["warnings"],
        json!(["1780 more lines (next.continueWindow)"])
    );
}
