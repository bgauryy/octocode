//! S4a: a hit in a line past `LONG_LINE_BYTES` is a window of that line: the row
//! states its column and where the window starts, its value is the line's own
//! text there, and the read it hands off is of that one line.
use super::*;
use octocode_engine::security::LONG_LINE_BYTES;

#[test]
fn a_hit_in_a_line_past_a_whole_read_is_a_window_with_its_column() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join("src")).expect("dir");
    let before = "z".repeat(LONG_LINE_BYTES / 2);
    let line = format!("{before} NEEDLE {}", "z".repeat(LONG_LINE_BYTES / 2 + 2));
    fs::write(root.path().join("src/one.txt"), &line).expect("fixture");
    let (policy, security) = policy_for(root.path());
    let request = ls_query(
        serde_json::json!({"path": "src", "matchString": ["NEEDLE"]}),
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
    let reads = row["related"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|call| call["tool"] == "localFetch")
        .collect::<Vec<_>>();
    assert!(!reads.is_empty(), "a read of the hit is offered: {row}");
    assert!(
        reads
            .iter()
            .all(|call| call["input"]["queries"][0]["ranges"]
                == serde_json::json!([{"startLine": 1, "endLine": 1}])),
        "the read is of the one line: {reads:?}"
    );
}
