//! Default-layout listing walks: a streamed page sized for the response
//! window never splits into row parts, so following only each row's
//! `next.nextPage` reaches every row exactly once; and when a smaller
//! explicit window does split a page, no continuation an agent can follow
//! skips the rows of a later part.

use crate::support::{self, Run as _};

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
    json!({"path": root, "matchString": "needle", "mainGoal": goal,
        "reasoning": "Walk every hit through row continuations."})
}

/// The structured envelope of a CLI or MCP call.
async fn call(runtime: &ToolRuntime, mcp: bool, id: usize, tool: &str, input: Value) -> Value {
    if mcp {
        let result = runtime
            .run_mcp(format!("walk-{id}"), tool.into(), input)
            .await
            .expect("mcp call");
        assert_ne!(result["isError"], true, "{result}");
        result["structuredContent"].clone()
    } else {
        runtime
            .run(format!("walk-{id}"), tool.into(), input)
            .await
            .expect("cli call")
            .structured_content
            .into_value()
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
                input = next["query"].clone();
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
    let runtime = workspace.runtime(&[]);
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

/// Every hit row of every default page keeps its `enclosing` name:
/// the page cut reserves the names before it fills rows, so a wide result
/// pages into more, smaller pages instead of dropping names.
#[tokio::test]
async fn every_default_local_search_page_names_the_enclosing_declaration_of_every_row() {
    let workspace = Workspace::new();
    let mut total = 0;
    let mut root = None;
    for file in 0..48 {
        let rows = 5 + (file * 7) % 45;
        let body = (0..rows)
            .map(|line| format!("    let s{line} = needle_value_{file}_{line}();\n"))
            .collect::<String>();
        total += rows;
        let path = workspace.write(
            &format!("src/m{file:02}/file_{file:02}.rs"),
            format!("fn holder_{file:02}() {{\n{body}}}\n"),
        );
        root = path
            .parent()
            .and_then(|dir| dir.parent())
            .map(|dir| dir.to_path_buf());
    }
    let root = root.unwrap().to_string_lossy().into_owned();
    let runtime = workspace.runtime(&[]);
    let mut input = json!({"queries": [search(&root, "Walk the hits.")]});
    let mut seen = 0;
    for calls in 1.. {
        assert!(calls < 60, "the walk terminates");
        let envelope = call(&runtime, false, calls, "localSearch", input).await;
        let data = &envelope["results"][0]["data"];
        let warnings = data["warnings"].to_string();
        assert!(!warnings.contains("enclosing"), "page {calls}: {warnings}");
        for file in data["files"].as_array().into_iter().flatten() {
            // A row names its declaration, or references the nearest earlier
            // row of the file that names the line it gives.
            let mut named = std::collections::HashMap::new();
            for row in file["matches"].as_array().into_iter().flatten() {
                seen += 1;
                if let Some(enclosing) = row.get("enclosing") {
                    named.insert(enclosing["line"].clone(), enclosing["symbolName"].clone());
                }
                let owner = row
                    .get("enclosing")
                    .map(|enclosing| &enclosing["symbolName"])
                    .or_else(|| named.get(&row["enclosingLine"]));
                assert!(
                    owner
                        .and_then(|name| name.as_str())
                        .is_some_and(|name| name.starts_with("holder_")),
                    "page {calls}: {} row {row} has no enclosing",
                    file["path"]
                );
            }
        }
        match data["next"].get("nextPage") {
            Some(next) => input = next["query"].clone(),
            None => break,
        }
    }
    assert_eq!(seen, total, "every hit once");
    runtime.close().await;
}

/// Every entry of a structureSearch `files` page as `base/dir/name`: groups
/// list each entry as its name plus a ` (<size>[, …])` suffix, and every
/// group names its `dir`, also when it continues one from the last page.
fn listed_paths(envelope: &Value) -> Vec<String> {
    let base = envelope["root"].as_str().expect("root");
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
    let runtime = workspace.runtime(&[]);
    for (mcp, detail) in [(false, "basic"), (true, "basic"), (false, "details")] {
        let mut first = json!({"operation": "files", "path": root, "include": ["**/*.rs"],
            "mainGoal": "List every file.", "reasoning": "Walk the listing."});
        if detail == "details" {
            first["extras"] = json!(["details"]);
        }
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
        "mainGoal": "Outline the tree.", "reasoning": "Walk the outline."});
    let (seen, _) = walk_row_continuations(
        &runtime,
        false,
        "structureSearch",
        first,
        "nextPage",
        |envelope| {
            // A `{dir, files}` group names its entries under its
            // workspace-relative `dir`.
            envelope["results"][0]["data"]["files"]
                .as_array()
                .into_iter()
                .flatten()
                // Files only: a directory is its own group, or a `name/`
                // entry when no group lists it.
                .flat_map(|group| {
                    group["files"]
                        .as_array()
                        .expect("group entries")
                        .iter()
                        .filter_map(|name| {
                            let name = name.as_str().expect("entry");
                            (!name.ends_with('/'))
                                .then(|| format!("{}/{name}", group["dir"].as_str().expect("dir")))
                        })
                        .collect::<Vec<_>>()
                })
                .collect()
        },
    )
    .await;
    assert_eq!(seen.iter().collect::<BTreeSet<_>>().len(), seen.len());
    assert_eq!(seen.len(), 900, "every file of the 30 directories once");
    runtime.close().await;
}

/// An explicit row-scoped `responseLength` smaller than a page still
/// splits it into row parts. The row's page continuation then rides its last
/// part, so an agent that follows a row continuation whenever one is shown,
/// and the response continuation otherwise, sees every hit exactly once, and
/// every unfinished response offers one of the two.
#[tokio::test]
async fn a_split_page_offers_its_row_continuation_only_after_its_last_part() {
    let workspace = Workspace::new();
    let (root, total) = search_fixture(&workspace, 10);
    let runtime = workspace.runtime(&[]);
    for window in [4_000, 2_500] {
        for mcp in [false, true] {
            let mut input = json!({"queries": [search(&root, "Walk the hits.")],
                "responseLength": window, "responseScope": "rows"});
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
                    (Some(next), _) => next["query"].clone(),
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
