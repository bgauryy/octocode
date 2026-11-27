//! One paging rule on every surface: a page ends at `limit` rows or at the
//! response budget, whichever comes first, and the walk is lossless.
use super::*;
use crate::ast_graph::store::{QueryOptions, query};
use octocode_test_support::json_fixture;

const WINDOW: usize = 8_000;

fn fixture() -> Fixture {
    let dir = Fixture::new();
    let calls = (0..120)
        .map(|n| format!("missing{n}();"))
        .collect::<String>();
    dir.write("app/a.ts", format!("export function a(){{ {calls} }}\n"));
    call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    dir
}

/// A graphQuery row under a response window, as the runtime dispatches it.
fn windowed(dir: &Path, row: Value) -> Value {
    windowed_at(dir, row, WINDOW)
}

fn windowed_at(dir: &Path, row: Value, window: usize) -> Value {
    let row = octocode_runtime_base::contracts::validate_built_query("graphQuery", row)
        .expect("input contract");
    let context = ExecutionContext {
        response_window: Some(window),
        ..context()
    };
    let result = execute(ToolId::GraphQuery, &row, dir, &context).expect("execute");
    octocode_runtime_base::test_support::assert_row_contract(ToolId::GraphQuery, &result.data);
    result.data
}

fn ids(page: &Value) -> Vec<String> {
    page["rows"]
        .as_array()
        .expect("results")
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn chars(value: &Value) -> usize {
    octocode_engine::json_size::utf16_units(value).expect("measurable")
}

/// `page` as the response delivers it: its row inside the response envelope,
/// each continuation published as an issued cursor.
fn published(page: &Value) -> Value {
    let mut data = page.clone();
    if let Some(next) = data.get_mut("next").and_then(Value::as_object_mut) {
        for call in next.values_mut() {
            call["input"] =
                json!({"cursor": octocode_runtime_base::response::cursor::issued::placeholder()});
        }
    }
    octocode_runtime_base::response::rows::envelope(
        ToolId::GraphQuery,
        vec![octocode_runtime_base::response::rows::result_row(
            0, data, None,
        )],
    )
}

/// The next page of `page`'s walk at `window`.
fn following(dir: &Path, page: &Value, window: usize) -> Option<Value> {
    page.pointer("/next/nextPage/input/queries/0")
        .map(|next| windowed_at(dir, next.clone(), window))
}

#[test]
fn a_page_ends_at_limit_or_the_response_budget() {
    let dir = fixture();
    let small = windowed(
        &dir.0,
        json_fixture(r#"{"operation": "unresolved", "path": "app", "limit": 5}"#),
    );
    assert_eq!(ids(&small).len(), 5, "the budget allows the whole limit");
    let mut query = json_fixture(r#"{"operation": "unresolved", "path": "app", "limit": 1000}"#);
    let mut walked = Vec::new();
    let mut pages = 0;
    let total = loop {
        let page = windowed(&dir.0, query);
        pages += 1;
        let rows = ids(&page);
        assert!(!rows.is_empty(), "a walk always moves: {page}");
        assert!(
            rows.len() == 1 || chars(&published(&page)) <= WINDOW,
            "page {pages} exceeds the response window: {}",
            chars(&published(&page))
        );
        walked.extend(rows);
        match page.pointer("/next/nextPage/input/queries/0") {
            Some(next) => query = next.clone(),
            None => break page["pagination"]["total"].as_u64().expect("total"),
        }
    };
    assert!(pages > 1, "a 1000-row limit is cut by the budget");
    assert_eq!(walked.len() as u64, total);
    assert_eq!(walked.iter().collect::<BTreeSet<_>>().len(), walked.len());
    let whole = call(
        &dir.0,
        ToolId::GraphQuery,
        json_fixture(r#"{"operation": "unresolved", "path": "app", "limit": 1000}"#),
    );
    assert_eq!(walked, ids(&whole), "the walk keeps snapshot order");
}

/// A page the budget cuts holds every whole row its response window fits,
/// however small the window: one more row would have overflowed the
/// response as delivered, and the shared pager delivers the page whole.
/// Rows count as delivered (lean by default), not as projected before the
/// lean view drops their provenance.
#[test]
fn a_budget_cut_page_holds_every_row_that_fits() {
    let dir = Fixture::new();
    let source = (0..120)
        .map(|n| format!("export function f{n}(){{ missing{n}(); }}\n"))
        .collect::<String>();
    dir.write("app/a.ts", source);
    call(
        &dir.0,
        ToolId::GraphBuild,
        json_fixture(r#"{"path": "app"}"#),
    );
    for window in [WINDOW / 6, WINDOW] {
        for operation in ["symbols", "unresolved"] {
            let page = windowed_at(
                &dir.0,
                json!({"operation":operation,"path":"app","limit":1000}),
                window,
            );
            let next = following(&dir.0, &page, window).unwrap_or_else(|| {
                panic!("{operation} at {window}: the budget cuts the page: {page}")
            });
            let delivered = published(&page);
            let size = chars(&delivered);
            assert!(
                size <= window,
                "{operation}: {size} exceeds the window {window}"
            );
            // The separator before the next row is one character.
            let with_next = size + 1 + chars(&next["rows"][0]);
            assert!(
                with_next > window,
                "{operation}: {} rows in {size} chars left room in {window} for the next row ({with_next})",
                ids(&page).len()
            );
            let response = octocode_runtime_base::response::pager::prepare(
                octocode_runtime_base::response::pager::ResponseInput {
                    tool: ToolId::GraphQuery,
                    structured: delivered,
                    options: octocode_runtime_base::response::page_model::ResponsePageOptions {
                        response_chars: Some(window),
                        paging: true,
                        ..Default::default()
                    },
                },
                &octocode_io::cancellation::Cancellation::new(),
            )
            .expect("response page");
            let row = &response.value["results"][0];
            assert!(
                row.get("rowPart").is_none() && response.value.get("next").is_none(),
                "{operation} at {window}: the pager cut the page again: {}",
                response.value
            );
            assert_eq!(row["data"]["rows"], page["rows"], "{operation} at {window}");
        }
    }
}

#[test]
fn engine_and_tool_cut_the_same_pages() {
    let dir = fixture();
    let mcp = windowed(
        &dir.0,
        json_fixture(r#"{"operation": "unresolved", "path": "app", "limit": 1000}"#),
    );
    let engine = query(
        &QueryOptions {
            op: "unresolved".into(),
            // The tool's continuations repeat its scope; the budget counts them.
            path: Some("app".into()),
            workspace: Some(dir.0.clone()),
            limit: Some(1000),
            response_window: Some(WINDOW),
            // The tool states freshness on every answer, and the budget counts it.
            check_freshness: true,
            ..Default::default()
        },
        &workspace_policy(&dir.0),
    );
    let engine_ids = engine.value["rows"]
        .as_array()
        .expect("results")
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    assert_eq!(engine_ids, ids(&mcp));
    assert_eq!(
        crate::ast_graph::test_support::next_offset(&engine),
        Some(ids(&mcp).len()),
        "{}",
        engine.value
    );
}
