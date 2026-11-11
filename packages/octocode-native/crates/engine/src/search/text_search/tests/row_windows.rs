//! A matched line too long for a row is shown as windows around its hits
//! ([`RowWindow`]): each a slice of the line that states where it starts,
//! every hit inside one, and a line that fits a row left whole.

use super::*;
use crate::types::{RowWindow, SearchContext, TextSearchMatch};

const WINDOW: RowWindow = RowWindow {
    longer_than: 200,
    margin: 10,
};

fn line_search(path: String, pattern: &str) -> TextSearchOptions {
    let mut options = opts(path, pattern);
    options.row_window = Some(WINDOW);
    options
}

/// The text of `row` placed in `line` by the byte it says it starts at.
fn placed<'a>(line: &'a str, row: &TextSearchMatch) -> &'a str {
    let start = row.window_start.expect("a window row") as usize;
    &line[start..start + row.value.len()]
}

#[test]
fn a_long_line_is_one_row_per_window_each_placed_by_its_start() {
    let t = TmpDir::new();
    // Hits at 300 and 320 share a window; the one at 900 has its own.
    let line = format!(
        "{}needle{}needle{}needle{}",
        "a".repeat(300),
        "b".repeat(14),
        "c".repeat(574),
        "d".repeat(400)
    );
    t.write("min.js", format!("short needle\n{line}\n"));
    for values in [SourceValues::Owned, SourceValues::Deferred] {
        let mut options = line_search(t.path(), "needle");
        options.values = values;
        let result = search(options).expect("search ok");
        let rows = &result.files[0].matches;
        let long = rows.iter().filter(|row| row.line == 2).collect::<Vec<_>>();
        assert_eq!(long.len(), 2, "{rows:?}");
        for row in &long {
            assert_eq!(placed(&line, row), row.value);
            assert_eq!(row.deferred, None);
            assert_eq!(row.first_line, 2);
        }
        assert_eq!(
            long[0].value,
            format!("{}needle{}needle{}", "a".repeat(10), "b".repeat(14), "c".repeat(10))
        );
        assert_eq!((long[0].window_start, long[0].byte_offset, long[0].column), (Some(290), 300, 300));
        assert_eq!(
            long[1].value,
            format!("{}needle{}", "c".repeat(10), "d".repeat(10))
        );
        assert_eq!((long[1].window_start, long[1].byte_offset), (Some(890), 900));
        // The short line is its whole text, as without windows.
        let short = rows.iter().find(|row| row.line == 1).expect("line 1");
        assert_eq!(short.window_start, None);
        // Every hit is still counted once.
        assert_eq!(result.stats.match_count, Some(4));
        assert_eq!(result.stats.matched_lines, Some(2));
    }
}

#[test]
fn window_edges_stay_on_characters_and_columns_count_utf16() {
    let t = TmpDir::new();
    // Two-byte characters around the hit: a margin of 10 bytes lands inside one.
    let line = format!("{}needle{}", "é".repeat(150), "é".repeat(150));
    t.write("accents.txt", format!("{line}\n"));
    let mut options = line_search(t.path(), "needle");
    options.row_window = Some(RowWindow {
        longer_than: 200,
        margin: 11,
    });
    let result = search(options).expect("search ok");
    let row = &result.files[0].matches[0];
    assert_eq!(placed(&line, row), row.value);
    assert!(row.value.contains("needle"));
    assert_eq!(row.byte_offset, 300);
    assert_eq!(row.column, 150);
    assert_eq!(row.window_start, Some(288));
}

#[test]
fn windows_that_join_into_the_whole_line_and_lines_that_fit_stay_whole() {
    let t = TmpDir::new();
    let dense = "needle ".repeat(60);
    let fits = format!("{}needle", "x".repeat(194));
    t.write("dense.txt", format!("{dense}\n{fits}\n"));
    let result = search(line_search(t.path(), "needle")).expect("search ok");
    let rows = &result.files[0].matches;
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.window_start.is_none()), "{rows:?}");
}

#[test]
fn a_window_row_joins_no_context_and_is_no_neighbours_context() {
    let t = TmpDir::new();
    let line = format!("{}needle{}", "a".repeat(300), "b".repeat(300));
    t.write("ctx.txt", format!("before needle\n{line}\nafter\n"));
    let mut options = line_search(t.path(), "needle");
    options.context = SearchContext::symmetric(1);
    let result = search(options).expect("search ok");
    let rows = &result.files[0].matches;
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].value, "before needle");
    assert_eq!(
        rows[1].value,
        format!("{}needle{}", "a".repeat(10), "b".repeat(10))
    );
    assert_eq!((rows[1].first_line, rows[1].window_start), (2, Some(290)));
}

#[test]
fn a_search_without_row_windows_shows_the_long_line_whole() {
    let t = TmpDir::new();
    let line = format!("{}needle{}", "a".repeat(300), "b".repeat(300));
    t.write("whole.txt", format!("{line}\n"));
    let result = search(opts(t.path(), "needle")).expect("search ok");
    let row = &result.files[0].matches[0];
    assert_eq!(row.value, line);
    assert_eq!(row.window_start, None);
}
