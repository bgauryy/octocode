use super::*;
use serde_json::json;

/// A collection the rows name as a linear channel walks by its own name.
#[test]
fn a_section_walks_by_its_own_name() {
    assert_eq!(walk_name(ToolId::GhGetHistoryItem, "commits"), "commits");
    assert_eq!(walk_name(ToolId::AstSearch, "files"), NEXT_PAGE);
}

#[test]
fn only_registry_collections_are_collections() {
    for (tool, collection) in [
        (ToolId::GhCloneRepo, "files"),
        (ToolId::LspSearch, NEXT_PAGE),
        (ToolId::LocalFetch, "importers"),
    ] {
        assert!(
            !declares(tool, collection),
            "{} {collection}",
            tool.as_str()
        );
    }
}

/// One next-page builder: the query with the cursor advanced and the
/// pins written, as an exact-replay call, however far past any former
/// schema ceiling the cursor runs.
#[test]
fn continued_page_advances_one_cursor_past_any_size() {
    let query = json!({"operation": "references", "path": "a.rs", "symbolName": "f",
            "line": 1, "page": 2});
    for next in [3, 1_001, 100_001, usize::MAX] {
        let call = continued_page(
            ToolId::LspSearch,
            query.clone(),
            "page",
            next,
            &[("snapshot", json!("lsp-v3:x"))],
        )
        .expect("an object query")
        .build();
        assert_eq!(call["tool"], "lspSearch", "{call}");
        let row = &call["input"]["queries"][0];
        assert_eq!(row["page"], json!(next), "{call}");
        assert_eq!(row["snapshot"], "lsp-v3:x", "{call}");
        assert_eq!(row["path"], "a.rs", "{call}");
    }
    assert!(continued_page(ToolId::LspSearch, json!(1), "page", 2, &[]).is_none());
}

/// A cursor is not always a number: a candidate-page token continues the
/// same query through the same builder.
#[test]
fn continued_page_carries_a_token_cursor() {
    let query = json!({"path": "src", "matchString": ["x"]});
    let call = continued_page(ToolId::LocalSearch, query, "selection", "token-1", &[])
        .expect("an object query")
        .build();
    assert_eq!(
        call["input"]["queries"][0]["selection"], "token-1",
        "{call}"
    );
}

/// The tools whose rows issue a `nextPage` walk through the builder: each
/// declares it a linear channel in core's registry, so the debug check of
/// [`insert_walk`] holds at every emitter.
const WALKERS: [ToolId; 12] = [
    ToolId::ArtifactSearch,
    ToolId::AstSearch,
    ToolId::Clasify,
    ToolId::GhGetHistoryItem,
    ToolId::GhSearchCode,
    ToolId::GhSearchHistory,
    ToolId::GhStructure,
    ToolId::GraphQuery,
    ToolId::LocalSearch,
    ToolId::LspSearch,
    ToolId::SecurityCheck,
    ToolId::StructureSearch,
];

#[test]
fn every_row_walker_declares_its_next_page() {
    for tool in WALKERS {
        assert!(row_walks(tool), "{}", tool.as_str());
        let mut row = json!({});
        let call = Continuation::continued(
            tool,
            crate::tools::test_support::accepted_row(tool, &json!({})),
        );
        let expected = call.clone().build();
        insert_row_walk(&mut row, tool, call);
        assert_eq!(row["calls"][NEXT_PAGE], expected, "{}", tool.as_str());
    }
}

/// A tool whose rows declare no `nextPage` cannot issue one.
#[test]
#[should_panic(expected = "not a declared walk")]
fn a_tool_without_a_walk_cannot_issue_one() {
    let tool = ToolId::GraphBuild;
    let row = crate::tools::test_support::accepted_row(tool, &json!({}));
    let mut calls = json!({});
    insert_walk(&mut calls, tool, None, Continuation::continued(tool, row));
}

/// A section named by a linear channel walks under its own name, and the
/// call a section's page block held is relocated to it unchanged.
#[test]
fn a_relocated_section_walk_keeps_its_call_under_the_section_name() {
    let tool = ToolId::GhGetHistoryItem;
    let row = crate::tools::test_support::accepted_row(tool, &json!({}));
    let call = Continuation::continued(tool, row).build();
    let mut holder = json!({});
    relocate_walk(&mut holder, tool, "comments", call.clone());
    assert_eq!(holder["next"]["comments"], call, "{holder}");
    let mut calls = json!(null);
    let again = Continuation::continued(
        tool,
        crate::tools::test_support::accepted_row(tool, &json!({})),
    );
    insert_walk(&mut calls, tool, Some("commits"), again);
    assert!(calls["commits"].is_object(), "{calls}");
    remove_row_walk(&mut holder);
    let mut with = json!({"calls": {NEXT_PAGE: {}, "readParent": {}}});
    remove_row_walk(&mut with);
    assert_eq!(with, json!({"calls": {"readParent": {}}}));
}

/// A saved response page's cursor walk is the row's own `nextPage`.
#[test]
fn a_cursor_walk_resumes_from_its_handle_alone() {
    let mut row = json!({});
    insert_cursor_walk(&mut row, ToolId::GhSearchCode, "handle");
    assert_eq!(row["next"][NEXT_PAGE]["input"], json!({"cursor": "handle"}));
}

/// The fetch walker's `next.nextPage` is built by the one builder: a typed
/// page call is the exact replay a plain continuation is, and the builder's
/// declaration check fires for a tool whose rows declare no linear walk.
#[test]
fn a_fetch_page_goes_through_the_builder() {
    let query: octocode_contract_types::LocalFetchQuery =
        serde_json::from_value(json!({"path": "README.md"})).expect("query");
    let page = serde_json::to_value(crate::tools::source_view::FetchCall::page(query.clone()))
        .expect("page");
    let plain = serde_json::to_value(crate::tools::source_view::FetchCall::continued(query))
        .expect("plain");
    assert_eq!(page, plain);
    assert_eq!(page["tool"], "localFetch", "{page}");
}

#[test]
#[should_panic(expected = "pages by no linear walk")]
fn a_fetch_page_of_an_undeclared_channel_fails_the_builder_check() {
    assert_row_walk(ToolId::GhCloneRepo);
}
