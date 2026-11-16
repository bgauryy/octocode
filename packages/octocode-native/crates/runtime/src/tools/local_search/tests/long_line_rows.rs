//! S4a: a hit in a line past `LONG_LINE_BYTES` is a window of that line: the row
//! states its column and where the window starts, its value is the line's own
//! text there, and the read it hands off is of that one line, opened at its
//! hits where the read holds the source whole.
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

/// The read a window row hands off opens the long line at its hit: every
/// page of that read up to the last shown hit that shows the long line shows
/// a hit in it, never the line's first bytes when its hit sits pages into it,
/// whatever other rows the file shows.
#[test]
fn the_read_of_a_window_row_opens_the_line_at_its_hit() {
    let root = tempfile::tempdir().expect("fixture");
    fs::create_dir_all(root.path().join("src")).expect("dir");
    let filler = "z".repeat(200_000);
    let line = format!("{filler} NEEDLE {filler}");
    let files = [
        ("src/one.min.js", format!("{line}\n")),
        (
            "src/mixed.min.js",
            format!("short NEEDLE line\n{}{line}\n", "plain\n".repeat(20)),
        ),
    ];
    for (file, text) in &files {
        fs::write(root.path().join(file), text).expect("fixture");
    }
    let (policy, security) = policy_for(root.path());
    for (file, _) in files {
        let request = ls_query(
            serde_json::json!({"path": file, "matchString": ["NEEDLE"]}),
            None,
        );
        let row = search_in(&request, &policy, &security, None)
            .expect("search")
            .to_row()
            .expect("row");
        let shown = row["files"][0]["matches"].as_array().expect("hits").len();
        let read = crate::tools::test_support::checks::related_call(&row, "localFetch", |_| true);
        let mut query: octocode_contract_types::LocalFetchQuery =
            serde_json::from_value(read["input"]["queries"][0].clone()).expect("localFetch row");
        let mut seen = 0;
        while seen < shown {
            let page = crate::tools::local_fetch::execute_local_fetch(
                &query,
                &policy,
                &security,
                &NeverCancel,
                None,
                None,
            );
            let content = page
                .content
                .as_deref()
                .unwrap_or_else(|| panic!("{page:?}"));
            let hits = content.matches("NEEDLE").count();
            assert!(
                hits > 0 || !content.contains("zzzz"),
                "{file}: a page of the long line without its hit: {read}: {:?}",
                page.pagination
            );
            seen += hits;
            let Some(next) = page.next.and_then(|next| next.continue_window) else {
                break;
            };
            query = next.query;
        }
        assert_eq!(
            seen, shown,
            "{file}: the read shows every shown hit: {read}"
        );
    }
}

/// A window row says so on its row: one warning names the first window hit
/// and the localFetch read of its whole line, so that read stays on a
/// complete row once the response stage drops optional leads.
#[test]
fn a_window_row_is_disclosed_and_keeps_its_whole_line_read() {
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
        serde_json::json!({"path": "src", "matchString": ["NEEDLE"]}),
        None,
    );
    let row = search_in(&request, &policy, &security, None)
        .expect("search")
        .to_row()
        .expect("row");
    let at = format!("{}:2", row["files"][0]["path"].as_str().expect("a path"));
    let warning = row["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .find(|warning| warning.contains(&at))
        .unwrap_or_else(|| panic!("the window hit {at} is disclosed: {row}"));
    assert!(warning.contains("localFetch"), "{warning}");
    let mut output = serde_json::json!({"results": [{"index": 0, "data": row}]});
    crate::response::workflow_lead::retain_guidance(
        crate::id::ToolId::LocalSearch,
        &mut output,
        &serde_json::json!({"queries": [request]}),
    );
    let data = &output["results"][0]["data"];
    assert!(
        data["related"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|call| call["tool"] == "localFetch"),
        "the whole-line read stays: {data}"
    );
}
