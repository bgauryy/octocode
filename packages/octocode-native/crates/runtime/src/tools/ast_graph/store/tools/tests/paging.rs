//! One paging rule on every surface: a page ends at `limit` rows or at the
//! response budget, whichever comes first, and the walk is lossless.
use super::*;
use crate::tools::ast_graph::store::{QueryOptions, query};

const WINDOW: usize = 8_000;

fn fixture() -> Fixture {
    let dir = Fixture::new();
    let calls = (0..120)
        .map(|n| format!("missing{n}();"))
        .collect::<String>();
    dir.write("app/a.ts", format!("export function a(){{ {calls} }}\n"));
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    dir
}

/// A graphQuery row under a response window, as the runtime dispatches it.
fn windowed(dir: &Path, row: Value) -> Value {
    let row = crate::contracts::validate_built_query("graphQuery", row).expect("input contract");
    let context = ExecutionContext {
        response_window: Some(WINDOW),
        ..context()
    };
    let result = execute(ToolId::GraphQuery, &row, dir, &context).expect("execute");
    crate::tools::test_support::assert_row_contract(ToolId::GraphQuery, &result.data);
    result.data
}

fn ids(page: &Value) -> Vec<String> {
    page["rows"]
        .as_array()
        .expect("results")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect()
}

fn chars(value: &Value) -> usize {
    octocode_engine::json_size::utf16_units(value).expect("measurable")
}

#[test]
fn a_page_ends_at_limit_or_the_response_budget() {
    let dir = fixture();
    let small = windowed(
        &dir.0,
        json!({"operation":"unresolved","path":"app","limit":5}),
    );
    assert_eq!(ids(&small).len(), 5, "the budget allows the whole limit");
    let mut query = json!({"operation":"unresolved","path":"app","limit":1000});
    let mut walked = Vec::new();
    let mut pages = 0;
    let total = loop {
        let page = windowed(&dir.0, query);
        pages += 1;
        let rows = ids(&page);
        assert!(!rows.is_empty(), "a walk always moves: {page}");
        assert!(
            rows.len() == 1 || chars(&page) <= WINDOW,
            "page {pages} exceeds the response window: {}",
            chars(&page)
        );
        walked.extend(rows);
        match page.pointer("/next/nextPage/input/queries/0") {
            Some(next) => query = next.clone(),
            None => break page["pagination"]["totalItems"].as_u64().expect("total"),
        }
    };
    assert!(pages > 1, "a 1000-row limit is cut by the budget");
    assert_eq!(walked.len() as u64, total);
    assert_eq!(walked.iter().collect::<BTreeSet<_>>().len(), walked.len());
    let whole = call(
        &dir.0,
        ToolId::GraphQuery,
        json!({"operation":"unresolved","path":"app","limit":1000}),
    );
    assert_eq!(walked, ids(&whole), "the walk keeps snapshot order");
}

#[test]
fn engine_and_tool_cut_the_same_pages() {
    let dir = fixture();
    let mcp = windowed(
        &dir.0,
        json!({"operation":"unresolved","path":"app","limit":1000}),
    );
    let engine = query(
        &QueryOptions {
            op: "unresolved".into(),
            // The tool's continuations repeat its scope; the budget counts them.
            path: Some("app".into()),
            workspace: Some(dir.0.clone()),
            limit: Some(1000),
            response_window: Some(WINDOW),
            ..Default::default()
        },
        &workspace_policy(&dir.0),
    );
    let engine_ids = engine.value["rows"]
        .as_array()
        .expect("results")
        .iter()
        .map(|row| row["id"].as_str().expect("id").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(engine_ids, ids(&mcp));
    assert_eq!(
        crate::tools::ast_graph::test_support::next_offset(&engine),
        Some(ids(&mcp).len()),
        "{}",
        engine.value
    );
}
