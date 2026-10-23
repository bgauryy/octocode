//! Flow edges replay verbatim: every `next.*` page and `hints.*` lead a
//! local tool emits is a complete fresh or saved tool input, passes the
//! strict input contract of the tool it names, uses only current field and
//! lead names, and runs unchanged as the whole call. The seeds
//! below reach the local edges whose fields the schema alignment changed;
//! the GitHub edges are replayed the same way by their provider tests.

use crate::support::{self, Run as _};

use octocode_native::contracts::validate;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use support::Workspace;

/// One emitted continuation: the tool that emitted it, its name, and the call.
#[derive(Debug)]
struct Edge {
    from: String,
    name: String,
    tool: String,
    query: Value,
}

/// Every `next` and `hints` lead of `value` (the response-paging call is
/// replayed on its own), as edges from `from`.
fn collect(from: &str, value: &Value, out: &mut Vec<Edge>) {
    out.extend(
        support::checks::leads(value)
            .into_iter()
            .filter(|lead| lead.channel != "responsePagination")
            .map(|lead| Edge {
                from: from.to_owned(),
                name: lead.name,
                tool: lead.tool,
                query: lead.query,
            }),
    );
}

fn assert_current(edge: &Edge) {
    validate(&edge.tool, edge.query.clone()).unwrap_or_else(|error| {
        panic!(
            "{}.{} → {} is not a complete input under the strict contract: {error:?}\n{edge:?}",
            edge.from, edge.name, edge.tool
        )
    });
}

/// Runs a continuation exactly as emitted: no envelope, no brief added.
async fn replay(
    runtime: &octocode_native::runtime::ToolRuntime,
    tool: &str,
    input: Value,
) -> Value {
    let outcome = runtime
        .run("replay".into(), tool.into(), input.clone())
        .await
        .unwrap_or_else(|error| panic!("{tool} {input}: {error:?}"));
    checked(tool, &input, outcome.structured_content.into_value())
}

fn checked(tool: &str, query: &Value, structured: Value) -> Value {
    for row in structured["results"].as_array().into_iter().flatten() {
        let code = row["data"]["errorCode"].as_str().unwrap_or_default();
        assert!(
            !matches!(code, "outputContractViolation" | "invalidInput"),
            "{tool} {query}: {row}"
        );
    }
    structured
}

fn fixture(workspace: &Workspace) {
    let mut lines = String::from("pub fn helper(value: u32) -> u32 {\n    value + 1\n}\n\n");
    for index in 0..40 {
        lines.push_str(&format!(
            "pub fn caller{index}() -> u32 {{\n    helper({index})\n}}\n\n"
        ));
    }
    workspace.write("src/lib.rs", lines);
    workspace.write(
        "src/long.txt",
        format!("start\n{}\nend\n", "needle ".repeat(400)),
    );
    workspace.write("src/store", b"SQLite format 3\0needle");
    for index in 0..12 {
        workspace.write(&format!("src/mod{index}.rs"), "pub fn helper_twin() {}\n");
    }
}

#[tokio::test]
async fn every_emitted_edge_is_current_strict_and_replays_verbatim() {
    let workspace = Workspace::new();
    fixture(&workspace);
    let runtime = workspace.runtime(&[("OCTOCODE_BETA", "true".into())]);
    let root = workspace.workspace.clone();
    let src = root.join("src");
    let lib = src.join("lib.rs");
    let seeds: Vec<(&str, Value)> = vec![
        // localSearch: nextPage, read, findCallers, matchString leads.
        (
            "localSearch",
            json!({"path": src, "matchString": "helper", "pageSize": 1}),
        ),
        (
            "localSearch",
            json!({"path": lib, "matchString": "helper", "matchPageSize": 1}),
        ),
        // localSearch: a binary file → binarySkipped (structureSearch nameRegex).
        ("localSearch", json!({"path": src, "matchString": "needle"})),
        // localSearch: an invalid regex → repair.
        (
            "localSearch",
            json!({"path": src, "matchString": "helper(", "regex": "rust"}),
        ),
        // structureSearch: read and continueWalk.
        (
            "structureSearch",
            json!({"operation": "files", "path": src, "include": ["lib.rs"]}),
        ),
        (
            "structureSearch",
            json!({"operation": "files", "path": src, "maxEntries": 3}),
        ),
        (
            "structureSearch",
            json!({"operation": "tree", "path": root, "maxDepth": 1, "pageSize": 1}),
        ),
        // astSearch: syntaxTree paging, match paging, symbols.
        (
            "astSearch",
            json!({"operation": "syntaxTree", "path": lib, "pageSize": 5}),
        ),
        (
            "astSearch",
            json!({"operation": "match", "path": lib, "pattern": "helper($A)", "matchPageSize": 2}),
        ),
        (
            "astSearch",
            json!({"operation": "symbols", "path": lib, "symbolName": "helper"}),
        ),
        // localFetch: continueWindow, matchString, block, a missed match.
        (
            "localFetch",
            json!({"path": lib, "unit": "lines", "length": 10}),
        ),
        (
            "localFetch",
            json!({"path": lib, "matchString": "caller3", "contextLines": 1}),
        ),
        (
            "localFetch",
            json!({"path": lib, "matchString": "no_such_text_anywhere"}),
        ),
        (
            "localFetch",
            json!({"path": lib, "ranges": ["5-6"], "block": true}),
        ),
        // Envelope window paging (responseScope:"structured"): responsePagination.next.
        (
            "localFetch",
            json!({"path": lib, "fullContent": true, "responseLength": 400, "responseScope": "structured"}),
        ),
    ];
    let mut edges = Vec::new();
    for (tool, query) in seeds {
        let output = checked(
            tool,
            &query,
            support::run(&runtime, tool, query.clone(), &support::ALL_EXTRAS)
                .await
                .outcome
                .structured_content
                .into_value(),
        );
        collect(tool, &output, &mut edges);
        if let Some(next) = output.pointer("/responsePagination/next") {
            let query = next["query"].clone();
            support::assert_cursor_only(&query);
            let replay = runtime
                .run("replay".into(), tool.into(), query)
                .await
                .expect("response page replays");
            assert!(replay.structured_content["results"].is_array());
            edges.push(Edge {
                from: tool.into(),
                name: "responsePagination.next".into(),
                tool: tool.into(),
                query: next["query"].clone(),
            });
        }
    }
    let mut seen = BTreeSet::new();
    for edge in &edges {
        assert_current(edge);
        seen.insert(format!("{}.{}", edge.from, edge.name));
    }
    // Replay each distinct (from, name) edge once, verbatim.
    let mut replayed = BTreeSet::new();
    for edge in &edges {
        if edge.tool == "clasify" || !replayed.insert(format!("{}.{}", edge.from, edge.name)) {
            continue;
        }
        let output = replay(&runtime, &edge.tool, edge.query.clone()).await;
        let mut second = Vec::new();
        collect(&edge.tool, &output, &mut second);
        second.iter().for_each(assert_current);
    }
    for expected in [
        "localSearch.nextPage",
        "localSearch.findCallers",
        "localSearch.binarySkipped",
        "structureSearch.read",
        "structureSearch.continueWalk",
        "astSearch.nextPage",
        "astSearch.nextMatchPage",
        "localFetch.continueWindow",
        "localFetch.responsePagination.next",
    ] {
        assert!(
            seen.contains(expected),
            "{expected} not reached; saw {seen:?}"
        );
    }
}
