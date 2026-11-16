//! The default order is the walk order: files as walked, rows as written,
//! whatever their text or path says.

use super::*;

fn walked_paths(files: &[(&str, &str)], fields: serde_json::Value) -> Vec<String> {
    let body = search_fixture(files, ls_query(fields, None));
    body["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| f["path"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn the_default_order_is_the_walk_order() {
    // A test file before a source file, a generated name, a declaration
    // in the last file: nothing is demoted or promoted.
    let files = [
        ("src/z.rs", "pub fn config() {}\n"),
        ("a_test.rs", "let c = config();\nlet d = config();\n"),
        ("gen/a.generated.rs", "config();\nconfig();\nconfig();\n"),
        ("b/dist/out.js", "config();\n"),
    ];
    let paths = walked_paths(&files, serde_json::json!({"matchString": ["config"]}));
    let mut sorted = paths.clone();
    sorted.sort_by(|a, b| std::path::Path::new(a).cmp(std::path::Path::new(b)));
    assert_eq!(paths, sorted, "{paths:?}");
    assert_eq!(paths.first().map(String::as_str), Some("a_test.rs"));
}

#[test]
fn a_files_rows_are_in_line_order_whatever_their_text() {
    let mut text = String::from("// config in a comment\nlet label = \"config\";\n");
    for _ in 0..40 {
        text.push_str("filler();\n");
    }
    text.push_str("pub fn config() {}\nreturn config();\n");
    let body = search_fixture(
        &[("only.rs", text.as_str())],
        ls_query(serde_json::json!({"matchString": ["config"]}), None),
    );
    let lines = body["files"][0]["matches"]
        .as_array()
        .expect("matches")
        .iter()
        .map(|row| row["line"].as_u64().expect("line"))
        .collect::<Vec<_>>();
    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(lines, sorted, "rows follow the file, not a rank");
    assert_eq!(lines.first(), Some(&1), "the comment hit comes first");
}
