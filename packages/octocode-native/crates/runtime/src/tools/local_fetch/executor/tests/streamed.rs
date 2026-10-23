//! Sources past the whole-file ceiling served as streamed line windows.

use super::*;

/// A bare read of a source past the whole-file ceiling states the span it
/// returned (not the window it held) and a line pagination in file lines.
#[test]
fn a_streamed_bare_read_states_the_span_it_returned() {
    let temp = Fixture::new();
    let dir = temp.0.clone();
    let (path, lines) = big_log(&dir, payload_line);
    let first = fetch(&q(&path), &Paths(dir.clone()));
    assert_eq!(first.status, ReadStatus::Success, "{first:?}");
    let returned = first.returned_lines.expect("returned lines");
    assert_eq!(first.end_line, Some(returned), "{first:?}");
    assert!(
        first
            .warnings
            .iter()
            .any(|warning| warning.starts_with("Large source (")),
        "{:?}",
        first.warnings
    );
    let page = first.pagination.as_ref().expect("pagination");
    assert_eq!(page.unit, WindowUnit::Lines, "{page:?}");
    assert_eq!(page.offset, 0, "{page:?}");
    assert_eq!(page.length, returned, "{page:?}");
    assert_eq!(page.total_lines, lines, "{page:?}");
    assert!(page.has_more, "{page:?}");
    assert_eq!(page.next_offset, Some(returned), "{page:?}");
}

// Past the whole-file ceiling a plain read streams a bounded line window:
// exact whole-file totals, a digest-bound continuation that advances,
// and matchString redirected to localSearch.
#[test]
fn oversized_text_source_is_served_as_streamed_line_windows() {
    let temp = Fixture::new();
    let dir = temp.0.clone();
    let (path, lines) = big_log(&dir, payload_line);
    let query = |q: LocalFetchQuery| fetch(&q, &Paths(dir.clone()));
    let first = query(q(&path));
    assert_eq!(first.status, ReadStatus::Success, "{first:?}");
    assert_eq!(first.total_lines, Some(lines));
    assert_eq!(first.start_line, Some(1));
    assert!(
        first
            .content
            .as_deref()
            .is_some_and(|c| c.starts_with("line 1 "))
    );
    let next = first
        .next
        .as_ref()
        .and_then(|n| n.continue_window.clone())
        .expect("continueWindow");
    assert!(next.query.snapshot.is_some());
    let second = query(next.query);
    assert_eq!(second.status, ReadStatus::Success, "{second:?}");
    assert_eq!(second.start_line, first.end_line.map(|line| line + 1));

    // A wide explicit window pages within itself and advances.
    let wide = query(LocalFetchQuery {
        path: path.to_string_lossy().parse().expect("path"),
        ranges: vec![format!("{}-{}", 1, 3_000).parse().expect("range")],
        ..test_local_fetch_query()
    });
    let inner = wide
        .next
        .as_ref()
        .and_then(|n| n.continue_window.clone())
        .expect("inner continue");
    let wide_two = query(inner.query);
    let page_end = |r: &LocalFetchResult| r.source_line_ranges.last().map(|range| range.end);
    let page_start = |r: &LocalFetchResult| r.source_line_ranges.first().map(|range| range.start);
    assert_eq!(
        page_start(&wide_two),
        page_end(&wide).map(|line| line + 1),
        "pages advance"
    );

    let tail = query(LocalFetchQuery {
        path: path.to_string_lossy().parse().expect("path"),
        ranges: vec![format!("{}-{}", lines, lines).parse().expect("range")],
        ..test_local_fetch_query()
    });
    assert_eq!(tail.start_line, Some(lines), "{tail:?}");
    assert!(
        tail.content
            .as_deref()
            .is_some_and(|c| c.starts_with(&format!("line {lines} ")))
    );
    assert!(tail.next.is_none());

    let matched = query(LocalFetchQuery {
        path: path.to_string_lossy().parse().expect("path"),
        match_string: Some(MatchString::String("line 7 ".parse().expect("match"))),
        ..test_local_fetch_query()
    });
    assert_eq!(matched.error_code, Some(ErrorCode::LargeSourceWindowOnly));
    // The localSearch is an executable lead, and the tip stays short.
    let search = matched
        .next
        .as_ref()
        .and_then(|next| next.text_search.clone())
        .expect("findText lead");
    assert_eq!(search["tool"], "localSearch", "{search}");
    assert_eq!(search["query"]["queries"][0]["matchString"], "line 7 ");
    assert!(
        matched.hints.iter().all(|hint| hint.len() <= 120),
        "{matched:?}"
    );
}

// Several `ranges` on an oversized source are each streamed window reads:
// one response holds every requested line once, with an omission marker
// between non-adjacent ranges, instead of a rejection whose repair
// restarts at line 1.
#[test]
fn oversized_source_serves_several_ranges_in_one_read() {
    let temp = Fixture::new();
    let dir = temp.0.clone();
    let (path, lines) = big_log(&dir, payload_line);
    let far = lines - 2;
    let result = fetch(
        &LocalFetchQuery {
            path: path.to_string_lossy().parse().expect("path"),
            ranges: vec![
                format!("{far}-{}", far + 1).parse().expect("range"),
                "1-2".parse().expect("range"),
                "2-3".parse().expect("range"),
            ],
            ..test_local_fetch_query()
        },
        &Paths(dir.clone()),
    );
    assert_eq!(result.status, ReadStatus::Success, "{result:?}");
    assert_eq!(result.total_lines, Some(lines));
    assert_eq!(
        result.content.as_deref(),
        Some(
            format!(
                "line 1 payload payload payload payload\nline 2 payload payload payload payload\nline 3 payload payload payload payload\n... [lines 4-{} not requested] ...\nline {far} payload payload payload payload\nline {} payload payload payload payload\n",
                far - 1,
                far + 1
            )
            .as_str()
        )
    );
    let spans: Vec<(usize, usize)> = result
        .source_line_ranges
        .iter()
        .map(|range| (range.start, range.end))
        .collect();
    assert_eq!(spans, [(1, 3), (far, far + 1)]);
    assert!(result.next.is_none(), "{result:?}");
    assert_eq!(result.error_code, None);
}

/// A streamed range pages in its own lines: every page continues where
/// the last one ended (no stub page at a window boundary), the page
/// offset counts from the range's start without resetting, every line
/// is reached once, and the streamed note is stated by the first page.
#[test]
fn a_streamed_range_pages_evenly_across_window_boundaries() {
    let temp = Fixture::new();
    let dir = temp.0.clone();
    let (path, _) = big_log(&dir, |n| {
        format!("{n:08} payload payload payload payload\n")
    });
    let (start, end) = (100usize, 6_000usize);
    let mut query = LocalFetchQuery {
        path: path.to_string_lossy().parse().expect("path"),
        ranges: vec![format!("{start}-{end}").parse().expect("range")],
        ..test_local_fetch_query()
    };
    let mut next_line = start;
    let mut sizes = Vec::new();
    for page_index in 0.. {
        let page = fetch(&query, &Paths(dir.clone()));
        assert_eq!(page.status, ReadStatus::Success, "{page:?}");
        let pagination = page.pagination.as_ref().expect("pagination");
        assert_eq!(pagination.offset, next_line - start, "page {page_index}");
        assert_eq!(pagination.total_lines, end + 1 - start, "page {page_index}");
        assert_eq!(
            page.warnings.iter().any(|w| w.starts_with("Large source")),
            page_index == 0,
            "page {page_index}: {:?}",
            page.warnings
        );
        let rows = page.content.as_deref().unwrap_or_default().lines().count();
        for row in page.content.as_deref().unwrap_or_default().lines() {
            assert_eq!(
                row,
                format!("{next_line:08} payload payload payload payload")
            );
            next_line += 1;
        }
        sizes.push(rows);
        match page.next.and_then(|next| next.continue_window) {
            Some(read) => query = read.query,
            None => break,
        }
    }
    assert_eq!(next_line, end + 1, "every line once");
    // Equal lines: every page but the last is a full page.
    let full = sizes[0];
    assert!(sizes.len() > 2, "{sizes:?}");
    assert!(
        sizes[..sizes.len() - 1].iter().all(|size| *size == full),
        "{sizes:?}"
    );
}

// A line chunk of an oversized source larger than one response page
// pages inside the chunk and then goes on past it: every line from the
// first is reached once, in order, across the chunk boundary.
#[test]
fn oversized_source_line_chunks_continue_past_a_paged_chunk() {
    let temp = Fixture::new();
    let dir = temp.0.clone();
    let (path, _) = big_log(&dir, payload_line);
    let mut query = LocalFetchQuery {
        path: path.to_string_lossy().parse().expect("path"),
        unit: Some(WindowUnit::Lines),
        length: crate::tools::source_view::types::wire_positive(1_000),
        ..test_local_fetch_query()
    };
    let mut next_line = 1usize;
    for _ in 0..12 {
        let page = fetch(&query, &Paths(dir.clone()));
        assert_eq!(page.status, ReadStatus::Success, "{page:?}");
        for row in page.content.as_deref().unwrap_or_default().lines() {
            assert_eq!(
                row,
                format!("line {next_line} payload payload payload payload")
            );
            next_line += 1;
        }
        query = page
            .next
            .as_ref()
            .and_then(|next| next.continue_window.clone())
            .unwrap_or_else(|| panic!("no continuation after line {}", next_line - 1))
            .query;
    }
    assert!(next_line > 2_001, "walked past two chunks: {next_line}");
}
