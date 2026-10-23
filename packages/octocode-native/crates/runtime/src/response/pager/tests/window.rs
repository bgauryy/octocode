use super::*;

#[test]
fn automatic_paging_pages_an_envelope_past_the_budget_in_utf16_units() {
    let structured = json!({"results": [{"index": 0, "data": {
        "items": (0..100).map(|line| json!({"line": line, "value": "quoted \"evidence\""})).collect::<Vec<_>>()
    }}]});
    let mut options = ResponsePageOptions::default();
    options.auto_paginate(&structured, 1_000);
    assert!(options.rows_scope());
    assert_eq!(options.response_length, Some(1_000));
    // The budget counts UTF-16 units, not bytes: 400 emoji are 800 units
    // (1_600 bytes) and fit.
    let mut options = ResponsePageOptions::default();
    options.auto_paginate(
        &json!({"results": [{"index": 0, "data": {"content": "😀".repeat(400)}}]}),
        1_000,
    );
    assert!(!options.rows_scope());
    assert_eq!(options.response_length, None);
    let mut options = ResponsePageOptions::default();
    options.auto_paginate(&json!({"results": []}), 1_000);
    assert!(!options.rows_scope());
    assert_eq!(options.response_length, None);
}

/// An offset or snapshot without a length is not an explicit window: the
/// response keeps the automatic page, so it can never be served unpaged
/// past the contract's `responseLength` maximum.
#[test]
fn an_offset_or_snapshot_alone_keeps_the_automatic_page() {
    let structured = json!({"results": [{"index": 0, "data": {"content": "x".repeat(5_000)}}]});
    for mut options in [
        ResponsePageOptions {
            response_offset: Some(0),
            ..Default::default()
        },
        ResponsePageOptions {
            response_snapshot: Some("response-rows-v1:x".into()),
            ..Default::default()
        },
    ] {
        assert!(!options.explicit());
        options.auto_paginate(&structured, 1_000);
        assert!(options.rows_scope());
        assert_eq!(options.response_length, Some(1_000));
    }
    // A length alone pages whole rows at that length, not the automatic
    // page; only a scoped window windows as asked.
    let mut sized = options(3_000);
    assert!(sized.explicit() && !sized.scoped());
    sized.auto_paginate(&structured, 1_000);
    assert!(sized.rows_scope());
    assert_eq!(sized.response_length, Some(3_000));
    let mut window = ResponsePageOptions {
        response_scope: Some(PageScope::Structured),
        ..options(3_000)
    };
    assert!(window.scoped());
    window.auto_paginate(&structured, 1_000);
    assert!(window.structured_scope() && !window.rows_scope());
    assert_eq!(window.response_length, Some(3_000));
    // A response that fits the length is not windowed at all.
    let mut fits = options(50_000);
    fits.auto_paginate(&structured, 1_000);
    assert_eq!(fits.response_length, None);
}

#[test]
fn a_single_indivisible_row_keeps_its_oversized_disclosure() {
    let content = "whole source evidence ".repeat(300);
    let output = prepare(
        ResponseInput {
            tool: "localFetch".into(),
            structured: json!({"results": [{"index": 0, "data": {"path": "a.txt", "content": content}}]}),
            options: ResponsePageOptions {
                response_length: Some(1_000),
                response_scope: Some(PageScope::Rows),
                ..Default::default()
            },
            remaining: Vec::new(),
        },
        &CancellationToken::new(),
    )
    .expect("whole row");
    assert_eq!(output["results"][0]["data"]["content"], content);
    let pagination = &output["responsePagination"];
    assert_eq!(pagination["oversized"], true);
    assert_eq!(pagination["hasMore"], false);
    assert!(
        pagination["length"]
            .as_u64()
            .is_some_and(|length| length > 1_000)
    );
    assert!(pagination.get("next").is_none());
}

#[test]
fn unicode_windows_reconstruct_exact_text_and_continuations_are_executable() {
    let text = "a😀b\n🧪研究c😀";
    for length in [1, 2, 3, 7] {
        let mut offset = 0;
        let mut snapshot = None;
        let mut joined = String::new();
        loop {
            let result = paginate_window(
                text,
                &ResponsePageOptions {
                    response_length: Some(length),
                    response_offset: Some(offset),
                    response_snapshot: snapshot.clone(),
                    response_scope: None,
                    saved: None,
                },
            );
            let page = result.pagination.expect("pagination");
            joined.push_str(&result.text);
            snapshot = Some(page.snapshot.clone());
            if !page.has_more {
                break;
            }
            offset = page.next_char_offset.expect("continuation");
        }
        assert_eq!(joined, text);
    }
}

#[test]
fn snapshot_and_unicode_boundary_mismatches_restart() {
    let first = paginate_window("a😀b", &options(1));
    let snapshot = first.pagination.expect("page").snapshot;
    let split = paginate_window(
        "a😀b",
        &ResponsePageOptions {
            response_length: Some(1),
            response_offset: Some(2),
            response_snapshot: Some(snapshot),
            response_scope: None,
            saved: None,
        },
    );
    assert_eq!(split.pagination.expect("page").restart, Some(true));
    let changed = paginate_window(
        "changed",
        &ResponsePageOptions {
            response_length: Some(2),
            response_offset: Some(2),
            response_snapshot: Some("response-v1:stale".into()),
            response_scope: None,
            saved: None,
        },
    );
    assert_eq!(changed.pagination.expect("page").changed, Some(true));
}

#[test]
fn frozen_line_boundary_empty_and_last_page_metadata_match() {
    let line = paginate_window("0123456789\n0123456789abc", &options(15));
    assert_eq!(line.text, "0123456789\n");
    assert_eq!(line.pagination.expect("page").next_char_offset, Some(11));

    let empty = paginate_window("", &options(100));
    let empty_page = empty.pagination.expect("page");
    assert_eq!(empty_page.total_chars, 0);
    assert!(!empty_page.has_more);
    assert_eq!(empty_page.next_char_offset, None);

    let short = paginate_window("short", &options(100));
    assert!(!short.pagination.expect("page").has_more);
}

#[test]
fn missing_snapshot_and_changed_beyond_end_return_typed_restart() {
    let missing = paginate_window(
        "a".repeat(100).as_str(),
        &ResponsePageOptions {
            response_length: Some(10),
            response_offset: Some(10),
            ..Default::default()
        },
    );
    let missing_page = missing.pagination.expect("page");
    assert_eq!(missing_page.restart, Some(true));
    assert_eq!(missing_page.changed, Some(false));
    assert_eq!(missing_page.next_char_offset, Some(0));

    let stale = paginate_window(
        "short",
        &ResponsePageOptions {
            response_length: Some(5),
            response_offset: Some(500),
            response_snapshot: Some("response-v1:stale".into()),
            response_scope: None,
            saved: None,
        },
    );
    let stale_page = stale.pagination.expect("page");
    assert_eq!(stale_page.char_offset, 5);
    assert_eq!(stale_page.char_length, 0);
    assert_eq!(stale_page.changed, Some(true));
}

#[test]
fn cancellation_fails_before_paging() {
    let input = ResponseInput {
        remaining: Vec::new(),
        tool: "tool".into(),
        structured: json!({}),
        options: options(2),
    };
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        prepare(input, &cancelled),
        Err(ResponseError::Cancelled)
    ));
}
