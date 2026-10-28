//! One count vocabulary and one 1-based column base.

use crate::support::{self, Run as _, Workspace};
use octocode_native::runtime::ToolRuntime;
use serde_json::json;

fn beta_runtime(workspace: &Workspace) -> ToolRuntime {
    workspace.runtime(&[("OCTOCODE_BETA", "true".into())])
}

// ---------- one count vocabulary ----------

#[tokio::test]
async fn local_search_counts_are_match_line_and_file_counts() {
    let workspace = Workspace::new();
    for name in ["a.txt", "b.txt", "c.txt"] {
        workspace.write(name, "needle\nx\nneedle\n");
    }
    let runtime = workspace.runtime(&[]);
    let paged = support::run(
        &runtime,
        "localSearch",
        // The page counters are a stats extra.
        json!({"path":".","matchString":["needle"]}),
    )
    .await;
    // The totals are flat on the row (no `stats` object, no `fileCount`: the
    // file count is `pagination.totalItems`).
    let totals = paged.data();
    assert_eq!(totals["matchCount"], 6, "{}", paged.structured_content);
    assert_eq!(
        totals["matchedLineCount"], 6,
        "{}",
        paged.structured_content
    );
    assert!(
        totals.get("stats").is_none(),
        "{}",
        paged.structured_content
    );
    assert_eq!(totals["files"].as_array().expect("files").len(), 3);
    // A merged context window lists its matched lines as `matchedLines`.
    let merged = support::run(
        &runtime,
        "localSearch",
        json!({"path":"a.txt","matchString":["needle"],"contextLines":{"before":2,"after":2}}),
    )
    .await;
    let row = &merged.data()["files"][0]["matches"][0];
    assert_eq!(
        row["matchedLines"],
        json!([1, 3]),
        "{}",
        merged.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn ast_tools_count_matches_as_match_count() {
    let workspace = Workspace::new();
    workspace.write("src/a.ts", "export const f = (x) => old(x) + old(1);\n");
    let runtime = beta_runtime(&workspace);
    let search = support::run(
        &runtime,
        "astSearch",
        // A complete page drops its stats; the stats extra keeps them.
        json!({"operation":"match","path":"src","parser":"typescript","rule":{"pattern":"old($A)"}}))
    .await;
    assert_eq!(
        search.data()["matchCount"],
        2,
        "{}",
        search.structured_content
    );
    runtime.close().await;
}

// ---------- one 1-based column base ----------

/// Hit columns are a detail (`extras: ["details"]`); every one shown is
/// 1-based.
#[tokio::test]
async fn every_column_is_one_based() {
    let workspace = Workspace::new();
    // "a()" starts at the 4th character of line 1 and the 10th of line 2.
    workspace.write("x.rs", "fn a() {}\nfn b() { a(); }\n");
    let runtime = beta_runtime(&workspace);
    let spans = support::run(
        &runtime,
        "localSearch",
        json!({"path":"x.rs","matchString":["a()"],"view":"matchOnly"}),
    )
    .await;
    let rows = &spans.data()["files"][0]["matches"];
    assert_eq!(rows[0]["column"], 4, "{}", spans.structured_content);
    assert_eq!(rows[1]["column"], 10, "{}", spans.structured_content);
    let matched = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"match","path":"x.rs","rule":{"pattern":"a()"},"includeCaptures":true}),
    )
    .await;
    let row = &matched.data()["files"][0]["matches"][0];
    assert_eq!(row["line"], 2, "{}", matched.structured_content);
    assert_eq!(row["column"], 10, "{}", matched.structured_content);
    let tree = support::run(
        &runtime,
        "astSearch",
        json!({"operation":"syntaxTree","path":"x.rs"}),
    )
    .await;
    // `function_item` spans line 1 from character 1 to the end (exclusive 10).
    assert_eq!(
        tree.data()["nodes"][1]["node"],
        "1 function_item 1:1-1:10 ^0",
        "{}",
        tree.structured_content
    );
    runtime.close().await;
}

#[tokio::test]
async fn lsp_position_and_call_hierarchy_inputs_fail_closed() {
    let workspace = Workspace::new();
    workspace.write("a.ts", "export function f() {}\n");
    let runtime = workspace.runtime(&[]);
    for (row, names) in [
        (
            json!({"operation":"definition","path":"a.ts","position":{"line":0,"character":16}}),
            ["position", "symbolName"],
        ),
        (
            json!({"operation":"callHierarchy","path":"a.ts","symbolName":"f","line":1}),
            ["callHierarchy", "callers"],
        ),
    ] {
        let error = runtime
            .run(
                "d1-lsp-retired".into(),
                "lspSearch".into(),
                json!({"queries":[row]}),
            )
            .await
            .expect_err("a retired lspSearch input is rejected");
        let text = format!("{error:?}");
        for name in names {
            assert!(text.contains(name), "{name}: {text}");
        }
    }
    runtime.close().await;
}
