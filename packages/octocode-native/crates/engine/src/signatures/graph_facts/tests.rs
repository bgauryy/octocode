use super::references::inline_format_captures;
use super::*;
use crate::text::extension_of;
use serde_json::Value;

pub(super) fn extract_graph_facts(content: &str, file_path: &str) -> Option<String> {
    extract_graph_facts_with_metadata(content, file_path)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

fn extract_graph_facts_with_metadata(
    content: &str,
    file_path: &str,
) -> Option<crate::signatures::GraphFactsExtraction> {
    let extension = extension_of(file_path, true, "txt");
    extract_graph_facts_with_metadata_with_extension(content, file_path, &extension)
}

fn extract_graph_facts_before(
    content: &str,
    file_path: &str,
    deadline: std::time::Instant,
) -> Option<String> {
    let extension = extension_of(file_path, true, "txt");
    extract_graph_facts_with_metadata_before(content, file_path, &extension, deadline)
        .and_then(|extraction| serde_json::to_string(&extraction.facts).ok())
}

/// Reference count of the first declaration named `name`.
fn reference_count(source: &str, path: &str, name: &str) -> u32 {
    let extraction = extract_graph_facts_with_metadata(source, path).expect("graph facts");
    let id = &extraction
        .facts
        .declarations
        .iter()
        .find(|declaration| declaration.name == name)
        .unwrap_or_else(|| panic!("declaration {name}"))
        .id;
    extraction
        .reference_counts
        .iter()
        .find(|count| &count.declaration_id == id)
        .expect("counted")
        .count
}

fn facts(src: &str, path: &str) -> Value {
    let raw = extract_graph_facts(src, path).expect("graph facts expected");
    serde_json::from_str(&raw).expect("valid graph json")
}

/// Facts under a wall-clock deadline that a loaded machine cannot reach, for
/// the complexity tests: they bound the work by thread CPU time instead
/// (`crate::test_cpu`), so scheduler delay neither cuts the walk short nor
/// fails the bound. Return typed facts so callers measure complete extraction
/// before converting to JSON values for assertions. Ordinary facts tests still
/// exercise the serialized API.
fn facts_with_load_proof_deadline(src: &str, path: &str) -> GraphFactsDocument {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    let extension = extension_of(path, true, "txt");
    let extraction = extract_graph_facts_with_metadata_before(src, path, &extension, deadline)
        .expect("graph facts expected");
    extraction.facts
}

fn facts_json(source: &str, path: &str) -> Value {
    serde_json::from_str(&extract_graph_facts(source, path).expect("graph facts"))
        .expect("facts JSON")
}

fn declared_names(source: &str, path: &str) -> Vec<String> {
    facts_json(source, path)["declarations"]
        .as_array()
        .expect("declarations")
        .iter()
        .filter_map(|declaration| declaration["name"].as_str().map(str::to_owned))
        .collect()
}

/// `(relation, from declaration name, to, line)` for every heritage edge.
fn heritage(source: &str, path: &str) -> Vec<(String, String, String, u64)> {
    let facts = facts_json(source, path);
    let declarations = facts["declarations"].as_array().expect("declarations");
    facts["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .filter(|edge| matches!(edge["relation"].as_str(), Some("extends" | "implements")))
        .map(|edge| {
            assert_eq!(edge["source"], "tree-sitter");
            assert_eq!(edge["resolution"], "syntax");
            let from = declarations
                .iter()
                .find(|declaration| declaration["id"] == edge["from"])
                .unwrap_or_else(|| panic!("edge from is a declaration id: {edge}"));
            (
                edge["relation"].as_str().unwrap_or_default().to_owned(),
                from["name"].as_str().unwrap_or_default().to_owned(),
                edge["to"].as_str().unwrap_or_default().to_owned(),
                edge["line"].as_u64().unwrap_or_default(),
            )
        })
        .collect()
}

fn edge(relation: &str, from: &str, to: &str, line: u64) -> (String, String, String, u64) {
    (relation.to_owned(), from.to_owned(), to.to_owned(), line)
}

/// The code of each diagnostic of a facts document, in order.
fn codes(facts: &Value) -> Vec<String> {
    facts["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .map(|diagnostic| diagnostic["code"].as_str().unwrap().to_owned())
        .collect()
}

mod calls_and_diagnostics;
mod deadlines;
mod declarations;
mod go_facts;
mod heritage_edges;
mod language_facts;
mod references;
mod rust_macro_calls;
mod rust_modules;

mod test_discovery;
mod validity;

mod adapter_repairs;

mod adapter_visibility;

mod namespaces;

#[cfg(feature = "tree-sitter-scala")]
mod scala;
