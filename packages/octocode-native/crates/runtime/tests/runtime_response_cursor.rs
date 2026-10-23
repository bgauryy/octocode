//! Whole-response pages are reached by one opaque cursor: a client that
//! copies only `responsePagination.next.query.responseSnapshot` (the one
//! continuation field an MCP client's published schema offers) walks every
//! page of every scope, and the walk reproduces the unpaged response.

use crate::support::{self, Run as _, Workspace};
use serde_json::{Value, json};

const LINES: usize = 120;

fn source() -> String {
    (0..LINES)
        .map(|index| format!("needle captured {index:03}\n"))
        .collect()
}

/// The MCP pages of `input`, each requested with only the previous page's
/// cursor. Asserts every continuation is the cursor alone, pages count up
/// by one under a fixed `totalPages`, and the last page ends the walk.
async fn walk(
    runtime: &octocode_native::runtime::ToolRuntime,
    tool: &str,
    input: Value,
) -> Vec<Value> {
    let mut pages = Vec::new();
    let mut request = input;
    loop {
        let result = runtime
            .run_mcp(format!("page-{}", pages.len()), tool.into(), request)
            .await
            .expect("page");
        assert_ne!(result["isError"], true, "{result}");
        let pagination = result["structuredContent"]["responsePagination"].clone();
        if !pages.is_empty() || pagination.is_object() {
            assert_eq!(pagination["currentPage"], pages.len() + 1, "{pagination}");
            assert_ne!(pagination["restart"], true, "{pagination}");
        }
        if let Some(first) = pages.first() {
            let first: &Value = first;
            assert_eq!(
                pagination["totalPages"],
                first["structuredContent"]["responsePagination"]["totalPages"],
                "totalPages is exact from the first page: {pagination}"
            );
        }
        pages.push(result);
        if pagination["hasMore"] != true {
            assert!(pagination["next"].is_null(), "{pagination}");
            break;
        }
        assert!(pages.len() < 200, "the walk terminates");
        let next = pagination["next"]["query"].clone();
        support::assert_cursor_only(&next);
        octocode_native::contracts::validate(tool, next.clone()).expect("canonical cursor");
        // Exactly what an MCP client sends: the cursor field and nothing else.
        request = json!({"responseSnapshot": next["responseSnapshot"].clone()});
    }
    pages
}

/// Every string leaf that names a captured line, in document order.
fn captured(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::String(text) if text.contains("needle captured") => found.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| captured(item, found)),
        Value::Object(map) => map.values().for_each(|item| captured(item, found)),
        _ => {}
    }
}

#[tokio::test]
async fn every_scope_walks_every_page_following_only_the_cursor() {
    let workspace = Workspace::new();
    let file = workspace.write("cursor.txt", source());
    let runtime = workspace.runtime(&[]);
    let queries = json!([{"path": file, "matchString": "needle", "pageSize": LINES}]);
    let whole = runtime
        .run_mcp(
            "whole".into(),
            "localSearch".into(),
            json!({"queries": queries, "responseLength": 50_000}),
        )
        .await
        .expect("unpaged");
    let whole = &whole["structuredContent"];
    assert!(whole.get("responsePagination").is_none(), "{whole}");
    let mut expected = Vec::new();
    captured(&whole["results"], &mut expected);
    expected.sort();
    assert_eq!(expected.len(), LINES, "{whole}");

    let pages = walk(
        &runtime,
        "localSearch",
        json!({"queries": queries, "responseLength": 900, "responseScope": "structured"}),
    )
    .await;
    assert!(pages.len() > 2, "structured: the response pages");
    let joined: String = pages
        .iter()
        .map(|page| {
            page["structuredContent"]["responseWindow"]
                .as_str()
                .expect("window")
        })
        .collect();
    let envelope: Value = serde_json::from_str(&joined).expect("the windows join into JSON");
    assert_eq!(envelope["results"], whole["results"], "structured");

    let pages = walk(
        &runtime,
        "localSearch",
        json!({"queries": queries, "responseLength": 900, "responseScope": "rows"}),
    )
    .await;
    assert!(pages.len() > 2, "rows: the response pages");
    let mut seen = Vec::new();
    for page in &pages {
        captured(&page["structuredContent"]["results"], &mut seen);
    }
    seen.sort();
    assert_eq!(seen, expected, "every captured line exactly once");
    runtime.close().await;
}

/// A cursor survives a persistent restart: a new runtime (another CLI
/// process) serves every later page from the saved response alone.
#[tokio::test]
async fn the_cursor_walks_saved_pages_across_persistent_restarts() {
    let workspace = Workspace::new();
    let file = workspace.write("cursor.txt", source());
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let queries = json!([{"path": file, "matchString": "needle", "pageSize": LINES}]);
    let runtime = workspace.runtime(&env);
    let first = runtime
        .run_mcp(
            "first".into(),
            "localSearch".into(),
            json!({"queries": queries, "responseLength": 900}),
        )
        .await
        .expect("first page");
    runtime.close().await;
    std::fs::remove_file(&file).expect("remove the live source");
    let mut seen = Vec::new();
    captured(&first["structuredContent"]["results"], &mut seen);
    let mut pagination = first["structuredContent"]["responsePagination"].clone();
    assert_eq!(pagination["scope"], "rows", "{pagination}");
    let total = pagination["totalPages"].clone();
    while pagination["hasMore"] == true {
        support::assert_cursor_only(&pagination["next"]["query"]);
        let runtime = workspace.runtime(&env);
        let page = runtime
            .run_mcp(
                "later".into(),
                "localSearch".into(),
                json!({"responseSnapshot": pagination["next"]["query"]["responseSnapshot"]}),
            )
            .await
            .expect("saved page");
        runtime.close().await;
        captured(&page["structuredContent"]["results"], &mut seen);
        pagination = page["structuredContent"]["responsePagination"].clone();
        assert_eq!(pagination["replayed"], true, "{pagination}");
        assert_eq!(pagination["totalPages"], total, "{pagination}");
    }
    seen.sort();
    let expected: Vec<String> = source().lines().map(str::to_owned).collect();
    assert_eq!(seen.len(), expected.len(), "{seen:?}");
    for (seen, line) in seen.iter().zip(&expected) {
        assert!(seen.contains(line.as_str()), "{seen} / {line}");
    }
}
