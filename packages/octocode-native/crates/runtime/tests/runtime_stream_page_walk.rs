#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Default-layout listing walks: a streamed page sized for the response
//! window never splits into row parts, so following only each row's
//! `next.nextPage` reaches every row exactly once; and when a smaller
//! explicit window does split a page, no continuation an agent can follow
//! skips the rows of a later part.

mod support;

use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use support::Workspace;

const WINDOW: usize = 20_000;

/// Serialized size in the unit the response pager counts.
fn chars(value: &Value) -> usize {
    value.to_string().encode_utf16().count()
}

/// Source lines whose JSON form expands: quotes, backslashes, tabs,
/// non-ASCII and astral characters, plus one over-long line per file that
/// is clipped to `matchContentLength`.
fn hit_line(file: usize, line: usize) -> String {
    format!(
        "let s{line} = \"needle \\\"{file}\\\" \\\\ path\t中文 \u{1F600} {}\";\n",
        "é".repeat(line % 7)
    )
}

fn search_fixture(workspace: &Workspace, files: usize) -> (String, usize) {
    let mut total = 0;
    let mut root = None;
    for file in 0..files {
        let rows = 5 + (file * 7) % 45;
        let mut body = (0..rows)
            .map(|line| hit_line(file, line))
            .collect::<String>();
        body.push_str(&format!("let long = \"needle {}\";\n", "x".repeat(600)));
        total += rows + 1;
        let path = workspace.write(&format!("src/m{file:02}/file_{file:02}.rs"), body);
        root = path
            .parent()
            .and_then(|dir| dir.parent())
            .map(|dir| dir.to_path_buf());
    }
    (root.unwrap().to_string_lossy().into_owned(), total)
}

fn search(root: &str, goal: &str) -> Value {
    json!({"path": root, "searchText": "needle", "goal": goal,
        "reasoning": "Walk every hit through row continuations."})
}

/// The structured envelope of a CLI or MCP call.
async fn call(runtime: &ToolRuntime, mcp: bool, id: usize, tool: &str, input: Value) -> Value {
    if mcp {
        let result = runtime
            .execute_mcp(format!("walk-{id}"), tool.into(), input)
            .await
            .expect("mcp call");
        assert_ne!(result["isError"], true, "{result}");
        result["structuredContent"].clone()
    } else {
        runtime
            .execute(format!("walk-{id}"), tool.into(), input)
            .await
            .expect("cli call")
            .structured_content
    }
}

fn match_rows(envelope: &Value) -> Vec<(String, u64)> {
    envelope["results"][0]["data"]["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| {
            let path = file["path"].as_str().expect("path").to_owned();
            file["matches"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |row| (path.clone(), row["line"].as_u64().expect("line")))
        })
        .collect()
}

/// Follow only `results[0].data.next.<key>` from `first`, unchanged, and
/// return (rows seen in order, calls). Every page must fit one response.
async fn walk_row_continuations(
    runtime: &ToolRuntime,
    mcp: bool,
    tool: &str,
    first: Value,
    key: &str,
    rows: impl Fn(&Value) -> Vec<String>,
) -> (Vec<String>, usize) {
    let mut seen = Vec::new();
    let mut input = json!({"queries": [first]});
    let mut calls = 0;
    loop {
        calls += 1;
        assert!(calls < 60, "{tool} walk terminates");
        let envelope = call(runtime, mcp, calls, tool, input).await;
        assert!(
            envelope.get("responsePagination").is_none(),
            "{tool} page {calls} split into response parts ({} chars)",
            chars(&envelope)
        );
        assert!(envelope["results"][0].get("rowPart").is_none());
        assert!(
            chars(&envelope) <= WINDOW,
            "{tool} page {calls}: {} chars",
            chars(&envelope)
        );
        seen.extend(rows(&envelope));
        let data = &envelope["results"][0]["data"];
        match data["next"].get(key) {
            Some(next) => {
                assert_eq!(next["tool"], tool, "{next}");
                input = json!({"queries": [next["query"].clone()]});
            }
            None => {
                assert_ne!(data["pagination"]["hasMore"], true, "{data}");
                break;
            }
        }
    }
    (seen, calls)
}

#[tokio::test]
async fn a_default_local_search_walk_fits_the_window_and_reaches_every_hit_once() {
    let workspace = Workspace::new();
    let (root, total) = search_fixture(&workspace, 48);
    let runtime = workspace.runtime(&[("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH", WINDOW.to_string())]);
    let long_goal = "Find every needle hit across the fixture tree. ".repeat(6);
    for (mcp, goal) in [
        (false, "Walk the hits."),
        (true, "Walk the hits."),
        (false, long_goal.as_str()),
    ] {
        let (seen, calls) = walk_row_continuations(
            &runtime,
            mcp,
            "localSearch",
            search(&root, goal),
            "nextPage",
            |envelope| {
                match_rows(envelope)
                    .into_iter()
                    .map(|(path, line)| format!("{path}:{line}"))
                    .collect()
            },
        )
        .await;
        let unique: BTreeSet<_> = seen.iter().collect();
        assert_eq!(unique.len(), seen.len(), "mcp {mcp}: a hit was shown twice");
        assert_eq!(seen.len(), total, "mcp {mcp}: every hit once");
        assert!(calls > 2, "the fixture spans several pages: {calls}");
    }
    runtime.close().await;
}

/// Every entry of a structureSearch `files` page as `base/dir/name`: groups
/// list each entry as its name plus a ` (<size>[, …])` suffix, and every
/// group names its `dir`, also when it continues one from the last page.
fn listed_paths(envelope: &Value) -> Vec<String> {
    let base = envelope["base"].as_str().expect("base");
    envelope["results"][0]["data"]["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|group| {
            let dir = group["dir"].as_str().expect("dir");
            group["files"]
                .as_array()
                .expect("entries")
                .iter()
                .map(move |entry| {
                    let entry = entry.as_str().expect("entry");
                    let name = entry
                        .strip_suffix(')')
                        .and_then(|entry| entry.rsplit_once(" ("))
                        .map_or(entry, |(name, _)| name);
                    format!("{base}/{dir}/{name}")
                })
        })
        .collect()
}

#[tokio::test]
async fn a_default_structure_listing_walk_fits_the_window_and_lists_every_file_once() {
    let workspace = Workspace::new();
    let mut root = None;
    for n in 0..900 {
        let path = workspace.write(
            &format!("tree/d{:02}/módulo_\"{n:04}\"_文件.rs", n % 30),
            "x\n",
        );
        root = path
            .parent()
            .and_then(|dir| dir.parent())
            .map(|dir| dir.to_path_buf());
    }
    let root = root.unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH", WINDOW.to_string())]);
    for (mcp, detail) in [(false, "basic"), (true, "basic"), (false, "full")] {
        let first = json!({"operation": "files", "path": root, "pathPattern": "**/*.rs",
            "detail": detail, "goal": "List every file.", "reasoning": "Walk the listing."});
        let (seen, calls) = walk_row_continuations(
            &runtime,
            mcp,
            "structureSearch",
            first,
            "nextPage",
            listed_paths,
        )
        .await;
        let unique: BTreeSet<_> = seen.iter().collect();
        assert_eq!(
            unique.len(),
            seen.len(),
            "{detail}: a file was listed twice"
        );
        assert_eq!(seen.len(), 900, "{detail}: every file once");
        assert!(calls > 1, "{detail}: the fixture spans several pages");
    }
    let first = json!({"operation": "tree", "path": root, "maxDepth": 2,
        "goal": "Outline the tree.", "reasoning": "Walk the outline."});
    let (seen, _) = walk_row_continuations(
        &runtime,
        false,
        "structureSearch",
        first,
        "nextPage",
        |envelope| {
            envelope["results"][0]["data"]["entries"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|entry| entry.as_str().expect("entry").to_owned())
                .collect()
        },
    )
    .await;
    assert_eq!(seen.iter().collect::<BTreeSet<_>>().len(), seen.len());
    assert_eq!(seen.len(), 930, "30 directories and their 900 files");
    runtime.close().await;
}

/// An explicit row-scoped `responseCharLength` smaller than a page still
/// splits it into row parts. The row's page continuation then rides its last
/// part, so an agent that follows a row continuation whenever one is shown,
/// and the response continuation otherwise, sees every hit exactly once, and
/// every unfinished response offers one of the two.
#[tokio::test]
async fn a_split_page_offers_its_row_continuation_only_after_its_last_part() {
    let workspace = Workspace::new();
    let (root, total) = search_fixture(&workspace, 10);
    let runtime = workspace.runtime(&[("OCTOCODE_OUTPUT_DEFAULT_CHAR_LENGTH", WINDOW.to_string())]);
    for window in [4_000, 2_500] {
        for mcp in [false, true] {
            let mut input = json!({"queries": [search(&root, "Walk the hits.")],
                "responseCharLength": window, "responseScope": "rows"});
            let mut seen = Vec::new();
            let mut split_parts = 0;
            let mut calls = 0;
            loop {
                calls += 1;
                assert!(calls < 400, "walk terminates");
                let envelope = call(&runtime, mcp, calls, "localSearch", input).await;
                seen.extend(match_rows(&envelope));
                let row = &envelope["results"][0];
                let row_next = row["data"]["next"].get("nextPage").cloned();
                if let Some(part) = row.get("rowPart") {
                    split_parts += 1;
                    if row_next.is_some() {
                        assert_eq!(
                            part["part"], part["of"],
                            "row next on an earlier part: {part}"
                        );
                    }
                }
                let response_next = envelope["responsePagination"]["next"]["query"].clone();
                input = match (row_next, response_next.is_object()) {
                    (Some(next), _) => json!({"queries": [next["query"].clone()]}),
                    (None, true) => response_next,
                    (None, false) => {
                        assert_ne!(row["data"]["pagination"]["hasMore"], true, "{row}");
                        break;
                    }
                };
            }
            let label = format!("window {window} mcp {mcp}");
            assert!(split_parts > 0, "{label}: the window splits pages");
            let unique: BTreeSet<_> = seen.iter().collect();
            assert_eq!(unique.len(), seen.len(), "{label}: a hit was shown twice");
            assert_eq!(seen.len(), total, "{label}: every hit once");
        }
    }
    runtime.close().await;
}
