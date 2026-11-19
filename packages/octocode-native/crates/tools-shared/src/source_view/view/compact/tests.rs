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

/// A script just past the parse budget, one short call per line (minified
/// text is refused before any parse estimate: see
/// `minified_text_gets_neither_view`), and the path it is read at.
fn dense() -> (String, &'static str) {
    let budget = octocode_engine::structural::parse_budget();
    let calls = budget / octocode_engine::structural::BYTES_PER_TOKEN / 5 + 1000;
    (
        format!("var a=[{}eval(zzz)];\n", "f(1),\n".repeat(calls)),
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

/// Minified text gets neither a compact view nor an outline, and is not
/// parsed for one: the row is a short notice that routes to matchString,
/// which still serves each match with its window. A source with one long
/// line among many short ones is still outlined.
#[test]
fn minified_text_gets_neither_view() {
    let bundle = format!(
        "!function(){{{}}}();\n",
        (0..4_000)
            .map(|at| format!("var a{at}=b{at}(c,d);"))
            .collect::<String>()
    );
    let path = "/fixture/bundle.min.js";
    for view in ["compact", "symbols"] {
        let row = served(&bundle, path, view);
        assert_eq!(
            row["errorCode"],
            json!(ErrorCode::LargeSourceWindowOnly),
            "{view}: {:.600}",
            row.to_string()
        );
        let error = row["error"].as_str().expect("error");
        assert!(
            error.contains("Minified") && error.contains("matchString"),
            "{view}: {error}"
        );
        assert!(
            row.get("content").is_none(),
            "{view}: {:.300}",
            row.to_string()
        );
        assert!(
            row.to_string().len() < 1_000,
            "{view}: {}",
            row.to_string().len()
        );
    }
    let query: LocalFetchQuery =
        serde_json::from_value(json!({"path": path, "matchString": ["a3999=b3999"]}))
            .expect("query");
    let row = serde_json::to_value(process_fetched_content(
        &query,
        bundle.as_bytes(),
        Path::new(path),
        &SourceFacts::default(),
        &ContentSecurity::new(),
        &NeverCancel,
    ))
    .expect("serialize");
    assert!(
        row["content"]
            .as_str()
            .is_some_and(|content| content.contains("a3999=b3999")),
        "{:.600}",
        row.to_string()
    );
    let mut code = "function run() {\n  return 1;\n}\n".repeat(400);
    code.push_str(&format!("const blob = \"{}\";\n", "x".repeat(20_000)));
    let row = served(&code, "/fixture/one_long_line.js", "symbols");
    assert_eq!(row["view"], "symbols", "{:.600}", row.to_string());
}
