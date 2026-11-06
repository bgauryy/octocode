//! One very long line (a minified bundle, a one-line JSON dump) stays
//! reachable in full through `next.*`, in pages of a sensible size.
use super::*;
use octocode_contract_types::ReadProgressUnit;

/// A source of `len` bytes whose every 9-byte record is its own position, so
/// a page read from the wrong place cannot equal the source's slice.
fn positional(len: usize) -> String {
    (0..len.div_ceil(9))
        .map(|at| format!("{at:08};"))
        .collect::<String>()[..len]
        .to_owned()
}

/// A narrow response window sizes line pages, never byte pages: a window of a
/// few dozen characters used to cut a one-line file into pages of half the
/// window (tens of bytes), and the walk to its end took thousands of calls.
#[test]
fn a_narrow_response_window_keeps_byte_pages_a_sensible_size() {
    let fixture = Fixture::new();
    let source = positional(200_000);
    let path = fixture.write("bundle.min.js", &source);
    let paths = Paths(fixture.0.clone());
    let mut query = q(&path);
    let mut collected = String::new();
    let mut pages = 0;
    loop {
        let read = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, Some(80), None);
        assert_eq!(read.status, ReadStatus::Success, "{read:?}");
        let page = read.pagination.as_ref().expect("pagination");
        assert_eq!(page.unit, ReadProgressUnit::Bytes, "{page:?}");
        assert_eq!(page.offset, collected.len(), "pages are contiguous");
        if page.has_more {
            assert!(
                page.length >= crate::tools::source_view::pagination::MIN_BYTE_PAGE,
                "a page of {} bytes",
                page.length
            );
        }
        collected.push_str(read.content.as_deref().expect("content"));
        pages += 1;
        let Some(next) = read.next.and_then(|next| next.continue_window) else {
            break;
        };
        query = next.query;
        assert!(pages < 100, "a byte walk takes sensible pages");
    }
    assert_eq!(collected, source);
}

/// The related localFetch reads `next` names (kind `related`) as JSON rows.
fn related_rows(read: &LocalFetchResult) -> Vec<serde_json::Value> {
    let next = serde_json::to_value(&read.next).expect("next serializes");
    next.as_object()
        .into_iter()
        .flat_map(|calls| calls.values())
        .filter(|call| call["kind"] == "related" && call["tool"] == "localFetch")
        .map(|call| call["input"]["queries"][0].clone())
        .collect()
}

/// Walk `next.nextPage` from `read`, returning each page's content joined.
fn walk_on(mut read: LocalFetchResult, paths: &Paths) -> String {
    let mut joined = String::new();
    for _ in 0..1_000 {
        assert_eq!(read.status, ReadStatus::Success, "{read:?}");
        joined.push_str(read.content.as_deref().expect("content"));
        let Some(next) = read.next.and_then(|next| next.continue_window) else {
            return joined;
        };
        read = fetch(&next.query, paths);
    }
    panic!("the walk did not end");
}

/// A match deep inside a long line selects that whole line. Required pages
/// alone keep its redacted prefix, hit and suffix exactly once.
#[test]
fn a_match_deep_in_a_long_line_keeps_its_prefix_in_required_pages() {
    let fixture = Fixture::new();
    let filler = positional(300_000);
    let hit_at = 200_000;
    let line = format!(
        "{}SECRET{}NEEDLE_HIT{}SECRET{}",
        &filler[..50_000],
        &filler[50_000..hit_at],
        &filler[hit_at..250_000],
        &filler[250_000..]
    );
    let path = fixture.write("bundle.min.js", format!("{line}\n"));
    let paths = Paths(fixture.0.clone());
    let view = format!("{line}\n").replace("SECRET", "[REDACTED]");

    let read = fetch(
        &qj(&path, serde_json::json!({"matchString":["NEEDLE_HIT"]})),
        &paths,
    );
    assert_eq!(read.status, ReadStatus::Success, "{read:?}");
    let page = read.pagination.clone().expect("pagination");
    assert_eq!(page.unit, ReadProgressUnit::Bytes, "{page:?}");
    assert_eq!(
        page.offset, 0,
        "page 1 starts at the selected line: {page:?}"
    );
    assert!(
        related_rows(&read).is_empty(),
        "no optional prefix recovery"
    );
    assert_eq!(walk_on(read, &paths), view);
}

/// A match a line page reaches after short context lines: the short lines
/// are page 1, and the long line then starts at its own first byte.
#[test]
fn a_long_line_reached_after_context_lines_keeps_its_prefix() {
    let fixture = Fixture::new();
    let filler = positional(100_000);
    let line = format!("{}NEEDLE_HIT{}", &filler[..80_000], &filler[80_000..]);
    let path = fixture.write("bundle.min.js", format!("head one\nhead two\n{line}\n"));
    let paths = Paths(fixture.0.clone());
    let read = fetch(
        &qj(
            &path,
            serde_json::json!({"matchString":["NEEDLE_HIT"], "contextLines":{"before":2,"after":0}}),
        ),
        &paths,
    );
    assert_eq!(
        read.content.as_deref(),
        Some("head one\nhead two\n"),
        "{read:?}"
    );
    let next = read
        .next
        .and_then(|next| next.continue_window)
        .expect("next");
    let second = fetch(&next.query, &paths);
    assert!(
        related_rows(&second).is_empty(),
        "no optional prefix recovery"
    );
    assert_eq!(walk_on(second, &paths), format!("{line}\n"));
}

/// A hit inside the line's first page keeps the page at the line start.
#[test]
fn a_hit_within_the_first_page_keeps_the_line_start() {
    let fixture = Fixture::new();
    let filler = positional(100_000);
    let line = format!("{}NEEDLE_HIT{}", &filler[..100], &filler[100..]);
    let path = fixture.write("bundle.min.js", format!("{line}\n"));
    let paths = Paths(fixture.0.clone());
    let read = fetch(
        &qj(&path, serde_json::json!({"matchString":["NEEDLE_HIT"]})),
        &paths,
    );
    assert_eq!(read.pagination.as_ref().expect("pagination").offset, 0);
    assert!(related_rows(&read).is_empty(), "nothing skipped");
    assert_eq!(walk_on(read, &paths), format!("{line}\n"));
}

/// A line longer than a streamed window is reachable in full: a byte offset
/// past the window is a valid offset of the line, never `outOfRange`, and the
/// byte walk reaches every byte once.
#[test]
fn a_streamed_line_longer_than_its_window_is_read_to_its_last_byte() {
    let fixture = Fixture::new();
    let line = positional(crate::security::SourceLimits::REAL.window + 900_000);
    // Streamed sources are those over the in-memory ceiling.
    let outside = (super::super::acquire::MAX_SOURCE_BYTES as usize).saturating_sub(line.len())
        / "outside\n".len()
        + 1_000;
    let path = fixture.write(
        "bundle.min.js",
        format!("{line}\ntail\n{}", "outside\n".repeat(outside)),
    );
    let paths = Paths(fixture.0.clone());
    let view = format!("{line}\n");

    let inside = qj(
        &path,
        serde_json::json!({"ranges":[{"startLine":1,"endLine":1}], "readProgress":{"unit":"bytes","offset":5_000_000}}),
    );
    let read = fetch(&inside, &paths);
    assert_eq!(read.status, ReadStatus::Success, "{read:?}");
    assert!(!read.out_of_range, "{read:?}");
    assert_eq!(
        read.content.as_deref(),
        Some(&view[5_000_000..5_000_000 + 16_384])
    );
    let page = read.pagination.expect("pagination");
    assert_eq!((page.offset, page.length), (5_000_000, 16_384), "{page:?}");
    assert_eq!(page.total_bytes, view.len(), "{page:?}");
    assert!(page.has_more, "{page:?}");

    // Only an offset past the line's last byte is out of range.
    let past = qj(
        &path,
        serde_json::json!({"ranges":[{"startLine":1,"endLine":1}], "readProgress":{"unit":"bytes","offset":view.len()}}),
    );
    let read = fetch(&past, &paths);
    assert!(read.out_of_range, "{read:?}");
    assert_eq!(read.pagination.expect("pagination").total_bytes, view.len());
}

/// A clasify capture keeps every page it reads, and every page reads, scans
/// and digests the whole source: its reader window holds the capture's whole
/// budget, so a selection within it is one page (one source pass), a single
/// line as much as many short ones. Under the caller's response share, one
/// long line read in 16 KiB byte pages cost one full pass per page.
#[test]
fn a_capture_reads_a_selection_within_its_budget_in_one_page() {
    let fixture = Fixture::new();
    let paths = Paths(fixture.0.clone());
    let budget = 300_000;
    let caller = crate::execution::ExecutionContext {
        cancellation: octocode_io::cancellation::Cancellation::new(),
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
        response_window: Some(131_072),
        github_credential: None,
    };
    let reader = crate::tools::source_capture::reader_context(&caller, budget);
    assert_eq!(reader.deadline, caller.deadline, "the call's own deadline");
    let one_line = positional(budget);
    let many_lines = positional(budget - budget / 50)
        .as_bytes()
        .chunks(49)
        .map(|chunk| format!("{}\n", std::str::from_utf8(chunk).expect("ascii")))
        .collect::<String>();
    for (name, source) in [("bundle.min.js", one_line), ("rows.txt", many_lines)] {
        assert!(source.len() <= budget, "{name} fits the budget");
        let path = fixture.write(name, &source);
        let read = execute_local_fetch(
            &q(&path),
            &paths,
            &Safe,
            &NeverCancel,
            reader.response_window,
            None,
        );
        assert_eq!(read.status, ReadStatus::Success, "{name}: {read:?}");
        assert!(
            read.next
                .as_ref()
                .and_then(|next| next.continue_window.as_ref())
                .is_none(),
            "{name}: one page, {:?}",
            read.pagination
        );
        assert_eq!(read.content.as_deref(), Some(source.as_str()), "{name}");
    }
}

/// Tiny windows and multi-line matches must not need optional related reads
/// or client-side slicing to reconstruct the selected Unicode evidence.
#[test]
fn required_pages_reconstruct_long_unicode_matched_lines_exactly_once() {
    let fixture = Fixture::new();
    let paths = Paths(fixture.0.clone());
    let prefix = "α😀界".repeat(5_000);
    for (name, selected, pattern) in [
        (
            "one.txt",
            format!("{prefix}needleTOKEN{}\n", " Ω".repeat(40)),
            "needleTOKEN",
        ),
        (
            "two.txt",
            format!("{prefix}needleTOKEN\nsecond😀{}\n", "β".repeat(5_000)),
            "needleTOKEN\nsecond",
        ),
    ] {
        let path = fixture.write(name, format!("outside before\n{selected}outside after\n"));
        for window in [80, 600, 1_000] {
            let mut query = qj(
                &path,
                serde_json::json!({
                    "matchString":[pattern], "contextLines":{"before":0,"after":0}
                }),
            );
            let mut joined = String::new();
            loop {
                let read =
                    execute_local_fetch(&query, &paths, &Safe, &NeverCancel, Some(window), None);
                assert_eq!(read.status, ReadStatus::Success, "{read:?}");
                let text = read.content.as_deref().expect("content");
                let end = joined.len() + text.len();
                assert!(end <= selected.len(), "no duplicate evidence");
                assert_eq!(
                    text,
                    &selected[joined.len()..end],
                    "contiguous selected bytes"
                );
                joined.push_str(text);
                let page = read.pagination.as_ref().expect("pagination");
                let next = read.next.and_then(|calls| calls.continue_window);
                assert_eq!(
                    page.has_more,
                    next.is_some(),
                    "every remainder is reachable"
                );
                let Some(next) = next else { break };
                assert!(!text.is_empty(), "required pages make progress");
                query = next.query;
            }
            assert_eq!(joined, selected, "{name}, response window {window}");
        }
    }
}
