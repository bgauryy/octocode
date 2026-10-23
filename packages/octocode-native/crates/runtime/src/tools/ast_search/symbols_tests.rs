//! A directory symbols scan holds what its filters keep, not every file's
//! declarations: peak memory follows the answer, not the repository.
use super::symbols::{AstSearchQuerySymbols, Outline, outline_directory};
use crate::execution::cancel::NeverCancel;
use crate::tools::ast_search::symbols::ast_search_query_symbols;
use crate::tools::test_support::{Fixture, checks, simple_policy};
use octocode_contract_types::ErrorCode;
use serde_json::json;

mod filters;
mod leads;
mod scans;

fn query(root: &std::path::Path, extra: serde_json::Value) -> AstSearchQuerySymbols {
    let mut row = json!({"operation":"symbols","path":root.to_string_lossy()});
    row.as_object_mut()
        .expect("object")
        .extend(extra.as_object().expect("object").clone());
    serde_json::from_value(row).expect("symbols query")
}

fn scan(q: &AstSearchQuerySymbols, root: &std::path::Path) -> Outline {
    let (paths, _) = simple_policy(root);
    outline_directory(q, &root.canonicalize().expect("root"), &paths, &NeverCancel).expect("scan")
}

fn rows(outline: &Outline) -> usize {
    outline.files.iter().map(|file| file.rows.len()).sum()
}

fn files(root: &std::path::Path, count: usize) {
    for index in 0..count {
        let mut source = format!(
            "export function helper{index}() {{ return {index}; }}\nexport class Widget{index} {{}}\n"
        );
        if index == 3 {
            source.push_str("export class Disposable {}\n");
        }
        std::fs::write(root.join(format!("file{index}.ts")), source).expect("source");
    }
}

fn empty_outline(root: &std::path::Path, extra: serde_json::Value) -> serde_json::Value {
    let (paths, security) = simple_policy(root);
    let query: octocode_contract_types::AstSearchQuery = {
        let mut row = json!({"operation":"symbols","path":root.to_string_lossy()});
        row.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        serde_json::from_value(row).expect("typed row")
    };
    super::execute_ast(&query, &paths, &security, &NeverCancel, None).expect("runs")
}

/// The astSearch row of a result's `includeSkipped` lead: the same scope
/// with the walk flags that reach the skipped files.
fn skipped_lead(out: &serde_json::Value) -> serde_json::Value {
    let lead = out["next"]["includeSkipped"]["query"]["queries"][0].clone();
    assert!(lead.is_object(), "no includeSkipped lead: {out}");
    lead
}

fn run_lead(root: &std::path::Path, lead: serde_json::Value) -> serde_json::Value {
    let (paths, security) = simple_policy(root);
    let query: octocode_contract_types::AstSearchQuery =
        serde_json::from_value(lead).expect("the lead is a typed row");
    super::execute_ast(&query, &paths, &security, &NeverCancel, None).expect("the lead runs")
}

/// Whether an outline lists a declaration named `name`.
fn declares(out: &serde_json::Value, name: &str) -> bool {
    out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| file["symbols"].as_array().into_iter().flatten())
        .any(|symbol| symbol["symbolName"] == name)
}

fn found_files(out: &serde_json::Value) -> Vec<String> {
    out["files"]
        .as_array()
        .map(|files| {
            files
                .iter()
                .filter_map(|file| file["path"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}
