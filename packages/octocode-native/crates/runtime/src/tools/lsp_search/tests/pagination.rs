//! Pages and snapshots: count cuts, pinned snapshots, restarts and
//! out-of-range pages.

use super::*;
use crate::response::page_windows::Paging;

/// A count-cut page of `items`: the cut an explicit `pageSize` asks for.
fn counted_page(items: &[Value], page: u32, size: u32) -> (Vec<Value>, Value, Option<NextPage>) {
    paginate(
        items,
        page,
        &Pages {
            paging: Paging::of(Some(size as usize), TEST_BUDGET),
            sizes: &plain_sizes(items),
            snapshot: "lsp-v3:test",
        },
    )
}

#[test]
fn pagination_omits_nullable_next_page_at_the_terminal_page() {
    let items = vec![serde_json::json!({"name": "one"})];
    let (_, terminal, next) = counted_page(&items, 1, 20);
    assert!(next.is_none());
    assert_eq!(terminal["hasMore"], false);
    assert!(terminal.get("nextPage").is_none());

    let items = vec![
        serde_json::json!({"name": "one"}),
        serde_json::json!({"name": "two"}),
    ];
    let (_, partial, next) = counted_page(&items, 1, 1);
    // The cursor rides the next page's lead, never the pagination block.
    assert!(partial.get("nextPage").is_none() && partial.get("snapshot").is_none());
    let next = next.expect("a page follows");
    assert_eq!((next.page, next.snapshot.as_str()), (2, "lsp-v3:test"));
}

#[test]
fn semantic_pagination_continuations_cover_the_full_result_fixture() {
    let expected = (0..7)
        .map(|index| {
            serde_json::json!({"symbolName": format!("symbol-{index}"), "path": "/repo/src/lib.rs"})
        })
        .collect::<Vec<_>>();
    let mut query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/src/lib.rs",
        "page": 1,
        "pageSize": 3
    }))
    .expect("canonical lsp query");
    let snapshot = semantic_snapshot(&query, "symbols", &expected);
    query = requery(&query, serde_json::json!({"snapshot": snapshot}));
    let mut actual = Vec::new();

    let walk = crate::tools::test_support::checks::walk(
        "lspSearch",
        serde_json::json!({"queries": [lsp_search_query::to_row(&query)]}),
        |call| {
            let query: LspSearchQuery = serde_json::from_value(call["queries"][0].clone())
                .expect("executable continuation query");
            assert_eq!(lsp_search_query::snapshot(&query), Some(snapshot.as_str()));
            let (page, pagination, next_page) = counted_page(
                &expected,
                lsp_search_query::page(&query).unwrap_or(1),
                lsp_search_query::page_size(&query).expect("explicit pageSize"),
            );
            let mut response = serde_json::json!({"matches": page, "pagination": pagination});
            // The fixture's pages are cut under its own digest.
            let next_page = next_page.map(|next| NextPage {
                snapshot: snapshot.clone(),
                ..next
            });
            offer_next_page(&query, &mut response, next_page);
            if response["pagination"]["hasMore"] != true {
                assert!(response.get("next").is_none());
            } else {
                assert_eq!(response["next"]["nextPage"]["tool"], "lspSearch");
            }
            let rows = page
                .iter()
                .map(|item| item["symbolName"].as_str().expect("name").to_owned())
                .collect();
            actual.extend(page);
            let row = response;
            crate::tools::test_support::checks::Page {
                output: crate::tools::test_support::published(
                    crate::id::ToolId::LspSearch,
                    &lsp_search_query::to_row(&query),
                    &row,
                ),
                rows,
            }
        },
    );

    assert_eq!(walk.queries.len(), 3);
    assert_eq!(actual, expected);
}

#[test]
fn later_pages_check_a_pinned_snapshot_and_carry_it_forward() {
    let query: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "documentSymbols", "mainGoal": "test", "reasoning": "test",
        "path": "/repo/src/lib.rs",
        "page": 2,
        "pageSize": 1
    }))
    .expect("canonical lsp query");
    let items = vec![
        serde_json::json!({"name":"a"}),
        serde_json::json!({"name":"b"}),
    ];
    let snapshot = semantic_snapshot(&query, "symbols", &items);
    // A page without a snapshot pins nothing: it pages the current answer.
    assert!(!snapshot_mismatch(&query, &snapshot));
    let pinned = requery(&query, serde_json::json!({"snapshot": "lsp-v3:stale"}));
    assert!(snapshot_mismatch(&pinned, &snapshot));
    let changed = snapshot_changed(&query);
    assert_eq!(changed["errorCode"], "staleSnapshot");
    let restart = &changed["next"]["restart"]["query"]["queries"][0];
    for cursor in crate::response::result::CURSOR_FIELDS {
        assert!(restart.get(cursor).is_none(), "{cursor}: {changed}");
    }

    let continued_query = requery(&query, serde_json::json!({"snapshot": snapshot}));
    let mut continued = serde_json::json!({"pagination": {"hasMore": true}});
    offer_next_page(
        &continued_query,
        &mut continued,
        Some(NextPage {
            page: 3,
            snapshot: snapshot.clone(),
        }),
    );
    assert_eq!(
        continued["next"]["nextPage"]["query"]["queries"][0]["page"],
        3
    );
    assert!(continued["next"]["nextPage"]["query"]["queries"][0]["snapshot"].is_string());

    // The last page a continuation may name (the contract `page` maximum)
    // keeps its rows and states the terminal limit instead of a lead past it.
    let mut last = serde_json::json!({
        "locations": [{"uri": "file:///repo/a.ts"}],
        "pagination": {"currentPage": 1000, "hasMore": true}
    });
    offer_next_page(
        &requery(
            &query,
            serde_json::json!({"snapshot": snapshot, "page": 1000}),
        ),
        &mut last,
        Some(NextPage {
            page: 1001,
            snapshot: snapshot.clone(),
        }),
    );
    assert!(last.pointer("/next/nextPage").is_none(), "{last}");
    assert_eq!(last["terminalLimit"], true, "{last}");
    assert_eq!(
        last["partialReasons"],
        serde_json::json!([crate::response::paging::CURSOR_MAXIMUM])
    );
    assert!(last["locations"].is_array(), "{last}");
}

/// A later page is addressed by its snapshot or a fixed pageSize, never by
/// a page number alone (whose rows would depend on the batch's window).
#[test]
fn a_later_page_needs_its_snapshot_or_a_page_size() {
    let bare = query(serde_json::json!({
        "operation": "references", "path": "/repo/a.rs", "symbolName": "f", "line": 1, "page": 2
    }));
    assert!(super::super::unpinned_later_page(&bare));
    assert!(!super::super::unpinned_later_page(&requery(
        &bare,
        serde_json::json!({"snapshot": "lsp-v3:x"})
    )));
    assert!(!super::super::unpinned_later_page(&requery(
        &bare,
        serde_json::json!({"pageSize": 10})
    )));
    assert!(!super::super::unpinned_later_page(&requery(
        &bare,
        serde_json::json!({"page": 1})
    )));
}

/// The refused later page leads back to the walk's first page, whose
/// `next.nextPage` carries the snapshot: no server starts, and the lead is
/// the same query without any cursor.
#[tokio::test(flavor = "current_thread")]
async fn an_unpinned_later_page_leads_to_the_restart() {
    let (_dir, root, paths) = temp_workspace();
    std::fs::write(root.join("a.rs"), "fn f() {}\n").expect("file");
    let row = serde_json::json!({
        "operation": "references", "path": root.join("a.rs").to_string_lossy(),
        "symbolName": "f", "line": 1, "page": 2
    });
    let result = super::super::execute(
        query(row.clone()),
        &crate::execution::cancel::NeverCancel,
        &fake_server_pool(),
        &paths,
        &super::super::LspExecutionConfig::default(),
        None,
    )
    .await
    .expect("row");
    assert_eq!(result["errorCode"], "invalidInput", "{result}");
    let restart = &result["next"]["restart"];
    assert_eq!(restart["tool"], "lspSearch", "{result}");
    assert!(
        restart["query"]["queries"][0].get("page").is_none(),
        "{result}"
    );
    let mut expected = row;
    expected.as_object_mut().expect("row").remove("page");
    let restarted = query(restart["query"]["queries"][0].clone());
    assert_eq!(
        lsp_search_query::to_row(&restarted),
        lsp_search_query::to_row(&query(expected)),
        "{result}"
    );
    crate::tools::test_support::assert_row_contract(
        crate::id::ToolId::LspSearch,
        &serde_json::json!({
            "operation": "references", "path": root.join("a.rs").to_string_lossy(),
            "symbolName": "f", "line": 1, "page": 2
        }),
        &result,
    );
}

#[test]
fn semantic_snapshot_ignores_paging_and_workflow_metadata() {
    let first: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "references",
        "mainGoal": "test", "reasoning": "Find every reference.",
        "extras": ["receipts"],
        "path": "/repo/src/lib.rs",
        "symbolName": "run",
        "line": 4,
        "pageSize": 1
    }))
    .expect("first page query");
    let continued: LspSearchQuery = serde_json::from_value(serde_json::json!({
        "operation": "references",
        "mainGoal": "test", "reasoning": "Continuation metadata may be normalized.",
        "path": "/repo/src/lib.rs",
        "symbolName": "run",
        "line": 4,
        "page": 2,
        "pageSize": 1,
        "snapshot": "prior"
    }))
    .expect("continuation query");
    let items = vec![serde_json::json!({"uri":"file:///repo/src/lib.rs"})];
    assert_eq!(
        semantic_snapshot(&first, "references", &items),
        semantic_snapshot(&continued, "references", &items)
    );

    let resized = requery(&continued, serde_json::json!({"pageSize": 100}));
    assert_ne!(
        semantic_snapshot(&continued, "references", &items),
        semantic_snapshot(&resized, "references", &items),
        "changing the page size can skip or repeat locations"
    );
}

#[test]
fn pages_past_the_end_are_empty_and_flagged_out_of_range() {
    let items = vec![
        serde_json::json!({"name": "one"}),
        serde_json::json!({"name": "two"}),
    ];
    let (page, pagination, _) = counted_page(&items, 5, 1);
    assert!(page.is_empty(), "must not clamp to the last page");
    assert_eq!(pagination["currentPage"], 5);
    assert_eq!(pagination["totalPages"], 2);
    assert_eq!(pagination["hasMore"], false);
    assert_eq!(pagination["outOfRange"], true);
    assert!(pagination.get("nextPage").is_none());

    let (page, pagination, _) = counted_page(&items, 2, 1);
    assert_eq!(page.len(), 1);
    assert!(pagination.get("outOfRange").is_none());
}

#[test]
fn paged_rows_state_page_facts_once_in_next() {
    let q = query(serde_json::json!({
        "operation": "references", "mainGoal": "test", "reasoning": "test",
        "path": "file:///repo/a.ts", "symbolName": "foo", "line": 1, "pageSize": 3
    }));
    let items = (0..7)
        .map(|index| serde_json::json!({"name": format!("s{index}")}))
        .collect::<Vec<_>>();
    let row = items_payload(&q, "symbols", serde_json::json!(items), TEST_BUDGET);
    // The continuation carries the page and the snapshot; the row's
    // pagination keeps the page facts on every page, its size included, and
    // no cursor.
    let call = &row["next"]["nextPage"]["query"]["queries"][0];
    assert_eq!(call["page"], 2, "{row}");
    assert!(call["snapshot"].is_string(), "{row}");
    assert_eq!(
        row["pagination"],
        serde_json::json!({"currentPage": 1, "totalPages": 3, "totalItems": 7, "hasMore": true, "pageSize": 3}),
        "{row}"
    );
}
