use super::*;
use octocode_contract_types::{LocalFetchQuery, ReadProgressUnit};

/// A default line page fills its budget as its lines render (`x\n` is three
/// characters of JSON), and the walk resumes to the last line exactly once.
#[test]
fn default_line_pages_fill_the_rendered_budget_and_resume_completely() {
    let source = "x\n".repeat(12_000);
    let mut query: LocalFetchQuery = serde_json::from_value(serde_json::json!({
        "path":"short.txt"
    }))
    .expect("query");
    let rendered = octocode_engine::json_size::string_body_units("x\n");
    let first = page_within(&source, &query, LineBudget::bytes(LINE_PAGE_BYTES), None);
    assert_eq!(first.pagination.length, LINE_PAGE_BYTES / rendered);
    assert_eq!(first.text, source[..2 * (LINE_PAGE_BYTES / rendered)]);
    let mut walked = first.text.clone();
    let mut page = first;
    while page.pagination.has_more {
        query = continuation(&query, &page.pagination)
            .expect("continuation")
            .continue_window
            .expect("window")
            .query;
        page = page_within(&source, &query, LineBudget::bytes(LINE_PAGE_BYTES), None);
        walked.push_str(&page.text);
    }
    assert_eq!(walked, source);
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

/// F1: a line within the byte budget that renders over the page's fill (its
/// quotes escape to two characters each) is a line page of its own, and the
/// short lines after it stay line pages: the walk never falls to byte pages
/// for lines a line page holds, and reaches every byte once.
#[test]
fn a_line_rendered_over_the_fill_is_a_one_line_page_and_paging_stays_in_lines() {
    let wide = format!("{}\n", "\"".repeat(150));
    // Past one default byte page, so a byte page it fell to is not the last.
    let content = format!("{wide}{wide}{}", "a\n".repeat(3_000));
    let budget = LineBudget {
        fill: 100,
        line: 200,
        gutter: 4,
    };
    assert!(budget.rendered(&wide) > budget.fill && wide.len() <= budget.line);
    let mut query: LocalFetchQuery =
        serde_json::from_value(serde_json::json!({"path":"quotes.txt"})).expect("query");
    let mut walked = String::new();
    let mut units = Vec::new();
    loop {
        let page = page_within(&content, &query, budget, None);
        units.push(page.pagination.unit);
        walked.push_str(&page.text);
        let Some(next) = continuation(&query, &page.pagination) else {
            break;
        };
        query = next.continue_window.expect("next").query;
    }
    assert_eq!(walked, content);
    assert!(
        units.iter().all(|unit| *unit == ReadProgressUnit::Lines),
        "{units:?}"
    );
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

/// A walk of a transformed view (raw lines behind each view line, some
/// dropped) through an oversized view line: every `next` moves forward, no
/// query repeats, and the pages join to the view. A byte page that ends the
/// long line resumes at the raw line after it, not at its view index (the
/// compact `auth.css` walk cycled lines -> bytes -> bytes -> lines -> lines).
#[test]
fn a_transformed_view_walks_through_an_oversized_line_without_repeating() {
    for (before, long_bytes, after, dropped_every) in [
        (23, 17_765, 76, 3),
        (1, 40_000, 2, 2),
        (5, 16_385, 0, 4),
        (0, 33_000, 9, 5),
    ] {
        let raw_total = (before + 1 + after) * dropped_every;
        let lines: Vec<usize> = (1..=raw_total)
            .filter(|line| line % dropped_every == 0)
            .collect();
        let source = SourceLines {
            lines,
            total: raw_total,
        };
        let mut content = String::new();
        for index in 0..before + 1 + after {
            if index == before {
                content.push_str(&"b".repeat(long_bytes));
                content.push('\n');
            } else {
                content.push_str(&format!("line {index}\n"));
            }
        }
        let mut query: LocalFetchQuery =
            serde_json::from_value(serde_json::json!({"path":"auth.css"})).expect("query");
        let mut walked = String::new();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let page = page_within(
                &content,
                &query,
                LineBudget::bytes(LINE_PAGE_BYTES),
                Some(&source),
            );
            walked.push_str(&page.text);
            let Some(next) = continuation(&query, &page.pagination) else {
                break;
            };
            query = next.continue_window.expect("next").query;
            let key = serde_json::to_string(&query).expect("key");
            assert!(
                seen.insert(key.clone()),
                "repeated {key} at {before}/{long_bytes}"
            );
        }
        assert_eq!(walked, content, "{before}/{long_bytes}/{after}");
    }
}
