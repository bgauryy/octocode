//! A compact or symbols read of a source whose parse is estimated past the
//! memory budget: neither view lexes or parses it. The compact read serves
//! the content view and names the reason; the symbols read names it with
//! the related read.
use super::super::*;
use octocode_contract_types::{ErrorCode, LocalFetchQuery};
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::security::ContentSecurity;
use octocode_test_support::json_fixture;
use serde_json::json;
use std::path::Path;

/// A one-line minified script just past the parse budget, and the path it
/// is read at.
fn dense() -> (String, &'static str) {
    let budget = octocode_engine::structural::parse_budget();
    let calls = budget / octocode_engine::structural::BYTES_PER_TOKEN / 5 + 1000;
    (
        format!("var a=[{}eval(zzz)];\n", "f(1),".repeat(calls)),
        "/fixture/dense.js",
    )
}

fn served(source: &str, path: &str, view: &str) -> serde_json::Value {
    let query: LocalFetchQuery =
        serde_json::from_value(json!({"path": path, "view": view})).expect("query");
    let row = process_fetched_content(
        &query,
        source.as_bytes(),
        Path::new(path),
        &SourceFacts::default(),
        &ContentSecurity::new(),
        &NeverCancel,
    );
    serde_json::to_value(&row).expect("serialize")
}

#[test]
fn a_compact_read_too_large_to_lex_serves_the_content_view_and_says_why() {
    let (source, path) = dense();
    let row = served(&source, path, "compact");
    assert_eq!(
        row["viewFallback"],
        json_fixture(
            r#"{
                "requested": "compact",
                "applied": "content",
                "reason": "too-large"
            }"#
        ),
        "{:.600}",
        row.to_string()
    );
    assert_eq!(
        row["content"],
        served(&source, path, "content")["content"],
        "the content view's page is served"
    );
}

#[test]
fn a_symbols_read_too_large_to_parse_names_the_reason_and_the_read() {
    let (source, path) = dense();
    let row = served(&source, path, "symbols");
    assert_eq!(
        row["errorCode"],
        json!(ErrorCode::SymbolsUnsupported),
        "{:.600}",
        row.to_string()
    );
    assert!(
        row["error"]
            .as_str()
            .is_some_and(|error| error.contains("too-large") && !error.contains("not-smaller")),
        "{:.600}",
        row.to_string()
    );
    assert!(
        row["related"].as_array().is_some_and(|calls| calls
            .iter()
            .any(|call| call.to_string().contains("dense.js"))),
        "a read of its lines is offered: {:.600}",
        row.to_string()
    );
}

/// A plain source within the budget is still compacted and outlined.
#[test]
fn a_source_within_the_budget_is_still_compacted_and_outlined() {
    let source = "// note\nfunction run() {\n  return 1;\n}\n".repeat(40);
    let row = served(&source, "/fixture/plain.js", "compact");
    assert_eq!(row["view"], json!("compact"), "{:.600}", row.to_string());
    let row = served(&source, "/fixture/plain.js", "symbols");
    assert_eq!(row["view"], json!("symbols"), "{:.600}", row.to_string());
}
