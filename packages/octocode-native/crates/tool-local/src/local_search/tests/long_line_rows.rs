//! S4a: a hit in a line past `LONG_LINE_BYTES` is a window of that line: the row
//! states its column and where the window starts, and its value is the line's
//! own text there. The row is evidence: it states no hint and offers no call.
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
    assert_evidence_row_guides_nothing(&row);
}

/// A window row says so on its row: its column marks the window, which no
/// warning restates; the tool description names the read of the whole line
/// (localFetch with the same matchString), so the response keeps no extra
/// lead for it on a complete row.
#[test]
fn a_window_row_is_marked_by_its_column_alone() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join("src")).expect("dir");
    let line = format!(
        "{} NEEDLE {}",
        "z".repeat(LONG_LINE_BYTES),
        "z".repeat(LONG_LINE_BYTES)
    );
    fs::write(root.path().join("src/one.txt"), format!("short\n{line}\n")).expect("fixture");
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
    assert_eq!(hit["line"], 2, "{row}");
    assert!(
        hit["column"].as_u64().is_some(),
        "the window is marked: {row}"
    );
    let mut output = serde_json::json!({"results": [{"index": 0, "data": row}]});
    octocode_runtime_base::response::workflow_lead::retain_guidance(
        octocode_runtime_base::id::ToolId::LocalSearch,
        &mut output,
        &serde_json::json!({"queries": [request]}),
    );
    let data = &output["results"][0]["data"];
    let path = data["files"][0]["path"].as_str().expect("a path");
    assert_eq!(
        data.to_string().matches(path).count(),
        1,
        "the row names its file once: {data}"
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
