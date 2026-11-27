use super::*;
use octocode_test_support::json_fixture;

#[test]
fn selected_ranges_merge_overlap_without_repeating_or_inventing_bytes() {
    let text = "α\r\n🙂\n三\nlast";
    let ranges = selected_ranges(
        text,
        &json_fixture(
            r#"{
                    "ranges": [
                        {"startLine": 3, "endLine": 4},
                        {"startLine": 1, "endLine": 1},
                        {"startLine": 3, "endLine": 3}
                    ]
                }"#,
        ),
    )
    .expect("valid ranges");
    assert_eq!(ranges, vec![0..4, 9..text.len()]);
    assert_eq!(
        ranges
            .iter()
            .map(|range| &text[range.clone()])
            .collect::<String>(),
        "α\r\n三\nlast"
    );
}

fn spans(text: &str, ranges: Value) -> Vec<Range<usize>> {
    selected_ranges(text, &json!({ "ranges": ranges })).expect("valid ranges")
}

#[test]
fn no_ranges_select_the_whole_text_and_empty_text_selects_nothing() {
    for query in [json!({}), json_fixture(r#"{"ranges": []}"#)] {
        assert_eq!(selected_ranges("a\nb", &query).unwrap(), vec![0..3]);
        assert!(selected_ranges("", &query).unwrap().is_empty());
    }
    assert!(spans("", json_fixture(r#"[{"startLine": 1, "endLine": 1}]"#)).is_empty());
}

#[test]
fn adjacent_ranges_join_and_an_end_past_the_text_is_clamped() {
    assert_eq!(
        spans(
            "a\nb\nc",
            json_fixture(
                r#"[
                        {"startLine": 1, "endLine": 1},
                        {"startLine": 2, "endLine": 2}
                    ]"#
            )
        ),
        vec![0..4]
    );
    assert_eq!(
        spans(
            "a\nb\nc",
            json_fixture(
                r#"[
                        {"startLine": 3, "endLine": 3},
                        {"startLine": 1, "endLine": 1}
                    ]"#
            )
        ),
        vec![0..2, 4..5]
    );
    assert_eq!(
        spans(
            "a\nb\nc",
            json_fixture(r#"[{"startLine": 2, "endLine": 99}]"#)
        ),
        vec![2..5]
    );
}

#[test]
fn ranges_that_select_no_text_are_an_error_not_an_empty_success() {
    // A final newline ends line 2; it does not start a line 3.
    for ranges in [
        json_fixture(r#"[{"startLine": 3, "endLine": 3}]"#),
        json_fixture(r#"[{"startLine": 9, "endLine": 12}]"#),
    ] {
        let error = selected_ranges("a\nb\n", &json!({ "ranges": ranges })).unwrap_err();
        assert_eq!(error.code, ErrorCode::ClassificationContextEmpty);
    }
    assert_eq!(
        spans(
            "a\nb\n",
            json_fixture(r#"[{"startLine": 2, "endLine": 3}]"#)
        ),
        vec![2..4]
    );
}

#[test]
fn a_malformed_range_is_an_invalid_source() {
    for ranges in [json_fixture(r#"["two-three"]"#), json_fixture(r#"[7]"#)] {
        let error = selected_ranges("a\nb", &json!({ "ranges": ranges })).unwrap_err();
        assert_eq!(
            error.code,
            ErrorCode::InvalidClassificationContext,
            "{ranges}"
        );
    }
}

#[test]
fn selection_metadata_records_the_source_and_a_read_of_the_same_snapshot() {
    let snapshot = octocode_engine::digest::sha256(b"a.rs contents");
    let captured = TextResource {
        text: String::new(),
        path: PathBuf::from("a.rs"),
        metadata: json_fixture(r#"{"status": "ready"}"#),
        local_check: None,
        pin: Some(snapshot.clone()),
    };
    let local = json_fixture(r#"{"tool": "localFetch", "query": {"path": "a.rs"}}"#);
    let metadata = selection_metadata(&captured, &local);
    assert_eq!(metadata["status"], "ready");
    assert_eq!(metadata["source"], local);
    assert_eq!(metadata["read"]["tool"], "localFetch");
    assert!(
        metadata["read"].to_string().contains(&snapshot),
        "the lead is pinned to the captured snapshot: {metadata}"
    );
    assert!(captured.metadata.get("source").is_none(), "input untouched");

    let page = json_fixture(r#"{"url": "https://example.org/doc"}"#);
    let metadata = selection_metadata(&captured, &page);
    assert_eq!(metadata["source"], page);
    assert!(metadata.get("read").is_none(), "a URL carries its own lead");
}
