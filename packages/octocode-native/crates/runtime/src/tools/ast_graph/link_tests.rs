//! Linking across languages, asked the way a caller asks: `graph ingest`
//! a fixture, then `graph query` its edges, cycles and import tallies.
use super::test_support::*;
use crate::tools::test_support::Fixture;
use serde_json::{Value, json};
use std::path::Path;

mod dead_code;
mod imports;
mod languages;
mod rust;

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

fn unresolved_internal(root: &Path) -> Value {
    ask(root, "stats", None)["imports"]["unresolvedInternal"].clone()
}
