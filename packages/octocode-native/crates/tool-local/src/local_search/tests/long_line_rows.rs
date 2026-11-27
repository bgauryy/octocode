//! S4a: a hit in a line past `LONG_LINE_BYTES` is a window of that line: the row
//! states its column, and its value is the line's own text there. It offers
//! the complete line as a separate read.
use super::*;
use octocode_engine::security::LONG_LINE_BYTES;
use octocode_test_support::json_fixture;

#[test]
fn a_hit_in_a_line_past_a_whole_read_is_a_window_with_its_column() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join("src")).expect("dir");
    let before = "z".repeat(LONG_LINE_BYTES / 2);
    let line = format!("{before} NEEDLE {}", "z".repeat(LONG_LINE_BYTES / 2 + 2));
    fs::write(root.path().join("src/one.txt"), &line).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        json_fixture(r#"{"path": "src", "matchString": ["NEEDLE"]}"#),
        None,
    );
    let row = search_in(&request, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("row");
    let hit = &row["files"][0]["matches"][0];
    let value = hit["value"].as_str().expect("a value");
    assert!(
        value.len() < line.len(),
        "a window of the line: {}",
        value.len()
    );
    let start = line.find(value).expect("the value is the line's own text");
    assert!(start > 0, "the window starts inside the line");
    assert_eq!(
        hit["column"].as_u64(),
        Some(before.len() as u64 + 2),
        "the hit's 1-based column"
    );
    assert!(row.get("hints").is_none(), "{row}");
    assert_eq!(related_calls(&row).len(), 1, "{row}");
}

/// Clipped lines get targeted reads even after response guidance filtering.
/// Multiple windows and multiple lines in one file share one file query.
#[test]
fn window_rows_offer_one_read_per_file_with_one_range_per_clipped_line() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join("src")).expect("dir");
    let filler = "z".repeat(octocode_tools_shared::source_view::pagination::MIN_BYTE_PAGE);
    let line = format!("{filler} NEEDLE {filler} NEEDLE {filler}");
    fs::write(
        root.path().join("src/one.txt"),
        format!("ordinary NEEDLE\n{line}\nseparator\n{line}\n"),
    )
    .expect("fixture");
    fs::write(root.path().join("src/two.txt"), &line).expect("fixture");
    fs::write(root.path().join("outside.txt"), &line).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        json_fixture(r#"{"path": "src", "matchString": ["NEEDLE"]}"#),
        None,
    );
    let row = search_in(&request, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("row");
    let hits = row["files"][0]["matches"].as_array().expect("hits");
    assert!(hits[0].get("column").is_none(), "{row}");
    assert!(
        hits.iter()
            .filter(|hit| hit["line"] == 2 && hit["column"].is_number())
            .count()
            > 1,
        "the fixture must give multiple windows on one line: {row}"
    );
    let mut output = serde_json::json!({"results": [{"index": 0, "data": row}]});
    octocode_runtime_base::response::workflow_lead::retain_guidance(
        octocode_runtime_base::id::ToolId::LocalSearch,
        &mut output,
        &serde_json::json!({"queries": [request]}),
    );
    let data = &output["results"][0]["data"];
    assert_eq!(
        data["related"].as_array().map(Vec::len),
        Some(1),
        "the file queries share one related call: {data}"
    );
    let read = &data["related"][0];
    assert_eq!(read["tool"], "localFetch", "{read}");
    assert_eq!(
        read["input"]["queries"],
        serde_json::json!([
            {"path":"src/one.txt","ranges":[{"startLine":2,"endLine":2},{"startLine":4,"endLine":4}]},
            {"path":"src/two.txt","ranges":[{"startLine":1,"endLine":1}]},
        ]),
        "{read}"
    );
}

/// A windowed hit is stated once: its row names the file and its column
/// marks the window. No warning restates the path or the rule the tool
/// description already states, whether the query's path is absolute or a
/// nested relative one.
#[test]
fn a_windowed_hit_names_its_path_once_on_its_row() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join("pkg/sub")).expect("dir");
    let line = format!(
        "{} NEEDLE {}",
        "z".repeat(LONG_LINE_BYTES / 2),
        "z".repeat(LONG_LINE_BYTES / 2 + 2)
    );
    fs::write(root.path().join("pkg/sub/long.txt"), &line).expect("fixture");
    let (policy, security) = policy_for(root.path());
    for path in [
        root.path().join("pkg").to_string_lossy().into_owned(),
        "pkg".to_owned(),
        "pkg/sub".to_owned(),
    ] {
        let request = ls_query(
            serde_json::json!({"path": path, "matchString": ["NEEDLE"]}),
            None,
        );
        let row = search_in(&request, &policy, &security, None)
            .expect("search")
            .to_row()
            .expect("row");
        let shown = row["files"][0]["path"].as_str().expect("row path");
        assert!(
            row["files"][0]["matches"][0]["column"].as_u64().is_some(),
            "{path}: the column marks the window: {row}"
        );
        let warnings = row["warnings"].to_string();
        assert!(
            !warnings.contains(shown) && !warnings.contains("long.txt"),
            "{path}: a warning restates the path: {row}"
        );
    }
}
