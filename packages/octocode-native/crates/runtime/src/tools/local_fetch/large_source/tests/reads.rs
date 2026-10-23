use super::*;

/// A streamed source shows line 1 without its leading BOM, as an
/// in-memory read does.
#[test]
fn a_streamed_source_hides_its_leading_bom() {
    let mut source = "\u{feff}alpha\nbeta\n".to_owned().into_bytes();
    source.extend(std::iter::repeat_n(b'z', 11 * 1024 * 1024));
    let file = temp(&source);
    let read = fetch(
        &file,
        &query(serde_json::json!({"path":file.path(),"ranges":["1-2"]})),
        None,
    );
    assert_eq!(read.content.as_deref(), Some("alpha\nbeta\n"), "{read:?}");
}

/// A streamed window that starts after line 1 keeps a U+FEFF its first
/// line begins with: only the file's first bytes hold its byte-order
/// mark.
#[test]
fn a_streamed_window_past_line_one_keeps_a_leading_feff() {
    let mut source = "alpha\nbeta\n\u{feff}gamma\n".to_owned().into_bytes();
    source.extend(std::iter::repeat_n(b'z', 11 * 1024 * 1024));
    let file = temp(&source);
    let read = fetch(
        &file,
        &query(serde_json::json!({"path":file.path(),"ranges":["3-3"]})),
        None,
    );
    assert_eq!(read.content.as_deref(), Some("\u{feff}gamma\n"), "{read:?}");
}

#[test]
fn a_copied_streamed_head_continuation_fills_its_bounded_window() {
    let mut source = "x\n".repeat(1_200).into_bytes();
    source.extend(std::iter::repeat_n(b'z', 11 * 1024 * 1024));
    let file = temp(&source);
    let read = |query: &LocalFetchQuery| fetch(&file, query, None);
    let first = read(&query(serde_json::json!({"path":file.path()})));
    assert_eq!(first.content.as_deref(), Some("x\n".repeat(100).as_str()));
    let next = first
        .next
        .and_then(|next| next.continue_window)
        .expect("head continuation");
    crate::contracts::prepare_many_and_validate(
        "localFetch",
        serde_json::json!({"queries":[&next.query]}),
    )
    .expect("canonical continuation");
    let second = read(&next.query);
    assert_eq!(
        second.content.as_deref(),
        Some("x\n".repeat(1_100).as_str())
    );
    assert_eq!(second.start_line, Some(101));
    assert_eq!(second.end_line, Some(1_200));
    let rest = second
        .next
        .and_then(|next| next.continue_window)
        .expect("unread last line");
    assert_eq!(local_fetch_query::offset(&rest.query), Some(1_200));
    crate::contracts::prepare_many_and_validate(
        "localFetch",
        serde_json::json!({"queries":[&rest.query]}),
    )
    .expect("canonical unread boundary");
    let mut explicit = next.query;
    explicit.length = wire_positive(7);
    assert_eq!(read(&explicit).returned_lines, Some(7));
}

/// An offset past the end of a streamed read is the in-memory read's
/// out-of-range page (nothing returned, a restart), not an error.
#[test]
fn a_streamed_offset_past_the_end_is_an_out_of_range_page() {
    let text = "x\n".repeat(50);
    let file = temp(text.as_bytes());
    let read = |json: serde_json::Value| fetch(&file, &query(json), None);
    for json in [
        serde_json::json!({"path":file.path(),"offset":80}),
        serde_json::json!({"path":file.path(),"ranges":["10-20"],"offset":30}),
    ] {
        let result = read(json);
        assert_eq!(result.status, ReadStatus::Success, "{result:?}");
        assert!(result.out_of_range, "{result:?}");
        assert_eq!(result.content.as_deref(), Some(""));
        let restart = result.next.and_then(|next| next.restart).expect("restart");
        assert_eq!(restart.query.offset, Some(0));
        // `pagination.outOfRange` states it; no warning restates it.
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }
    let past = read(serde_json::json!({"path":file.path(),"ranges":["90-95"]}));
    assert_eq!(past.error_code, Some(ErrorCode::InvalidInput), "{past:?}");
}

/// A streamed page fits the row's share: its window page is cut to it,
/// and ranges share it, the span that does not fit reading on.
#[test]
fn streamed_pages_fit_the_rows_share() {
    let text: String = (1..=2_000)
        .map(|line| format!("const s{line} = \"\\\\\\\\\";\n"))
        .collect();
    let file = temp(text.as_bytes());
    let share = 5_000;
    for (json, ranges) in [
        (serde_json::json!({"path":file.path()}), false),
        (
            serde_json::json!({"path":file.path(),"ranges":["1-300","600-900"]}),
            true,
        ),
    ] {
        let query = query(json);
        let result = fetch(&file, &query, Some(share));
        assert_eq!(result.status, ReadStatus::Success, "{result:?}");
        let row = serde_json::to_value(&result).expect("row");
        assert_eq!(
            crate::response::read_share::fitted_lines(
                &row,
                &crate::response::result::query_value(&query),
                share
            ),
            None,
            "the page fits its share: {}",
            row.to_string().len()
        );
        let next = result.next.expect("more to read");
        assert!(next.continue_window.is_some());
        assert_eq!(next.read_other_ranges.is_some(), ranges);
    }
}

/// A NUL byte past the head sample makes a streamed source binary.
#[test]
fn a_streamed_source_with_a_late_nul_is_binary() {
    let mut bytes = "x\n".repeat(20_000).into_bytes();
    bytes.extend_from_slice(b"\0\x01tail\n");
    let file = temp(&bytes);
    let result = fetch(&file, &query(serde_json::json!({"path":file.path()})), None);
    assert_eq!(result.error_code, Some(ErrorCode::BinaryFileUnsupported));
}
