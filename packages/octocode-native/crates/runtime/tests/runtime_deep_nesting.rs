//! A source nested deeper than the parsers' worker stack holds is read, not a
//! process abort: a native stack overflow cannot be caught, so one such file
//! in a scanned repository would kill every caller of the server. Every tool
//! that parses a file answers it, and the graph says which lane read JS/TS.

use crate::support::{self, Workspace};
use serde_json::json;

/// 50,000 levels, past what oxc's recursion holds on a 64 MiB stack.
const LEVELS: usize = 50_000;

/// One file per lane: oxc (TypeScript, JavaScript) and the Python reader of
/// module-level values, which ran on a 2 MiB runtime worker.
const FILES: [&str; 3] = ["p50000.ts", "deep.js", "deep.py"];

fn deep_files(workspace: &Workspace) {
    let parens = format!(
        "export function keep() {{}}\nconst x = {}1{};\n",
        "(".repeat(LEVELS),
        ")".repeat(LEVELS)
    );
    let brackets = format!(
        "export function keep() {{}}\nconst y = {}1{};\n",
        "[".repeat(LEVELS),
        "]".repeat(LEVELS)
    );
    workspace.write("p50000.ts", parens);
    workspace.write("deep.js", brackets);
    let python = format!(
        "def keep():\n    pass\n\nx = {}1{}\n",
        "(".repeat(LEVELS * 2),
        ")".repeat(LEVELS * 2)
    );
    workspace.write("deep.py", python);
}

#[tokio::test]
async fn deeply_nested_sources_are_answered_by_every_parsing_tool() {
    let workspace = Workspace::new();
    deep_files(&workspace);
    let runtime = workspace.runtime(&[]);
    let root = &workspace.workspace;

    let search = support::run(
        &runtime,
        "localSearch",
        support::query_path(root, json!({"matchString": "keep"})),
    )
    .await;
    assert_eq!(search.status(), "success", "{}", search.structured_content);
    let found = search.structured_content.to_string();
    for file in FILES {
        assert!(found.contains(file), "{file}: {found}");
        let symbols = support::run(
            &runtime,
            "localFetch",
            support::query_path(&root.join(file), json!({"view": "symbols"})),
        )
        .await;
        assert!(
            symbols.structured_content.to_string().contains("keep"),
            "{file}: {}",
            symbols.structured_content
        );
        let declared = support::run(
            &runtime,
            "astSearch",
            support::query_path(&root.join(file), json!({"operation": "symbols"})),
        )
        .await;
        let text = declared.structured_content.to_string();
        assert!(text.contains("keep"), "{file}: {text}");
    }

    let graph = support::run(&runtime, "graphBuild", support::query_path(root, json!({}))).await;
    let text = graph.structured_content.to_string();
    assert_eq!(graph.status(), "success", "{text}");
    assert!(
        text.contains("parser-fallback"),
        "the graph names the lane: {text}"
    );
}
