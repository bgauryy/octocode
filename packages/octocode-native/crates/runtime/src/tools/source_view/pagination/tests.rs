use super::*;
use octocode_contract_types::{LocalFetchQuery, ReadProgressUnit};

#[test]
fn default_line_pages_fill_the_byte_window_and_resume_completely() {
    let source = "x\n".repeat(12_000);
    let mut query: LocalFetchQuery = serde_json::from_value(serde_json::json!({
        "path":"short.txt"
    }))
    .expect("query");
    let first = page_within(&source, &query, LineBudget::bytes(LINE_PAGE_BYTES), None);
    assert_eq!(first.pagination.length, LINE_PAGE_BYTES / 2);
    assert_eq!(first.text, source[..LINE_PAGE_BYTES]);
    query = continuation(&query, &first.pagination)
        .expect("continuation")
        .continue_window
        .expect("window")
        .query;
    let last = page_within(&source, &query, LineBudget::bytes(LINE_PAGE_BYTES), None);
    assert_eq!(first.text + &last.text, source);
    assert!(!last.pagination.has_more);
}

/// A line longer than the configured line page but shorter than the
/// default one pages by bytes only through itself: line paging resumes
/// right after it, and the walk reaches every byte exactly once.
#[test]
fn a_line_over_the_configured_page_resumes_line_paging_after_it() {
    let long = format!("{}\n", "y".repeat(3_000));
    let content = format!("{}{long}{}", "short\n".repeat(10), "tail\n".repeat(300));
    let budget = 1_000;
    let mut query: LocalFetchQuery =
        serde_json::from_value(serde_json::json!({"path":"long.txt"})).expect("query");
    let mut walked = String::new();
    let mut units = Vec::new();
    for _ in 0..20 {
        let page = page_within(&content, &query, LineBudget::bytes(budget), None);
        if page.pagination.unit == ReadProgressUnit::Bytes {
            assert!(
                long.contains(&page.text),
                "a byte page stays inside the long line"
            );
        }
        units.push(page.pagination.unit);
        walked.push_str(&page.text);
        let Some(next) = continuation(&query, &page.pagination) else {
            break;
        };
        query = next.continue_window.expect("next").query;
    }
    assert_eq!(walked, content);
    assert_eq!(units.last(), Some(&ReadProgressUnit::Lines), "{units:?}");
}

#[test]
fn default_raw_line_windows_keep_source_coordinates_after_transformations() {
    let source = SourceLines {
        lines: (1..=5_000).filter(|line| line % 2 == 0).collect(),
        total: 5_000,
    };
    let content = "x\n".repeat(source.lines.len());
    let query: LocalFetchQuery = serde_json::from_value(serde_json::json!({
        "path":"compact.txt"
    }))
    .expect("query");
    let result = page_within(
        &content,
        &query,
        LineBudget::bytes(LINE_PAGE_BYTES),
        Some(&source),
    );
    assert_eq!(result.text, content);
    assert_eq!(result.pagination.length, source.total);
    assert!(!result.pagination.has_more);
}

/// The whole-line scan: the line holding `offset` measured from its real
/// start to its real end, whatever the page's end.
fn reference(
    content: &str,
    line: usize,
    offset: usize,
    end: usize,
    line_page_bytes: usize,
) -> Option<(usize, usize)> {
    let bytes = content.as_bytes();
    let line_start = bytes[..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |at| at + 1);
    let line_end = bytes[offset..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(bytes.len(), |at| offset + at + 1);
    (line_end - line_start > line_page_bytes && end >= line_end && line_end < bytes.len())
        .then_some((line_end, line))
}

/// Lines of every length around the line page, read at every offset by
/// pages of several sizes, give the answer of the whole-line scan.
#[test]
fn a_bounded_scan_answers_as_the_whole_line_scan() {
    let page = 8;
    let content: String = [0, 1, 7, 8, 9, 10, 20, 3, 8, 9, 0]
        .iter()
        .map(|len| format!("{}\n", "x".repeat(*len)))
        .collect::<String>()
        + "tail without newline";
    let mut oversized = 0;
    for offset in 0..=content.len() {
        let line = 1 + content.as_bytes()[..offset]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count();
        for length in [1, 4, 8, 9, 16, 30, content.len()] {
            let end = (offset + length).min(content.len());
            let expected = reference(&content, line, offset, end, page);
            oversized += usize::from(expected.is_some());
            assert_eq!(
                oversized_line_end(&content, line, offset, end, page),
                expected,
                "offset {offset} end {end}"
            );
        }
    }
    assert!(oversized > 20, "the corpus reaches oversized lines");
}
