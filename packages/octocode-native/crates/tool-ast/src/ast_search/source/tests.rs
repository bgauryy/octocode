//! A source past the whole-read limit: every astSearch operation answers
//! with the shared large-source recovery and a lead the next tool serves,
//! and never points to a tool that would refuse the same file.
use octocode_contract_types::AstSearchQuery;
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::test_support::{Fixture, simple_policy};
use serde_json::{Value, json};

/// A sparse file of `len` bytes: sized past a limit without writing it.
fn sized(root: &Fixture, name: &str, len: u64) -> String {
    let path = root.write(name, "");
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .and_then(|file| file.set_len(len))
        .expect("sparse fixture");
    path.to_string_lossy().into_owned()
}

/// The row (an error row's hints and calls) one astSearch call returns.
fn run(root: &Fixture, row: Value) -> Value {
    let (paths, security) = simple_policy(&root.0);
    let query: AstSearchQuery = serde_json::from_value(row).expect("typed row");
    match super::super::execute_ast(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &query,
        &paths,
        &security,
        &NeverCancel,
        None,
    ) {
        Ok(row) => row,
        Err(error) => json!({
            "errorCode": error.code,
            "hints": error.hints,
            "calls": error.calls.map(|calls| *calls),
        }),
    }
}

fn past_whole_read() -> u64 {
    octocode_runtime_base::security::SourceLimits::REAL.whole_source_bytes() + 1
}

/// The tools a row's leads call: an error's `calls`, or a row's `related`.
fn lead_tools(row: &Value) -> Vec<String> {
    let calls = row["calls"]
        .as_object()
        .into_iter()
        .flat_map(|calls| calls.values());
    let related = row["related"].as_array().into_iter().flatten();
    calls
        .chain(related)
        .filter_map(|call| call["tool"].as_str().map(str::to_owned))
        .collect()
}

fn rows(root: &Fixture, path: &str) -> [Value; 3] {
    [
        run(
            root,
            json!({"operation":"match","path":path,"rule":{"pattern":"fn $N() {}"}}),
        ),
        run(root, json!({"operation":"symbols","path":path})),
        run(root, json!({"operation":"syntaxTree","path":path})),
    ]
}

#[test]
fn a_source_text_tools_serve_gets_the_large_source_hint_and_a_read() {
    let root = Fixture::new();
    let path = sized(&root, "big.rs", past_whole_read());
    for row in rows(&root, &path) {
        assert_eq!(row["errorCode"], "fileTooLarge", "{row}");
        let hints = row["hints"].to_string();
        assert!(
            hints.contains(octocode_runtime_base::response::rows::LARGE_SOURCE_HINT),
            "{row}"
        );
        assert_eq!(lead_tools(&row), ["localFetch"], "{row}");
    }
    let literal = run(
        &root,
        json!({"operation":"match","path":path,"rule":{"pattern":"fn alpha() {}"}}),
    );
    assert_eq!(lead_tools(&literal), ["localSearch"], "{literal}");
}

/// Past localSearch's line ceiling the text search would refuse too: no
/// lead or hint names it; the read windows still serve the file.
#[test]
fn a_source_past_the_text_search_ceiling_names_no_search() {
    let root = Fixture::new();
    let len = octocode_engine::search::text_search::DEFAULT_MAX_LINE_SEARCH_FILE_BYTES + 1;
    let path = sized(&root, "huge.rs", len);
    let literal = run(
        &root,
        json!({"operation":"match","path":path,"rule":{"pattern":"fn alpha() {}"}}),
    );
    for row in rows(&root, &path).iter().chain([&literal]) {
        assert!(!row["hints"].to_string().contains("localSearch"), "{row}");
        assert_eq!(lead_tools(row), ["localFetch"], "{row}");
    }
}

/// Past localFetch's streaming ceiling no local tool reads the file: the
/// row offers nothing that would refuse it.
#[test]
fn a_source_no_local_tool_reads_offers_no_lead() {
    let root = Fixture::new();
    let len = octocode_tools_shared::source_view::prepare::MAX_STREAM_SOURCE_BYTES + 1;
    let path = sized(&root, "vast.rs", len);
    for row in rows(&root, &path) {
        assert_eq!(row["errorCode"], "fileTooLarge", "{row}");
        assert!(lead_tools(&row).is_empty(), "{row}");
        let hints = row["hints"].to_string();
        assert!(
            !hints.contains("localSearch") && !hints.contains("localFetch"),
            "{row}"
        );
    }
}

/// A one-line file within the byte limit whose parse is estimated past the
/// memory budget is read as too large, as a source past the limit is; a
/// plain file of the same kind is read.
#[test]
fn a_dense_file_within_the_byte_limit_is_read_as_too_large_for_a_parse() {
    let root = Fixture::new();
    let budget = octocode_engine::structural::parse_budget();
    let calls = budget / octocode_engine::structural::BYTES_PER_TOKEN / 5 + 1000;
    let dense = root.write("min.js", format!("var a=[{}];\n", "f(1),".repeat(calls)));
    assert!(super::read_parse_source(&dense).expect("read").is_none());
    let plain = root.write("plain.js", "hit(1);\n");
    assert!(super::read_parse_source(&plain).expect("read").is_some());
}
