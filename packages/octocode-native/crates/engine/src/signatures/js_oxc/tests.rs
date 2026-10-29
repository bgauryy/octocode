use super::outline::symbol_kind_name;
use super::*;
use serde_json::Value;

/// JS/TS document symbols as a JSON `DocumentSymbol[]`; `None` for oversized
/// input, a hard parse failure, or a file with no top-level symbols.
fn extract_js_symbols(content: &str, file_path: &str) -> Option<String> {
    on_oxc_worker(content, file_path, |content, file_path| {
        with_js_program::<false, _>(content, file_path, |_, parser_ret| {
            let line_index = SpanPositions::new(content);
            let mut symbols = Vec::new();
            collect_program(&parser_ret.program, &line_index, &mut symbols);
            if symbols.is_empty() || job_cancelled() {
                return None;
            }
            serde_json::to_string(&symbols).ok()
        })
    })
}

pub(in crate::signatures) fn extract_graph_facts(content: &str, file_path: &str) -> Option<String> {
    extract_graph_facts_with_metadata(content, file_path)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

fn symbols(content: &str, path: &str) -> Value {
    let json = extract_js_symbols(content, path).expect("symbols expected");
    serde_json::from_str(&json).expect("valid json")
}

fn graph(content: &str, path: &str) -> Value {
    let json = extract_graph_facts(content, path).expect("graph facts expected");
    serde_json::from_str(&json).expect("valid json")
}

fn names(value: &Value) -> Vec<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_string())
        .collect()
}

fn call_pairs(value: &Value) -> Vec<(String, String)> {
    value["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| {
            (
                call["caller"].as_str().unwrap().to_string(),
                call["callee"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn declaration<'v>(facts: &'v Value, name: &str) -> &'v Value {
    facts["declarations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == name && d.get("parent").is_none())
        .unwrap_or_else(|| panic!("declaration {name}: {facts}"))
}

/// `(kind, caller, callee, has callerId)` for every call fact of `kind`.
fn calls_of_kind(facts: &Value, kind: &str) -> Vec<(String, String, bool)> {
    facts["calls"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|call| call["kind"] == kind)
        .map(|call| {
            (
                call["caller"].as_str().unwrap().to_string(),
                call["callee"].as_str().unwrap().to_string(),
                call.get("callerId").is_some(),
            )
        })
        .collect()
}

/// `(relation, from declaration name, to, line)` for heritage edges.
fn heritage_edges(facts: &Value) -> Vec<(String, String, String, u64)> {
    let declarations = facts["declarations"].as_array().unwrap();
    facts["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|edge| matches!(edge["relation"].as_str(), Some("extends" | "implements")))
        .map(|edge| {
            assert_eq!(edge["source"], "oxc");
            assert_eq!(edge["resolution"], "syntax");
            let from = declarations
                .iter()
                .find(|declaration| declaration["id"] == edge["from"])
                .unwrap_or_else(|| panic!("from is a declaration id: {edge}"));
            (
                edge["relation"].as_str().unwrap().to_string(),
                from["name"].as_str().unwrap().to_string(),
                edge["to"].as_str().unwrap().to_string(),
                edge["line"].as_u64().unwrap(),
            )
        })
        .collect()
}

mod call_walk;
mod calls;
mod commonjs;
mod graph_occurrences;
mod module_facts;
mod outline;
