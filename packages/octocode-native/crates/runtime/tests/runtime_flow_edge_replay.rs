// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! Flow edges replay verbatim: every `next.*` page and `hints.*` lead a
//! local tool emits is a complete tool input (`{queries:[row]}`), passes the
//! strict input contract of the tool it names, uses only current field and
//! lead names, and runs unchanged as the whole call. The seeds
//! below reach the local edges whose fields the schema alignment changed;
//! the GitHub edges are replayed the same way by their provider tests.

use crate::support;

use octocode_native::contracts::{parsed_contract, validate};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use support::{Workspace, call};

/// Names retired from `scope` (`input`: a row field; `lead`: a
/// continuation name) for `tool`, from the shared D1 deny-list.
fn retired(scope: &str, tool: &str) -> Vec<String> {
    support::retired_names(&[scope], Some(tool))
        .into_iter()
        .map(|entry| entry["name"].as_str().expect("name").to_owned())
        .collect()
}

/// One emitted continuation: the tool that emitted it, its name, and the call.
#[derive(Debug)]
struct Edge {
    from: String,
    name: String,
    tool: String,
    query: Value,
}

/// Every `{tool, query}` under a `next` or `hints` key, never inside a call.
fn collect(from: &str, value: &Value, out: &mut Vec<Edge>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| collect(from, item, out)),
        Value::Object(map) => {
            for (key, child) in map {
                if key == "next" || key == "hints" {
                    if let Some(calls) = child.as_object() {
                        for (name, call) in calls {
                            if let (Some(tool), Some(query)) =
                                (call.get("tool").and_then(Value::as_str), call.get("query"))
                            {
                                out.push(Edge {
                                    from: from.to_owned(),
                                    name: name.clone(),
                                    tool: tool.to_owned(),
                                    query: query.clone(),
                                });
                            }
                        }
                    }
                } else if key != "query" {
                    collect(from, child, out);
                }
            }
        }
        _ => {}
    }
}

/// The rows of a continuation's complete input.
fn rows(query: &Value) -> Vec<Value> {
    query
        .get("queries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn assert_current(edge: &Edge) {
    assert!(
        !retired("lead", &edge.from).contains(&edge.name),
        "{} emits retired continuation {}: {edge:?}",
        edge.from,
        edge.name
    );
    assert!(
        edge.query["queries"].is_array(),
        "{}.{} is not a complete input: {edge:?}",
        edge.from,
        edge.name
    );
    validate(&edge.tool, edge.query.clone()).unwrap_or_else(|error| {
        panic!(
            "{}.{} → {} fails the strict contract: {error:?}\n{edge:?}",
            edge.from, edge.name, edge.tool
        )
    });
    for row in rows(&edge.query) {
        let Some(object) = row.as_object() else {
            continue;
        };
        for field in retired("input", &edge.tool) {
            assert!(
                !object.contains_key(&field),
                "{}.{} → {} sets retired field {field}: {edge:?}",
                edge.from,
                edge.name,
                edge.tool
            );
        }
        // astTopology's anchor is `source`; `file` was its old name.
        if edge.tool == "astTopology" {
            assert!(!object.contains_key("file"), "{edge:?}");
        }
    }
}

async fn run(runtime: &octocode_native::runtime::ToolRuntime, tool: &str, query: Value) -> Value {
    let outcome = call(runtime, tool, query.clone())
        .await
        .unwrap_or_else(|error| panic!("{tool} {query}: {error:?}"));
    checked(tool, &query, outcome.structured_content)
}

/// Runs a continuation exactly as emitted: no envelope, no brief added.
async fn replay(
    runtime: &octocode_native::runtime::ToolRuntime,
    tool: &str,
    input: Value,
) -> Value {
    let outcome = runtime
        .execute("replay".into(), tool.into(), input.clone())
        .await
        .unwrap_or_else(|error| panic!("{tool} {input}: {error:?}"));
    checked(tool, &input, outcome.structured_content)
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
    workspace.write(
        "web/a.ts",
        "import { b } from './b';\nimport { gone } from './gone';\nexport const a = b + gone;\n",
    );
    workspace.write("web/b.ts", "export const b = 1;\n");
    workspace.write(
        "web/c.ts",
        "import { a } from './a';\nimport { x } from './missing';\nexport const c = a + x;\n",
    );
}

#[tokio::test]
async fn every_emitted_edge_is_current_strict_and_replays_verbatim() {
    let workspace = Workspace::new();
    fixture(&workspace);
    let runtime = workspace.runtime(&[("OCTOCODE_BETA", "true".into())]);
    let root = workspace.workspace.clone();
    let src = root.join("src");
    let web = root.join("web");
    let lib = src.join("lib.rs");
    let seeds: Vec<(&str, Value)> = vec![
        // localSearch: nextPage, read, callers, matchString leads.
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
        // structureSearch: read and expandScan.
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
        // astTopology: diagnostics paging, a suffix miss, a capped scan.
        (
            "astTopology",
            json!({"operation": "dependents", "path": web, "source": "b.ts"}),
        ),
        (
            "astTopology",
            json!({"operation": "dependencies", "path": root, "source": "a.ts"}),
        ),
        (
            "astTopology",
            json!({"operation": "dependencies", "path": web, "source": "a.ts", "maxFiles": 1}),
        ),
        // localFetch: continue, matchString, block, a missed match.
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
        // Response paging: responsePagination.next.
        (
            "localFetch",
            json!({"path": lib, "fullContent": true, "responseLength": 400}),
        ),
    ];
    let mut edges = Vec::new();
    for (tool, query) in seeds {
        let output = run(&runtime, tool, query).await;
        collect(tool, &output, &mut edges);
        if let Some(next) = output.pointer("/responsePagination/next") {
            let query = next["query"].clone();
            assert!(query.get("responseOffset").is_some(), "{next}");
            for field in ["responseCharOffset", "responseCharLength"] {
                assert!(query.get(field).is_none(), "{next}");
            }
            let replay = runtime
                .execute("replay".into(), tool.into(), query)
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
        "localSearch.callers",
        "localSearch.binarySkipped",
        "structureSearch.read",
        "structureSearch.expandScan",
        "astSearch.nextPage",
        "astSearch.nextMatchPage",
        "astTopology.retrySuffixMatch",
        "localFetch.continue",
    ] {
        assert!(
            seen.contains(expected),
            "{expected} not reached; saw {seen:?}"
        );
    }
}

/// Every registered continuation kind is a current name.
#[test]
fn every_registered_continuation_kind_is_current() {
    let contract = parsed_contract().expect("contract");
    let kinds = &contract["continuationChannels"]["kinds"];
    let mut names = Vec::new();
    for channel in ["pages", "leads"] {
        names.extend(
            kinds[channel]
                .as_array()
                .unwrap_or_else(|| panic!("{channel} kinds"))
                .iter()
                .filter_map(Value::as_str),
        );
    }
    assert!(!names.is_empty());
    let retired = support::retired_names(&["lead"], None);
    for name in names {
        assert!(
            !retired.iter().any(|entry| entry["name"] == name),
            "retired kind {name} is registered"
        );
    }
}
