//! `matchString` on sources past the whole-file ceiling: served from the
//! streamed file, every hit and its context once across the pages.

use super::*;
use std::collections::BTreeSet;

/// A source just past the whole-file ceiling with `NEEDLE` on line 7, on
/// every `every`-th line and on its last line: its path, line count and hit
/// lines.
fn needles(dir: &std::path::Path, every: usize) -> (PathBuf, usize, Vec<usize>) {
    let path = dir.join("needles.log");
    let mut text = String::new();
    let mut hits = vec![];
    let mut lines = 0usize;
    while text.len() as u64 <= MAX_SOURCE_BYTES {
        lines += 1;
        if lines == 7 || lines.is_multiple_of(every) {
            hits.push(lines);
            text.push_str(&format!("NEEDLE {lines}\n"));
        } else {
            text.push_str(&payload_line(lines));
        }
    }
    lines += 1;
    hits.push(lines);
    text.push_str(&format!("last NEEDLE {lines}"));
    fs::write(&path, &text).expect("write");
    (path, lines, hits)
}

/// Walking every continuation of a streamed match read serves each hit and
/// each context line exactly once, across more pages than one holds, and
/// every page publishes within the output contract.
#[test]
fn a_streamed_match_read_serves_every_hit_once_across_pages() {
    let temp = Fixture::new();
    let dir = temp.0.clone();
    // About twice the context windows one page's `ranges` holds.
    let source_lines = usize::try_from(MAX_SOURCE_BYTES).expect("bytes") / payload_line(1).len();
    let every = source_lines / (2 * crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS);
    let (path, lines, hits) = needles(&dir, every);
    let (before, after) = (2usize, 20usize);
    assert!(every > before + after + 1, "windows stay apart");
    let start = qj(
        &path,
        serde_json::json!({"matchString":["NEEDLE"],"contextLines":{"before":before,"after":after}}),
    );
    let mut queue = vec![start];
    let (mut matched, mut served, mut pages) = (vec![], vec![], 0);
    while let Some(query) = queue.pop() {
        let page = fetch(&query, &Paths(dir.clone()));
        assert_eq!(page.error_code, None, "{page:?}");
        let wire = serde_json::to_value(&page).expect("page");
        let output = crate::tools::test_support::published(crate::id::ToolId::LocalFetch, &wire);
        crate::tools::test_support::checks::assert_output_contract("localFetch", &output);
        assert_eq!(page.total_lines, Some(lines));
        if page.selected_match_count.is_some() {
            assert_eq!(page.selected_match_count, Some(hits.len()), "{page:?}");
        }
        matched.extend(page.matched_lines.iter().copied());
        for range in &page.source_line_ranges {
            served.extend(range.start..=range.end);
        }
        let rows = page.content.as_deref().unwrap_or_default();
        assert!(rows.contains("NEEDLE") || page.matched_lines.is_empty());
        pages += 1;
        assert!(pages < 100, "runaway paging");
        if let Some(next) = page.next {
            queue.extend(next.read_other_ranges.map(|read| read.query));
            queue.extend(next.continue_window.map(|read| read.query));
        }
    }
    assert!(pages > 1, "more windows than one page holds");
    let mut once = matched.clone();
    once.sort_unstable();
    assert_eq!(once, hits, "every hit once");
    let wanted = hits
        .iter()
        .flat_map(|hit| hit.saturating_sub(before).max(1)..=(hit + after).min(lines))
        .collect::<BTreeSet<_>>();
    let mut each = served.clone();
    each.sort_unstable();
    each.dedup();
    assert_eq!(each.len(), served.len(), "no line served twice");
    assert_eq!(each.into_iter().collect::<BTreeSet<_>>(), wanted);
}

/// A pattern with no hit is an empty read with the whole file's totals.
#[test]
fn a_streamed_match_read_without_hits_is_empty() {
    let temp = Fixture::new();
    let dir = temp.0.clone();
    let (path, lines, _) = needles(&dir, 20_000);
    let page = fetch(
        &qj(&path, serde_json::json!({"matchString":["ABSENT"]})),
        &Paths(dir.clone()),
    );
    assert_eq!(page.status, ReadStatus::Empty, "{page:?}");
    assert_eq!(page.selected_match_count, Some(0));
    assert_eq!(page.total_lines, Some(lines));
}
