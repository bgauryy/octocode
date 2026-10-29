//! Linking across languages, asked the way a caller asks: `graph ingest`
//! a fixture, then `graph query` its edges, cycles and import tallies.
use super::test_support::*;
use crate::tools::test_support::Fixture;
use serde_json::{Value, json};
use std::path::Path;

mod c_family;
mod commonjs;
mod cpp;
mod csharp;
mod dead_code;
mod families;
mod go;
mod imports;
mod java;
mod languages;
mod python;
mod rust;
mod rust_idioms;
mod scala;
mod scala_idioms;
mod scala_scope;
mod typescript;

/// Ingests `root` (as its own workspace) and returns it.
fn ingested(temp: &Fixture) -> &Path {
    let root = temp.0.as_path();
    let out = ingest(root, root);
    assert_eq!(out.value.get("error"), None, "{}", out.value);
    root
}

/// Files (not packages or symbols) of a traversal answer.
fn files(out: &Value) -> Vec<&str> {
    result_ids(out)
        .into_iter()
        .filter(|id| !id.starts_with("pkg:") && !id.contains('#'))
        .collect()
}

/// `(id, via, confidence)` of every call edge an `op` (`callees` or
/// `callers`) reaches from `node`, at any confidence, sorted.
fn call_rows(root: &Path, op: &str, node: &str) -> Vec<(String, String, String)> {
    let out = ask_with(root, op, Some(node), |o| {
        o.confidence = Some("low".into());
    });
    assert_eq!(out.get("error"), None, "{out}");
    let mut rows = out["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or_default().to_owned();
            (text("id"), text("via"), text("confidence"))
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

/// `(callee, via, confidence)` of every call edge out of `caller`.
fn call_edges(root: &Path, caller: &str) -> Vec<(String, String, String)> {
    call_rows(root, "callees", caller)
}

/// `(caller, via, confidence)` of every call edge into `target`.
fn caller_edges(root: &Path, target: &str) -> Vec<(String, String, String)> {
    call_rows(root, "callers", target)
}

fn edge(id: &str, via: &str, confidence: &str) -> (String, String, String) {
    (id.to_owned(), via.to_owned(), confidence.to_owned())
}

fn unresolved_internal(root: &Path) -> Value {
    ask(root, "stats", None)["imports"]["unresolvedInternal"].clone()
}

/// `(callee, via)` of every call edge out of `caller`.
fn callees(root: &Path, caller: &str) -> Vec<(String, String)> {
    call_edges(root, caller)
        .into_iter()
        .map(|(id, via, _)| (id, via))
        .collect()
}

fn to(target: &str, via: &str) -> (String, String) {
    (target.to_owned(), via.to_owned())
}
