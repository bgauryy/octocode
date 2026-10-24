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

/// A line longer than a streamed window is reachable in full: a byte offset
/// past the window is a valid offset of the line, never `outOfRange`, and the
/// byte walk reaches every byte once.
#[test]
fn a_streamed_line_longer_than_its_window_is_read_to_its_last_byte() {
    let fixture = Fixture::new();
    let line = positional(super::super::large_source::WINDOW_BYTES + 900_000);
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
