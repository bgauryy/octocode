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

/// The most bytes a first answer about a match in a long line takes.
const WINDOW_PAGE_BYTES: usize = 400;

/// D3: a match deep inside a line too long for one page is on page 1, with
/// the bytes around it, at the byte offset the page states. The required
/// pages then hold the rest of the line: its redacted prefix and suffix,
/// every byte exactly once, with no optional read.
#[test]
fn a_match_deep_in_a_long_line_is_on_page_one_and_the_line_is_walked_once() {
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
    let query = qj(&path, serde_json::json!({"matchString":["NEEDLE_HIT"]}));

    let read = fetch(&query, &paths);
    assert_eq!(read.status, ReadStatus::Success, "{read:?}");
    let page = read.pagination.clone().expect("pagination");
    assert_eq!(page.unit, ReadProgressUnit::Bytes, "{page:?}");
    let text = read.content.as_deref().expect("content");
    let hit = view.find("NEEDLE_HIT").expect("hit");
    assert!(
        text.contains("NEEDLE_HIT"),
        "page 1 holds the match: {text}"
    );
    assert!(text.len() <= WINDOW_PAGE_BYTES, "{} bytes", text.len());
    assert!(
        page.offset < hit && hit + "NEEDLE_HIT".len() < page.offset + page.length,
        "the page states where its bytes are, around byte {hit}: {page:?}"
    );
    assert_eq!(page.total_bytes, view.len(), "{page:?}");
    assert!(
        related_rows(&read).is_empty(),
        "no optional prefix recovery"
    );
    let pages = walk_exactly_once(&query, &paths, None, &view);
    assert_eq!(pages[0].0, page.offset);
    assert_eq!(pages[1].0, 0, "the rest starts at the line's first byte");
}

/// D3: every match in a long line has its window before the rest of the
/// line, in source order, on as few pages as hold them (one here: windows
/// that touch are one run, far ones follow behind a gap marker); a window
/// next to a redaction never cuts its marker; the walk is the line exactly
/// once.
#[test]
fn every_match_window_of_a_long_line_comes_before_the_rest() {
    let fixture = Fixture::new();
    let filler = positional(120_000);
    let line = format!(
        "NEEDLE{}SECRET{}NEEDLE{}NEEDLE{}NEEDLE{}SECRET{}NEEDLE",
        &filler[..30_000],
        &filler[30_000..30_020],
        // Two hits 40 bytes apart share one window.
        &filler[30_020..30_060],
        &filler[30_060..90_000],
        &filler[90_000..90_010],
        &filler[90_010..],
    );
    let path = fixture.write("bundle.min.js", format!("head\n{line}\ntail\n"));
    let paths = Paths(fixture.0.clone());
    let view = format!("{line}\n").replace("SECRET", "[REDACTED]");
    let hits: Vec<usize> = view.match_indices("NEEDLE").map(|(at, _)| at).collect();
    assert_eq!(hits.len(), 5);
    let query = qj(&path, serde_json::json!({"matchString":["NEEDLE"]}));
    for window in [None, Some(1_000)] {
        let pages = walk_exactly_once(&query, &paths, window, &view);
        // First the windows, on one page: the line's first bytes, the pair,
        // the hit after it, the line's last bytes.
        assert!(pages[0].1.contains("NEEDLE"), "{window:?}: {:?}", pages[0]);
        // The gap markers name the file's bytes: the selection starts after
        // `head\n`.
        let shift = "head\n".len();
        let windows: Vec<(usize, String)> =
            crate::tools::source_view::pagination::placed_pieces(pages[0].0 + shift, &pages[0].1)
                .into_iter()
                .map(|(at, piece)| (at - shift, piece.to_owned()))
                .collect();
        assert_eq!(
            windows.len(),
            4,
            "{window:?}: {:?}",
            &pages[..5.min(pages.len())]
        );
        assert!(
            windows.windows(2).all(|pair| pair[0].0 < pair[1].0),
            "{window:?}: windows in source order"
        );
        let shown: usize = windows
            .iter()
            .map(|(_, text)| text.matches("NEEDLE").count())
            .sum();
        assert_eq!(shown, hits.len(), "{window:?}: every match has its window");
        for (at, text) in &windows {
            assert!(
                text.len() <= WINDOW_PAGE_BYTES,
                "{window:?}: {at}+{}",
                text.len()
            );
            assert_eq!(
                text.matches("[REDACTED").count(),
                text.matches("[REDACTED]").count(),
                "{window:?}: a marker shows whole: {text}"
            );
        }
        assert_eq!(windows[0].0, 0, "{window:?}: a hit at the line start");
        assert!(
            pages[1..].iter().all(|(_, text)| !text.contains("NEEDLE")),
            "{window:?}: the rest holds no match"
        );
    }
}

/// N6: one file states one byte count. A match read of a long line after
/// a banner line (jquery.min.js) names the file's bytes, as a plain read
/// does: its `total` is the file's size, not its selection's (the banner and
/// its line break were left out of 87,444 of 87,533), its `offset` and gap
/// markers are the file's bytes, and each piece is the file's bytes there.
#[test]
fn a_match_read_states_the_file_bytes_a_plain_read_states() {
    let fixture = Fixture::new();
    let banner = "/*! bundle v1 | (c) contributors | example.org/license */\n";
    let filler = positional(87_000);
    let line = format!(
        "{}NEEDLE{}NEEDLE{}NEEDLE{}",
        &filler[..20_000],
        &filler[20_000..60_000],
        &filler[60_000..80_000],
        &filler[80_000..]
    );
    let source = format!("{banner}{line}\n");
    let path = fixture.write("bundle.min.js", &source);
    let paths = Paths(fixture.0.clone());
    let wire =
        |read: &LocalFetchResult| serde_json::to_value(read).expect("row")["pagination"].clone();
    let plain = wire(&fetch(&q(&path), &paths));
    assert_eq!(plain["unit"], "bytes", "{plain}");
    assert_eq!(plain["total"], source.len(), "{plain}");
    let query = qj(&path, serde_json::json!({"matchString":["NEEDLE"]}));
    let read = fetch(&query, &paths);
    let page = wire(&read);
    assert_eq!(page["unit"], "bytes", "{page}");
    assert_eq!(page["total"], plain["total"], "one total per unit: {page}");
    let at = page["offset"].as_u64().expect("offset") as usize;
    let text = read.content.as_deref().expect("content");
    let pieces = crate::tools::source_view::pagination::placed_pieces(at, text);
    assert!(pieces.len() > 1, "the hits share page one: {pieces:?}");
    for (at, piece) in pieces {
        assert_eq!(
            source.get(at..at + piece.len()),
            Some(piece),
            "the file's bytes at {at}"
        );
    }
    // The walk is the selection exactly once, as before.
    walk_exactly_once(&query, &paths, None, &format!("{line}\n"));
}

/// A match a line page reaches after short context lines: the short lines
/// are page 1, the match's window is page 2, and the walk is the selection
/// exactly once.
#[test]
fn a_long_line_reached_after_context_lines_shows_its_match_next() {
    let fixture = Fixture::new();
    let filler = positional(100_000);
    let line = format!("{}NEEDLE_HIT{}", &filler[..80_000], &filler[80_000..]);
    let source = format!("head one\nhead two\n{line}\n");
    let path = fixture.write("bundle.min.js", &source);
    let paths = Paths(fixture.0.clone());
    let query = qj(
        &path,
        serde_json::json!({"matchString":["NEEDLE_HIT"], "contextLines":{"before":2,"after":0}}),
    );
    let read = fetch(&query, &paths);
    assert!(
        related_rows(&read).is_empty(),
        "no optional prefix recovery"
    );
    let pages = walk_exactly_once(&query, &paths, None, &source);
    assert_eq!(pages[0], (0, "head one\nhead two\n".to_owned()));
    assert!(pages[1].1.contains("NEEDLE_HIT"), "{:?}", pages[1]);
    assert!(pages[1].1.len() <= WINDOW_PAGE_BYTES);
}

/// A hit within a window's margin of the line start: its window starts at
/// the line's first byte and the rest follows it.
#[test]
fn a_hit_near_the_line_start_is_a_window_from_the_first_byte() {
    let fixture = Fixture::new();
    let filler = positional(100_000);
    let line = format!("{}NEEDLE_HIT{}", &filler[..40], &filler[40..]);
    let path = fixture.write("bundle.min.js", format!("{line}\n"));
    let paths = Paths(fixture.0.clone());
    let query = qj(&path, serde_json::json!({"matchString":["NEEDLE_HIT"]}));
    let pages = walk_exactly_once(&query, &paths, None, &format!("{line}\n"));
    assert_eq!(pages[0].0, 0);
    assert!(pages[0].1.contains("NEEDLE_HIT"));
    assert_eq!(pages[1].0, pages[0].1.len(), "the rest goes on from it");
}

/// A matched line one byte page holds whole is that page: nothing to put
/// first.
#[test]
fn a_matched_line_within_one_byte_page_is_one_page() {
    let fixture = Fixture::new();
    let line = format!("{}NEEDLE_HIT{}\n", "x".repeat(1_500), "y".repeat(1_500));
    let path = fixture.write("bundle.min.js", format!("head\n{line}tail\n"));
    let paths = Paths(fixture.0.clone());
    let query = qj(&path, serde_json::json!({"matchString":["NEEDLE_HIT"]}));
    for window in [None, Some(600)] {
        let pages = walk_exactly_once(&query, &paths, window, &line);
        assert_eq!(pages.len(), 1, "{window:?}");
    }
}

/// D4: a plain read whose line page stops at a line too long for it fills
/// the page with that line's first bytes: one byte page from the read's
/// first byte, as long as any byte page, and the walk goes on inside the
/// line. Every byte once, in order.
#[test]
fn a_plain_read_fills_its_page_into_the_long_line() {
    let fixture = Fixture::new();
    let banner = "/*! bundle v1 | (c) contributors | example.org/license */\n";
    let source = format!("{banner}{}\nlast();\n", positional(87_000));
    let path = fixture.write("bundle.min.js", &source);
    let paths = Paths(fixture.0.clone());
    let read = fetch(&q(&path), &paths);
    let page = read.pagination.clone().expect("pagination");
    assert_eq!(page.unit, ReadProgressUnit::Bytes, "{page:?}");
    assert_eq!(page.offset, 0, "{page:?}");
    let full = crate::tools::source_view::pagination::LINE_PAGE_BYTES;
    // The room counts text as it serializes: a line break is two characters.
    assert_eq!(page.length + 1, full, "a full page: {page:?}");
    assert_eq!(
        read.source_line_ranges,
        vec![LineRange { start: 1, end: 2 }],
        "the page cites both lines it holds"
    );
    let pages = walk_exactly_once(&q(&path), &paths, None, &source);
    assert!(
        pages.windows(2).all(|pair| pair[0].0 < pair[1].0),
        "a plain read walks in order"
    );
    assert_eq!(
        pages.len(),
        source.len().div_ceil(full) + 1,
        "the long line's pages, and the line after it"
    );
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
        deadline: Some(std::time::Instant::now() + std::time::Duration::from_secs(60)),
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
/// or client-side slicing to reconstruct the selected Unicode evidence: the
/// required pages are the selection exactly once, each at the place it
/// states, the match's window first.
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
            let query = qj(
                &path,
                serde_json::json!({
                    "matchString":[pattern], "contextLines":{"before":0,"after":0}
                }),
            );
            let pages = walk_exactly_once(&query, &paths, Some(window), &selected);
            assert!(
                pages[0].1.contains("needleTOKEN"),
                "{name}, response window {window}: {:?}",
                pages[0]
            );
        }
    }
}

/// D1: a selection (`ranges` or `matchString`) that ends on a long line
/// which is not the file's last walks that line to its end: the walk is the
/// selection exactly once, its last page states `hasMore:false`, and every
/// page that states `hasMore:true` carries its next call
/// ([`walk_exactly_once`] checks the wire of each page).
#[test]
fn a_selection_ending_on_a_long_line_ends_its_walk() {
    let fixture = Fixture::new();
    let filler = positional(40_000);
    let line = format!("{}NEEDLE_HIT{}", &filler[..20_000], &filler[20_000..]);
    let source = format!("{line}\nshort two\nshort three\n");
    let path = fixture.write("longascii.txt", &source);
    let paths = Paths(fixture.0.clone());
    for selector in [
        serde_json::json!({"ranges":[{"startLine":1,"endLine":1}]}),
        serde_json::json!({"matchString":["NEEDLE_HIT"],"contextLines":{"before":0,"after":0}}),
    ] {
        let query = qj(&path, selector.clone());
        for window in [Some(300), Some(2_500), None] {
            let pages = walk_exactly_once(&query, &paths, window, &format!("{line}\n"));
            assert!(pages.len() > 1, "{selector} at {window:?}: a byte walk");
        }
    }
}

/// A page of several match windows of one long line states what it shows:
/// its `length` is the source bytes of its windows (not the span from its
/// first window to its last, which later pages show), and `returnedLines`
/// counts the one source line, not the gap markers between its windows.
#[test]
fn a_page_of_match_windows_states_the_bytes_and_lines_it_shows() {
    let fixture = Fixture::new();
    let filler = positional(300_000);
    let line = format!(
        "{}NEEDLE{}NEEDLE{}NEEDLE{}",
        &filler[..50_000],
        &filler[50_000..150_000],
        &filler[150_000..250_000],
        &filler[250_000..]
    );
    let path = fixture.write("bundle.min.js", &line);
    let paths = Paths(fixture.0.clone());
    let read = fetch(
        &qj(&path, serde_json::json!({"matchString":["NEEDLE"]})),
        &paths,
    );
    assert_eq!(read.status, ReadStatus::Success, "{read:?}");
    let page = read.pagination.clone().expect("pagination");
    let text = read.content.as_deref().expect("content");
    let pieces = crate::tools::source_view::pagination::placed_pieces(page.offset, text);
    assert!(pieces.len() > 1, "one page shows several windows: {text}");
    let shown: usize = pieces.iter().map(|(_, piece)| piece.len()).sum();
    assert_eq!(page.length, shown, "{page:?}");
    assert_eq!(read.returned_lines, Some(1), "{read:?}");
    assert!(page.has_more, "the rest of the line follows: {page:?}");
}
