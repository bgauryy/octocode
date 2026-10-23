//! Full-content and byte pages: budgets, response windows, shares and
//! character boundaries.

use super::*;

/// A whole-file read and its continuations page by the row's share of the
/// window, gutters and JSON included, never by a fixed line count or the
/// old byte ceiling: every page fits and the pages rebuild the source.
#[test]
fn full_content_continuation_pages_by_the_byte_budget_not_100_lines() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("short-lines.rs");
    let source = "let x = 1;\n".repeat(6_000);
    fs::write(&file, &source).expect("fixture should be written");
    let mut query = q(&file);
    query.full_content = Some(true);
    let mut read = String::new();
    for call in 0.. {
        assert!(call < 10, "the walk ends");
        let page = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, Some(50_000), None);
        if call == 0 {
            assert!(
                page.partial_reasons.is_empty(),
                "{:?}",
                page.partial_reasons
            );
        }
        let query_json = crate::response::result::query_value(&query);
        let row = serde_json::to_value(&page).expect("row");
        let wire =
            crate::tools::test_support::published(crate::id::ToolId::LocalFetch, &query_json, &row);
        assert_eq!(
            crate::response::read_share::fitted_lines(
                &wire["results"][0]["data"],
                &query_json,
                50_000
            ),
            None,
            "the page fits its share"
        );
        let pagination = page.pagination.clone().expect("paged");
        let content = page.content.clone().expect("content");
        read.push_str(&content);
        let Some(next) = page.next.and_then(|next| next.continue_window) else {
            break;
        };
        assert!(pagination.length > 1_000, "{pagination:?}");
        assert!(
            content.len() > 16_384,
            "the old byte ceiling still limits the page"
        );
        assert!(next.query.length.is_none(), "{:?}", next.query);
        query = next.query;
    }
    assert_eq!(read, source);
}

/// A whole-file read is cut at the configured response window, not at a
/// fixed size: the same file pages under a small window and returns whole
/// under a large one.
#[test]
fn full_content_is_cut_at_the_configured_response_window() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("mid.txt");
    fs::write(&file, "let value = 1;\n".repeat(1_500)).expect("fixture");
    let mut full = q(&file);
    full.full_content = Some(true);
    let read = |window| execute_local_fetch(&full, &paths, &Safe, &NeverCancel, window, None);
    let small = read(Some(10_000));
    assert!(
        small.partial_reasons.is_empty(),
        "{:?}",
        small.partial_reasons
    );
    assert!(small.next.and_then(|next| next.continue_window).is_some());
    let large = read(Some(50_000));
    assert!(
        large.partial_reasons.is_empty(),
        "{:?}",
        large.partial_reasons
    );
    assert!(large.next.is_none());
    assert_eq!(large.returned_lines, Some(1_500));
}

#[test]
fn full_content_limits_return_executable_continuations() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let view_limited = t.0.join("view.txt");
    fs::write(&view_limited, "x".repeat(60_000)).expect("view fixture should be written");
    let mut compact = q(&view_limited);
    compact.full_content = Some(true);
    compact.minify = Some(MinifyMode::Standard);
    let result = execute_local_fetch(&compact, &paths, &Safe, &NeverCancel, Some(50_000), None);
    // Page 1 of the view is returned inline with the continuation.
    assert_eq!(result.error_code, None);
    assert!(
        result
            .content
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert!(
        result.partial_reasons.is_empty(),
        "{:?}",
        result.partial_reasons
    );
    assert!(result.next.and_then(|next| next.continue_window).is_some());

    let source_temp = Fixture::new();
    let source_limited = source_temp.0.join("source.txt");
    fs::write(&source_limited, "y".repeat(110 * 1024)).expect("source fixture should be written");
    let mut full = q(&source_limited);
    full.full_content = Some(true);
    let result = execute_local_fetch(
        &full,
        &Paths(source_temp.0.clone()),
        &Safe,
        &NeverCancel,
        Some(50_000),
        None,
    );
    assert_eq!(result.error_code, None);
    assert!(
        result
            .content
            .as_deref()
            .is_some_and(|text| !text.is_empty())
    );
    assert!(
        result.partial_reasons.is_empty(),
        "{:?}",
        result.partial_reasons
    );
    assert!(result.next.and_then(|next| next.continue_window).is_some());
}

/// A match read cut to its response share fills it: the long page's
/// coordinates priced the first cut, and the shorter page grows while its
/// row still fits.
#[test]
fn a_page_cut_to_its_share_fills_it() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let p = t.0.join("pagination.rs");
    // A source where every third line holds a hit: the fixture stays the
    // same whatever the crate's own sources look like.
    let source: String = (0..900)
        .map(|line| match line % 3 {
            0 => format!("    let offset{line} = page.offset + {line};\n"),
            _ => format!("    let value{line} = compute({line}, \"padding text for the page\");\n"),
        })
        .collect();
    fs::write(&p, source).expect("fixture");
    for (response, least) in [(1_500, 1), (3_000, 22), (5_000, 54), (20_000, 300)] {
        let share =
            crate::response::read_share::row_window(crate::id::ToolId::LocalFetch, response, 1);
        let query = qj(
            &p,
            serde_json::json!({"matchString":"offset","contextLines":1}),
        );
        let page = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, Some(share), None);
        let row = serde_json::to_value(&page).expect("row");
        // The share holds the row as published: default extras pruned and
        // the row minimized, as the response stage ships it.
        let wire = crate::tools::test_support::published(
            crate::id::ToolId::LocalFetch,
            &crate::response::result::query_value(&query),
            &row,
        );
        let chars = crate::response::read_share::row_chars(&wire["results"][0]["data"]);
        let lines = page.returned_lines.unwrap_or(0);
        assert!(
            chars <= share || lines == 1,
            "{response}: {chars} > {share}"
        );
        assert!(
            lines >= least && (page.next.is_none() || chars * 5 >= share * 4),
            "{response}: {lines} lines in {chars} of {share}"
        );
    }
}

#[test]
fn byte_pages_preserve_utf8_and_use_byte_offsets() {
    let t = Fixture::new();
    let p = t.0.join("a.txt");
    fs::write(&p, "a😀b").expect("test fixture operation should succeed");
    let paths = Paths(t.0.clone());
    let mut req = q(&p);
    req.unit = Some(WindowUnit::Bytes);
    req.length = wire_positive(2);
    let a = fetch(&req, &paths);
    assert_eq!(a.content.as_deref(), Some("a😀"));
    req = a
        .next
        .expect("test fixture operation should succeed")
        .continue_window
        .expect("test fixture operation should succeed")
        .query;
    let b = fetch(&req, &paths);
    assert_eq!(b.content.as_deref(), Some("b"));
    assert_eq!(b.returned_chars, Some(1))
}

/// An explicit byte offset inside a character starts the page at that
/// character's first byte, says so, and loses no byte.
#[test]
fn a_mid_character_byte_offset_snaps_to_the_character_start() {
    let t = Fixture::new();
    let p = t.0.join("mb.txt");
    fs::write(&p, "abc語def\n").expect("fixture");
    let paths = Paths(t.0.clone());
    let r = fetch(
        &qj(
            &p,
            serde_json::json!({"unit": "bytes", "offset": 4, "length": 4}),
        ),
        &paths,
    );
    assert_eq!(r.error_code, None, "{r:?}");
    assert_eq!(r.content.as_deref(), Some("語d"), "{r:?}");
    assert!(
        r.warnings
            .iter()
            .any(|warning| warning.contains("starts at its first byte, 3")),
        "{:?}",
        r.warnings
    );
}
