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
        &query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":2}]})),
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
        &query(serde_json::json!({"path":file.path(),"ranges":[{"startLine":3,"endLine":3}]})),
        None,
    );
    assert_eq!(read.content.as_deref(), Some("\u{feff}gamma\n"), "{read:?}");
}

#[test]
fn default_streamed_read_fills_short_lines_and_continues_the_long_line() {
    let mut source = "x\n".repeat(1_200).into_bytes();
    source.extend(std::iter::repeat_n(b'z', 11 * 1024 * 1024));
    let file = temp(&source);
    let first = fetch(&file, &query(serde_json::json!({"path":file.path()})), None);
    assert_eq!(first.content.as_deref(), Some("x\n".repeat(1200).as_str()));
    let next = first
        .next
        .and_then(|next| next.continue_window)
        .expect("long line remains");
    crate::contracts::prepare_many_built(
        "localFetch",
        serde_json::json!({"queries":[&next.query]}),
    )
    .expect("canonical private continuation");
    assert_eq!(local_fetch_query::offset(&next.query), Some(1200));
    let second = fetch(&file, &next.query, None);
    assert!(
        second
            .content
            .as_deref()
            .is_some_and(|text| !text.is_empty() && text.bytes().all(|byte| byte == b'z'))
    );
    assert!(second.next.and_then(|next| next.continue_window).is_some());
}

/// An offset past the end of a streamed read is the in-memory read's
/// out-of-range page (nothing returned, a restart), not an error.
#[test]
fn a_streamed_offset_past_the_end_is_an_out_of_range_page() {
    let text = "x\n".repeat(50);
    let file = temp(text.as_bytes());
    let read = |json: serde_json::Value| fetch(&file, &query(json), None);
    for json in [
        serde_json::json!({"path":file.path(),"readProgress":{"unit":"lines","offset":80}}),
        serde_json::json!({"path":file.path(),"ranges":[{"startLine":10,"endLine":20}],"readProgress":{"unit":"lines","offset":30}}),
    ] {
        let result = read(json);
        assert_eq!(result.status, ReadStatus::Success, "{result:?}");
        assert!(result.out_of_range, "{result:?}");
        assert_eq!(result.content.as_deref(), Some(""));
        let restart = result.next.and_then(|next| next.restart).expect("restart");
        assert_eq!(local_fetch_query::offset(&restart.query).unwrap_or(0), 0);
        // `pagination.outOfRange` states it; no warning restates it.
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }
    let past =
        read(serde_json::json!({"path":file.path(),"ranges":[{"startLine":90,"endLine":95}]}));
    assert_eq!(past.error_code, Some(ErrorCode::InvalidInput), "{past:?}");
}

/// A streamed page fits the row's share: its window page is cut to it,
/// and ranges share it, the span that does not fit reading on.
#[test]
fn streamed_multirange_continuations_reach_every_requested_line_once() {
    let source = (1..=2000)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    let file = temp(source.as_bytes());
    let start = query(
        serde_json::json!({"path":file.path(),"ranges":[{"startLine":1,"endLine":300},{"startLine":600,"endLine":900}]}),
    );
    let mut pending = std::collections::VecDeque::from([start]);
    let mut seen = std::collections::BTreeSet::new();
    let mut pages = 0;
    while let Some(q) = pending.pop_front() {
        let out = fetch(&file, &q, Some(500));
        assert_eq!(out.error_code, None, "{out:?}");
        for line in out
            .content
            .as_deref()
            .expect("content")
            .lines()
            .filter_map(|line| line.strip_prefix("line "))
        {
            assert!(
                seen.insert(line.parse::<usize>().expect("line number")),
                "duplicate {line}"
            );
        }
        if let Some(next) = out.next {
            for call in [next.continue_window, next.read_other_ranges]
                .into_iter()
                .flatten()
            {
                crate::contracts::prepare_many_built(
                    "localFetch",
                    serde_json::json!({"queries":[&call.query]}),
                )
                .expect("canonical continuation");
                pending.push_back(call.query);
            }
        }
        pages += 1;
        assert!(pages < 100);
    }
    assert!(pages > 2);
    assert_eq!(seen, (1..=300).chain(600..=900).collect());
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

/// Walking every emitted continuation of a read whose giant private-key body
/// line sits across the window edge joins into the independently sanitized
/// source exactly once: no gap, no duplicate, no key byte.
#[test]
fn a_walk_across_a_giant_key_body_reconstructs_the_sanitized_source_once() {
    let mut text = "x".repeat(2 * BYTE_LOOKBEHIND + 17);
    text.push_str("\r\n-----BEGIN PRIVATE KEY-----\r\n");
    text.push_str(&"A".repeat(2 * BYTE_LOOKBEHIND + 5));
    text.push_str("\r\n-----END PRIVATE KEY-----\r\n\r\n");
    text.push_str(&"€".repeat(BYTE_LOOKBEHIND));
    let mut keys = crate::security::KeyBlockState::default();
    let expected: String = text
        .split_inclusive('\n')
        .map(|line| {
            if keys.push_line(line.trim_end_matches(['\r', '\n'])) {
                format!("{}\n", crate::security::key_fragment_placeholder())
            } else {
                line.to_owned()
            }
        })
        .collect();
    let file = temp(text.as_bytes());
    let mut call = query(serde_json::json!({"path":file.path()}));
    let (mut joined, mut pages) = (String::new(), 0);
    loop {
        let page = fetch(&file, &call, Some(2 * WINDOW_BYTES));
        assert_eq!(page.error_code, None, "page {pages}");
        joined.push_str(page.content.as_deref().expect("content"));
        pages += 1;
        assert!(pages < 20, "runaway paging");
        match page.next.and_then(|next| next.continue_window) {
            Some(next) => {
                crate::contracts::prepare_many_built(
                    "localFetch",
                    serde_json::json!({"queries":[&next.query]}),
                )
                .expect("canonical continuation");
                call = next.query;
            }
            None => break,
        }
    }
    assert!(!joined.contains("AAAA"));
    assert!(
        joined == expected,
        "walk of {pages} pages: {} bytes, expected {}",
        joined.len(),
        expected.len()
    );
}
