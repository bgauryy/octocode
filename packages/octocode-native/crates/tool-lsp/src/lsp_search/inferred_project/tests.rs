use super::*;
use octocode_test_support::json_fixture;
use serde_json::json;

pub(super) fn refs_query(uri: &str) -> LspSearchQuery {
    serde_json::from_value(json!({
        "operation":"references","path":uri,
        "symbolName":"greet","line":1
    }))
    .expect("references query")
}

pub(super) fn refs_row() -> Value {
    json_fixture(r#"{"operation": "references", "matches": [], "coverage": {}}"#)
}

#[test]
fn inferred_ts_project_marks_references_partial_with_text_fallback() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    std::fs::create_dir_all(&src).expect("src");
    let file = src.join("util.ts");
    std::fs::write(&file, "export function greet() {}\n").expect("write");
    // A tsconfig above the temp dir would legitimately suppress the flag.
    if typescript_project_dir(dir.path()).is_some() {
        return;
    }
    let query = refs_query(&format!("file://{}", file.display()));
    let mut row = refs_row();
    let root = dir.path().to_string_lossy().into_owned();
    annotate(
        &mut row,
        &query,
        Some("typescript"),
        &file.to_string_lossy(),
        &Scope::new(root.clone(), Vec::new()),
    );
    assert_eq!(row["isPartial"], true, "{row}");
    // The reason is stated once, in partialReasons.
    assert_eq!(row["partialReasons"], json!([REASON]));
    assert!(row.pointer("/coverage/reason").is_none(), "{row}");
    // The code is the whole statement: the tsconfig.json repair is the
    // reason's meaning, said once by core.
    assert!(row.get("warnings").is_none(), "{row}");
    octocode_runtime_base::test_support::checks::assert_no_reason_meaning(&row);
    let next = row["related"]
        .as_array()
        .unwrap()
        .iter()
        .find(|call| call["tool"] == "localSearch")
        .expect("text fallback");
    assert_eq!(next["kind"], "related");
    assert_eq!(next["input"]["queries"][0]["path"], root.as_str());
    assert_eq!(
        next["input"]["queries"][0]["matchString"],
        json!([octocode_engine::text::word_pattern("greet")])
    );
    octocode_runtime_base::contracts::validate("localSearch", next["input"].clone())
        .expect("complete fallback");

    // With a tsconfig at the workspace root the server loads the project.
    std::fs::write(dir.path().join("tsconfig.json"), "{}").expect("tsconfig");
    let mut row = refs_row();
    annotate(
        &mut row,
        &query,
        Some("typescript"),
        &file.to_string_lossy(),
        &Scope::new(root.clone(), Vec::new()),
    );
    assert_eq!(row, refs_row());
}

#[test]
fn non_ts_languages_and_outgoing_operations_are_untouched() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("lib.rs");
    std::fs::write(&file, "fn greet() {}\n").expect("write");
    let root = dir.path().to_string_lossy().into_owned();
    let query = refs_query(&format!("file://{}", file.display()));
    let mut row = refs_row();
    annotate(
        &mut row,
        &query,
        Some("rust"),
        &file.to_string_lossy(),
        &Scope::new(root.clone(), Vec::new()),
    );
    assert_eq!(row, refs_row());

    let definition: LspSearchQuery = serde_json::from_value(json!({
        "operation":"definition",
        "path":format!("file://{}", file.display()),"symbolName":"greet","line":1
    }))
    .expect("definition query");
    let mut row = refs_row();
    annotate(
        &mut row,
        &definition,
        Some("typescript"),
        &file.to_string_lossy(),
        &Scope::new(root.clone(), Vec::new()),
    );
    assert_eq!(row, refs_row());
}
