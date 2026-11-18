//! MCP response pages are reached by copying one opaque cursor input.

use crate::support::{self, Run as _, Workspace};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

const LINES: usize = 120;

fn source() -> String {
    (0..LINES)
        .map(|index| format!("needle captured {index:03}\n"))
        .collect()
}

/// The MCP pages of `input`, each requested with only the previous page's
/// cursor. Asserts every continuation is the cursor alone and canonical,
/// and the last page ends the walk.
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
        if pagination["next"].is_null() {
            break;
        }
        assert!(pages.len() < 200, "the walk terminates");
        let next = pagination["next"]["input"].clone();
        support::assert_cursor_only(&next);
        octocode_runtime_base::contracts::validate(tool, next.clone()).expect("canonical cursor");
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
    let mut pages = 1;
    while pagination["next"].is_object() {
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
    let request = json_fixture(
        r#"{
            "queries": [
                {
                    "operation": "files",
                    "path": ".",
                    "entryType": "file",
                    "extensions": ["txt"],
                    "limit": 2,
                    "view": "paths"
                }
            ]
        }"#,
    );
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
/// complete result rows (including row parts and indices) and its warnings. Every continuation is the issued cursor
/// with `beside` merged in, read by a restarted runtime. `evict_after` keeps
/// that many later pages before evicting the body on every following read.
async fn later_pages(
    workspace: &Workspace,
    tool: &str,
    start: &Value,
    beside: &Value,
    evict_after: Option<usize>,
) -> Vec<(Value, Value)> {
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let mut runtime = workspace.runtime(&env);
    let first = runtime
        .run("first".into(), tool.into(), start.clone())
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
        if evict_after.is_some_and(|count| pages.len() >= count) {
            let store = workspace.home.join("tmp/response-pages");
            for entry in std::fs::read_dir(store).expect("store").flatten() {
                std::fs::remove_file(entry.path()).expect("evict entry");
            }
        }
        runtime = workspace.runtime(&env);
        let page = runtime
            .run("later".into(), tool.into(), input)
            .await
            .unwrap_or_else(|error| panic!("page {}: {error:?}", pages.len() + 2))
            .structured_content
            .into_value();
        pages.push((page["results"].clone(), page["warnings"].clone()));
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
        let alone = later_pages(
            &workspace,
            "localSearch",
            &start,
            &json!({}),
            evicted.then_some(0),
        )
        .await;
        assert!(alone.len() > 1, "the fixture spans several pages");
        assert!(
            alone.iter().all(|(_, warnings)| warnings.is_null()),
            "{alone:?}"
        );
        for size in [60_000, 1200, 300] {
            let sized = later_pages(
                &workspace,
                "localSearch",
                &start,
                &json!({"responseChars": size}),
                evicted.then_some(0),
            )
            .await;
            let rows = |pages: &[(Value, Value)]| {
                pages
                    .iter()
                    .map(|(rows, _)| rows.clone())
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

/// Re-executing an evicted response must recover its original page cuts,
/// including the original repair notes and space reserved for rejected rows.
async fn repaired_pages_after_eviction(repair: bool, rejected: bool) {
    let workspace = Workspace::new();
    let file = workspace.write("captured.txt", source());
    let mut row = json!({"path":file,"matchString":["needle"]});
    if repair {
        row["limit"] = json!(LINES.to_string());
    }
    let mut queries = vec![row];
    if rejected {
        queries.insert(0, json!({"path":file,"matchString":["needle"],"offset":1}));
    }
    let start = json!({"queries":queries,"responseChars":1200});
    let retained = later_pages(&workspace, "localSearch", &start, &json!({}), None).await;
    assert!(retained.len() > 1, "the fixture spans response pages");
    for after in [0, 1] {
        let rerun = later_pages(&workspace, "localSearch", &start, &json!({}), Some(after)).await;
        assert_eq!(
            rerun, retained,
            "eviction preserves complete result rows and warnings on the same page"
        );
    }
    if repair {
        let sized = later_pages(
            &workspace,
            "localSearch",
            &start,
            &json_fixture(r#"{"responseChars": 60000}"#),
            Some(1),
        )
        .await;
        assert_eq!(sized.len(), retained.len());
        for ((rows, original), (sized_rows, warnings)) in retained.iter().zip(&sized) {
            assert_eq!(
                sized_rows, rows,
                "a cursor-call note does not move page edges"
            );
            let warnings = warnings.as_array().expect("cursor-call warning");
            let (call, saved) = warnings.split_last().expect("one call note");
            assert_eq!(saved, original.as_array().map_or(&[][..], Vec::as_slice));
            assert!(
                call.as_str()
                    .is_some_and(|note| note.contains("responseChars"))
            );
        }
    }
}

#[tokio::test]
async fn repaired_response_pages_survive_eviction() {
    repaired_pages_after_eviction(true, false).await;
}

#[tokio::test]
async fn rejected_response_pages_survive_eviction() {
    repaired_pages_after_eviction(false, true).await;
}

#[tokio::test]
async fn repaired_mixed_response_pages_survive_eviction() {
    repaired_pages_after_eviction(true, true).await;
}

/// A mixed read batch pages its response: a long line's byte walk and line
/// reads beside a rejected row, and one line too wide for its share but
/// shorter than a long line, which is read whole (never cut across byte
/// windows) and which the response pager splits into parts. Its later pages
/// replay identically after eviction. The rejected row changes the reads'
/// shares even though it is not replayed.
#[tokio::test]
async fn mixed_read_response_pages_survive_eviction() {
    let long_line = octocode_tools_shared::source_view::pagination::MIN_BYTE_PAGE;
    let workspace = Workspace::new();
    let long = format!("needle captured {}\n", "x".repeat(7000));
    let wide = format!("needle captured {}\n", "x".repeat(3500));
    assert!(wide.len() < long_line && long.len() > long_line);
    let large = workspace.write("oversized.txt", long);
    let wide = workspace.write("wide.txt", wide);
    let lines = workspace.write("captured.txt", source());
    for window in [900, 1200, 3000] {
        let start = json!({"queries":[
            {"path":large}, {"path":lines,"offset":1}, {"path":lines}, {"path":wide}
        ],"responseChars":window});
        let runtime = workspace.runtime(&[]);
        let first = runtime
            .run("mixed".into(), "localFetch".into(), start.clone())
            .await
            .expect("first page")
            .structured_content
            .into_value();
        runtime.close().await;
        let retained = later_pages(&workspace, "localFetch", &start, &json!({}), None).await;
        assert!(!retained.is_empty(), "the mixed read pages at {window}");
        // The walk reads byte-paged and line-paged rows.
        let units = std::iter::once(&first["results"])
            .chain(retained.iter().map(|(rows, _)| rows))
            .flat_map(|rows| rows.as_array().into_iter().flatten())
            .filter_map(|row| row.pointer("/data/pagination/unit")?.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert!(
            units.contains("bytes") && units.contains("lines"),
            "a mixed read at {window}: {units:?}"
        );
        for after in [0, 1] {
            let rerun =
                later_pages(&workspace, "localFetch", &start, &json!({}), Some(after)).await;
            assert_eq!(
                rerun, retained,
                "mixed read at {window} after {after} retained pages"
            );
        }
    }
}

#[tokio::test]
async fn a_repaired_response_still_refuses_changed_source_after_eviction() {
    let workspace = Workspace::new();
    let file = workspace.write("captured.txt", source());
    let env = [("OCTOCODE_STORAGE_MODE", "persistent".into())];
    let runtime = workspace.runtime(&env);
    let first = runtime
        .run(
            "first".into(),
            "localSearch".into(),
            json!({
                "queries":[{"path":file,"matchString":["needle"],"limit":LINES.to_string()}],
                "responseChars":1200
            }),
        )
        .await
        .expect("first page")
        .structured_content;
    let next = first["responsePagination"]["next"]["input"].clone();
    support::assert_cursor_only(&next);
    runtime.close().await;
    for entry in std::fs::read_dir(workspace.home.join("tmp/response-pages"))
        .unwrap()
        .flatten()
    {
        std::fs::remove_file(entry.path()).unwrap();
    }
    std::fs::write(file, source().replace("captured 007", "changed 007")).unwrap();
    let runtime = workspace.runtime(&env);
    let error = runtime
        .run("changed".into(), "localSearch".into(), next)
        .await
        .expect_err("changed evidence is not replayed");
    assert_eq!(
        error.code,
        octocode_contract_types::ErrorCode::StaleSnapshot
    );
    runtime.close().await;
}

/// A cursor call is the cursor alone: a field beside it at another value
/// than the cursor's saved query holds, or a private progress field, is
/// refused, never silently ignored. (One at its saved value is dropped and
/// said: `runtime_cursor_beside`.)
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
        ("path", json!(workspace.write("other.txt", source()))),
        ("matchString", json_fixture(r#"["captured"]"#)),
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
