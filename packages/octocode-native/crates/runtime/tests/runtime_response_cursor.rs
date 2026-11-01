//! MCP response pages are reached by copying one opaque cursor input.

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
            assert_eq!(pagination["page"], pages.len() + 1, "{pagination}");
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
        let next = pagination["next"]["input"].clone();
        support::assert_cursor_only(&next);
        octocode_native::contracts::validate(tool, next.clone()).expect("canonical cursor");
        // Exactly what an MCP client sends: the cursor field and nothing else.
        request = next;
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
async fn mcp_walks_every_captured_line_following_only_the_cursor() {
    let workspace = Workspace::new();
    let file = workspace.write("cursor.txt", source());
    let runtime = workspace.runtime(&[]);
    let queries = json!([{"path": file, "matchString": ["needle"]}]);
    let whole = runtime
        .run_mcp(
            "whole".into(),
            "localSearch".into(),
            json!({"queries": queries, "responseChars": 50_000}),
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
        json!({"queries":queries,"responseChars":900}),
    )
    .await;
    assert!(pages.len() > 2, "the fixture spans response pages");
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
    let queries = json!([{"path": file, "matchString": ["needle"]}]);
    let runtime = workspace.runtime(&env);
    let first = runtime
        .run_mcp(
            "first".into(),
            "localSearch".into(),
            json!({"queries": queries, "responseChars": 900}),
        )
        .await
        .expect("first page");
    runtime.close().await;
    std::fs::remove_file(&file).expect("remove the live source");
    let mut seen = Vec::new();
    captured(&first["structuredContent"]["results"], &mut seen);
    let mut pagination = first["structuredContent"]["responsePagination"].clone();
    let total = pagination["totalPages"].clone();
    let mut pages = 1;
    while pagination["hasMore"] == true {
        pages += 1;
        assert!(pages < 200, "finite persistent page walk");
        support::assert_cursor_only(&pagination["next"]["input"]);
        let runtime = workspace.runtime(&env);
        let page = runtime
            .run_mcp(
                "later".into(),
                "localSearch".into(),
                pagination["next"]["input"].clone(),
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

/// The `nextPage` lead of `result`'s first row.
fn next_page(result: &Value) -> Value {
    result["structuredContent"]["results"][0]["data"]["next"]["nextPage"]["input"].clone()
}

/// The same continuation issued twice (two identical requests, as a lead a
/// reader meets on several pages) is one cursor string: following either
/// reads the same next page (`limit` rows per page), so a reader that
/// follows each lead it meets reads that page once.
#[tokio::test]
async fn an_identical_continuation_is_one_cursor() {
    let workspace = Workspace::new();
    for name in ["a", "b", "c", "d", "e"] {
        workspace.write(&format!("{name}.txt"), "x\n");
    }
    let runtime = workspace.runtime(&[]);
    let request = json!({"queries":[{"operation":"files","path":".","entryType":"file",
        "extensions":["txt"],"limit":2,"view":"paths"}]});
    let mut leads = Vec::new();
    for call in ["first", "second"] {
        let result = runtime
            .run_mcp(call.into(), "structureSearch".into(), request.clone())
            .await
            .expect("listing");
        let lead = next_page(&result);
        support::assert_cursor_only(&lead);
        leads.push(lead);
    }
    assert_eq!(leads[0], leads[1], "one continuation, one cursor");
    let mut windows = Vec::new();
    for lead in &leads {
        let page = runtime
            .run_mcp("next".into(), "structureSearch".into(), lead.clone())
            .await
            .expect("next page");
        assert_ne!(page["isError"], true, "{page}");
        windows.push(page["structuredContent"]["results"][0]["data"]["files"].clone());
    }
    assert_eq!(windows[0], windows[1], "the same page");
    assert!(windows[0].as_array().is_some_and(|files| !files.is_empty()));
    let distinct = leads
        .iter()
        .map(|lead| lead["cursor"].as_str().expect("cursor"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        distinct.len(),
        1,
        "a reader following each lead reads it once"
    );
    runtime.close().await;
}
