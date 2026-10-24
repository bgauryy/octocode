//! Evidence walks exercise both transports with varied response budgets.
use crate::support::{self, Run as _, Workspace};
use octocode_native::runtime::ToolRuntime;
use serde_json::{Value, json};
use std::collections::BTreeSet;

async fn output(runtime: &ToolRuntime, mcp: bool, tool: &str, input: Value) -> Value {
    if mcp {
        let value = runtime
            .run_mcp("walk".into(), tool.into(), input)
            .await
            .expect("MCP call");
        assert_ne!(value["isError"], true, "{value}");
        value["structuredContent"].clone()
    } else {
        runtime
            .run("walk".into(), tool.into(), input)
            .await
            .expect("CLI call")
            .structured_content
            .into_value()
    }
}

async fn walk(
    runtime: &ToolRuntime,
    mcp: bool,
    tool: &str,
    query: Value,
    budget: usize,
    rows: impl Fn(&Value) -> Vec<String>,
) -> Vec<String> {
    let mut walker =
        support::checks::Walker::new(tool, json!({"queries":[query],"responseChars":budget}));
    while let Some(input) = walker.next_query() {
        let value = output(runtime, mcp, tool, input).await;
        let chars = value.to_string().encode_utf16().count();
        assert!(
            chars <= budget || value["responsePagination"]["oversized"] == true,
            "size must be bounded or explicitly terminal: {chars}/{budget}"
        );
        walker.accept(support::checks::Page {
            rows: rows(&value),
            output: value,
        });
    }
    let walk = walker.finish();
    assert!(walk.queries.len() > 1, "fixture exercises continuation");
    assert_eq!(
        walk.rows.iter().collect::<BTreeSet<_>>().len(),
        walk.rows.len(),
        "no evidence appears twice"
    );
    walk.rows
}

#[tokio::test]
async fn search_walks_reach_every_hit_once_with_source_coordinates() {
    let workspace = Workspace::new();
    let mut expected = BTreeSet::new();
    for file in 0..48 {
        let path = format!("src/m{file:02}/file_{file:02}.rs");
        let body = (0..30)
            .map(|line| {
                format!(
                    "    let value_{line} = needle(\"界😀 {}\");\n",
                    "é".repeat(line % 7)
                )
            })
            .collect::<String>();
        workspace.write(&path, format!("fn holder_{file:02}() {{\n{body}}}\n"));
        for line in 2..=31 {
            expected.insert(format!("{path}:{line}"));
        }
    }
    let runtime = workspace.runtime(&[]);
    for mcp in [false, true] {
        for budget in [2500, 20000] {
            let rows=walk(&runtime,mcp,"localSearch",json!({"path":workspace.workspace,"matchString":["needle"],"contextLines":{"before":0,"after":0}}),budget,|value| {
                let mut rows=Vec::new();
                for result in value["results"].as_array().expect("results") {
                    for file in result["data"]["files"].as_array().into_iter().flatten() {
                        for matched in file["matches"].as_array().expect("matches") {
                            let owner=matched.get("enclosing").unwrap_or_else(||panic!("each hit retains enclosing evidence: {matched}, {file}"));
                            assert!(owner["symbolName"].as_str().expect("name").starts_with("holder_"));
                            let line=matched["line"].as_u64().expect("line");
                            assert!(owner["line"].as_u64().expect("start")<=line&&owner["endLine"].as_u64().expect("end")>=line);
                            rows.push(format!("{}:{line}",file["path"].as_str().expect("path")));
                        }
                    }
                }
                rows
            }).await;
            assert_eq!(
                rows.into_iter().collect::<BTreeSet<_>>(),
                expected,
                "MCP {mcp}, budget {budget}"
            );
        }
    }
    runtime.close().await;
}

#[tokio::test]
async fn structure_walks_reach_every_file_once_in_listing_and_tree_views() {
    let workspace = Workspace::new();
    let mut expected = BTreeSet::new();
    for n in 0..900 {
        let path = format!("tree/d{:02}/módulo_\"{n:04}\"_文件.rs", n % 30);
        workspace.write(&path, "x\n");
        expected.insert(path);
    }
    let runtime = workspace.runtime(&[]);
    for mcp in [false, true] {
        for operation in ["files", "tree"] {
            let rows=walk(&runtime,mcp,"structureSearch",json!({"operation":operation,"path":workspace.workspace,"include":["**/*.rs"],"maxDepth":4}),4000,|value| {
                let mut paths=Vec::new();
                for result in value["results"].as_array().expect("results") {
                    for group in result["data"]["files"].as_array().into_iter().flatten() {
                        for entry in group["files"].as_array().expect("entries") {
                            let entry=entry.as_str().expect("entry");
                            if entry.ends_with('/') { continue; }
                            let name=entry.strip_suffix(')').and_then(|entry|entry.rsplit_once(" (")).map_or(entry,|(name,_)|name);
                            paths.push(format!("{}/{name}",group["dir"].as_str().expect("dir")));
                        }
                    }
                }
                paths
            }).await;
            assert_eq!(
                rows.into_iter().collect::<BTreeSet<_>>(),
                expected,
                "MCP {mcp}, {operation}"
            );
        }
    }
    runtime.close().await;
}
