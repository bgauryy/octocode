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
        let payload = support::mcp_payload(&result);
        let pagination = payload["responsePagination"].clone();
        if !pages.is_empty() || pagination.is_object() {
            assert_ne!(pagination["restart"], true, "{pagination}");
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
    let whole = support::mcp_payload(&whole);
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
        captured(&support::mcp_payload(page)["results"], &mut seen);
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
    let first_payload = support::mcp_payload(&first);
    captured(&first_payload["results"], &mut seen);
    let mut pagination = first_payload["responsePagination"].clone();
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
        captured(&support::mcp_payload(&page)["results"], &mut seen);
        pagination = support::mcp_payload(&page)["responsePagination"].clone();
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
    support::mcp_payload(result)["results"][0]["data"]["next"]["nextPage"]["input"].clone()
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
        windows.push(support::mcp_payload(&page)["results"][0]["data"]["files"].clone());
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

/// The pages after the first of a persistent walk from `start`, each as its
/// captured lines and its warnings. Every continuation is the issued cursor
/// with `beside` merged in, read by a restarted runtime; `evicted` drops the
/// saved responses first, so each page re-executes instead of replaying.
async fn later_pages(
    workspace: &Workspace,
    start: &Value,
    beside: &Value,
    evicted: bool,
) -> Vec<(Vec<String>, Value)> {
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let mut runtime = workspace.runtime(&env);
    let first = runtime
        .run("first".into(), "localSearch".into(), start.clone())
        .await
        .expect("first page")
        .structured_content
        .into_value();
    let mut next = first.pointer("/responsePagination/next/input").cloned();
    let mut pages = Vec::new();
    while let Some(mut input) = next {
        assert!(pages.len() < 200, "the walk terminates");
        support::assert_cursor_only(&input);
        for (field, value) in beside.as_object().into_iter().flatten() {
            input[field] = value.clone();
        }
        runtime.close().await;
        if evicted {
            let store = workspace.home.join("tmp/response-pages");
            for entry in std::fs::read_dir(store).expect("store").flatten() {
                std::fs::remove_file(entry.path()).expect("evict entry");
            }
        }
        runtime = workspace.runtime(&env);
        let page = runtime
            .run("later".into(), "localSearch".into(), input)
            .await
            .unwrap_or_else(|error| panic!("page {}: {error:?}", pages.len() + 2))
            .structured_content
            .into_value();
        let mut lines = Vec::new();
        captured(&page["results"], &mut lines);
        pages.push((lines, page["warnings"].clone()));
        next = page.pointer("/responsePagination/next/input").cloned();
    }
    runtime.close().await;
    pages
}

/// A page size beside a cursor has one reading: the cursor keeps the size it
/// was issued with. The field is dropped and each page says so, and the walk
/// reads page for page the rows the cursor alone reads, whether its pages
/// replay or re-execute.
#[tokio::test]
async fn a_page_size_beside_a_cursor_reads_the_same_pages_and_says_so() {
    let workspace = Workspace::new();
    let file = workspace.write("captured.txt", source());
    let start = json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":1200});
    for evicted in [false, true] {
        let alone = later_pages(&workspace, &start, &json!({}), evicted).await;
        assert!(alone.len() > 1, "the fixture spans several pages");
        assert!(
            alone.iter().all(|(_, warnings)| warnings.is_null()),
            "{alone:?}"
        );
        for size in [60_000, 1200, 300] {
            let sized =
                later_pages(&workspace, &start, &json!({"responseChars": size}), evicted).await;
            let rows = |pages: &[(Vec<String>, Value)]| {
                pages
                    .iter()
                    .map(|(lines, _)| lines.clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                rows(&sized),
                rows(&alone),
                "responseChars:{size} beside the cursor, evicted: {evicted}"
            );
            for (_, warnings) in &sized {
                let notes = warnings.as_array().map_or(&[][..], Vec::as_slice);
                assert_eq!(notes.len(), 1, "{warnings}");
                let note = notes[0].as_str().unwrap_or_default();
                assert!(
                    note.contains("responseChars") && note.contains("page size it was issued with"),
                    "{note}"
                );
            }
        }
    }
}

/// A cursor call is the cursor alone: any field beside it but the page size
/// is refused, never silently ignored.
#[tokio::test]
async fn a_cursor_call_refuses_another_field_beside_it() {
    let workspace = Workspace::new();
    let file = workspace.write("captured.txt", source());
    let runtime = workspace.runtime(&[]);
    let first = runtime
        .run(
            "first".into(),
            "localSearch".into(),
            json!({"queries":[{"path":file,"matchString":["needle"]}],"responseChars":1200}),
        )
        .await
        .expect("first page")
        .structured_content
        .into_value();
    let next = first["responsePagination"]["next"]["input"].clone();
    support::assert_cursor_only(&next);
    for (field, value) in [
        ("path", json!(file)),
        ("matchString", json!(["needle"])),
        ("page", json!(2)),
    ] {
        let mut input = next.clone();
        input[field] = value;
        let error = runtime
            .run("beside".into(), "localSearch".into(), input)
            .await
            .expect_err("a cursor call carries only the cursor");
        assert_eq!(
            error.code,
            octocode_contract_types::ErrorCode::InvalidInput,
            "{field}: {error:?}"
        );
    }
    runtime
        .run("page".into(), "localSearch".into(), next)
        .await
        .expect("the cursor alone reads on");
    runtime.close().await;
}
