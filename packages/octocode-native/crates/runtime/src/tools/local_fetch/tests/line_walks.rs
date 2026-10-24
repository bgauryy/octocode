//! Walks over oversized lines reach every byte once, in byte pages.

use super::*;
use octocode_contract_types::{ReadProgressUnit, ReadView};

/// Default matching selects complete hit lines, with no implicit context.
#[test]
fn default_match_context_is_zero_for_long_and_short_lines() {
    let fixture = Fixture::new();
    let paths = Paths(fixture.0.clone());
    for width in [3, 505] {
        let hit = format!("HIT {}\n", "x".repeat(width));
        let path = fixture.write(
            &format!("line-{width}.txt"),
            format!("before\n{hit}after\n"),
        );
        let result = fetch(
            &qj(&path, serde_json::json!({"matchString":["HIT"]})),
            &paths,
        );
        assert_eq!(result.content.as_deref(), Some(hit.as_str()));
        assert_eq!(
            result.source_line_ranges,
            vec![LineRange { start: 2, end: 2 }]
        );
        assert!(result.next.is_none());
    }
}

/// A `unit:"lines"` walk over a file holding one line longer
/// than a page reaches every source byte exactly once. Line pages continue
/// at the next line; the oversized line is served as byte pages that each
/// cite it as `sourceLineRanges: [{line, endLine}]` (the wire names),
/// and line paging resumes on the following line.
#[test]
fn line_walk_over_an_oversized_line_covers_every_byte_once() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("bundle.js");
    let mut source = String::new();
    for i in 1..=1_164 {
        source.push_str(&format!("var line_{i} = {i};\n"));
    }
    let long_line = 1_165;
    source.push_str(&format!("    CSS: '{}',\n", "x".repeat(42_830)));
    for i in 1..=11 {
        source.push_str(&format!("tail_{i}();\n"));
    }
    let total_lines = long_line + 11;
    fs::write(&file, &source).expect("fixture");
    let mut query = qj(&file, serde_json::json!({}));
    let (mut text, mut last_line, mut byte_pages, mut pages) = (String::new(), 0, 0, 0);
    loop {
        let page = fetch(&query, &paths);
        assert_eq!(page.error_code, None, "{page:?}");
        pages += 1;
        assert!(pages < 40, "runaway paging");
        let wire = serde_json::to_value(&page).expect("serializable");
        let content = page.content.clone().unwrap_or_default();
        let unit = page.pagination.as_ref().map(|p| p.unit);
        if unit == Some(ReadProgressUnit::Bytes) {
            byte_pages += 1;
            assert_eq!(
                wire["sourceLineRanges"],
                serde_json::json!([{"line": long_line, "endLine": long_line}]),
                "a byte page cites the oversized line it serves"
            );
            last_line = long_line;
        } else {
            let ranges = &page.source_line_ranges;
            let first = ranges.first().map_or(0, |r| r.start);
            assert_eq!(
                first,
                last_line + 1,
                "line page {pages} must continue gap-free"
            );
            assert!(
                ranges.windows(2).all(|w| w[1].start == w[0].end + 1),
                "{ranges:?}"
            );
            last_line = ranges.last().map_or(last_line, |r| r.end);
        }
        text.push_str(&content);
        match page.next.and_then(|next| next.continue_window) {
            Some(next) => query = next.query,
            None => break,
        }
    }
    assert!(byte_pages >= 3, "the 42 KB line spans several byte pages");
    assert_eq!(last_line, total_lines);
    assert!(text == source, "pages join into the source exactly once");
}

#[test]
fn selected_line_walk_keeps_byte_fallback_and_every_source_byte() {
    use crate::tools::selection::{Decision, JudgeCall, SelectionJudge};
    use serde_json::Value;

    use std::time::Duration;

    struct SourceOrderJudge;
    impl SelectionJudge for SourceOrderJudge {
        fn identity(&self) -> Value {
            serde_json::json!({"backend":"source-order-test"})
        }
        fn select(&self, call: JudgeCall<'_>) -> crate::tools::selection::Judgments {
            Ok(call
                .admitted
                .iter()
                .map(|&ordinal| (ordinal, Decision::Selected))
                .collect())
        }
    }

    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("selected-giant.txt");
    let mut source = (1..=1_164)
        .map(|line| format!("var line_{line} = {line};\n"))
        .collect::<String>();
    source.push_str(&format!("    CSS: '{}',\n", "x".repeat(42_830)));
    source.push_str("last line without newline");
    fs::write(&file, &source).expect("fixture");
    let executor = tokio::runtime::Runtime::new().expect("runtime");
    let selection = crate::tools::test_support::selection_fake::runtime(
        SourceOrderJudge,
        executor.handle().clone(),
        Duration::from_secs(10),
    );
    let mut query = qj(&file, serde_json::json!({}));
    amend(
        &mut query,
        serde_json::json!({"classification":serde_json::json!({"predicates":["find CSS"]})}),
    );
    let (mut text, mut units, mut pages) = (String::new(), Vec::new(), 0);
    loop {
        let page = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, None, Some(&selection));
        assert_eq!(page.error_code, None, "{page:?}");
        assert_eq!(
            page.selection.as_ref().map(|receipt| receipt.status),
            Some(octocode_contract_types::SelectionStatus::Complete)
        );
        pages += 1;
        assert!(pages < 40, "runaway paging");
        units.push(page.pagination.as_ref().map(|pagination| pagination.unit));
        text.push_str(page.content.as_deref().unwrap_or_default());
        match page.next.and_then(|next| next.continue_window) {
            Some(next) => query = next.query,
            None => break,
        }
    }
    assert!(
        units
            .windows(2)
            .any(|pair| pair == [Some(ReadProgressUnit::Lines), Some(ReadProgressUnit::Bytes)])
    );
    assert!(
        units
            .windows(2)
            .any(|pair| pair == [Some(ReadProgressUnit::Bytes), Some(ReadProgressUnit::Lines)])
    );
    assert_eq!(
        text, source,
        "selection and mixed units preserve every source byte"
    );
}

/// A path-only read of a minified (one huge line) file pages by bytes;
/// every continuation carries the unit of the page it continues, so walking
/// `next.continueWindow` verbatim reaches the end byte-identical.
#[test]
fn path_only_walk_of_a_minified_file_is_byte_identical() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("min.js");
    let source: String = (0..40_000)
        .map(|i| format!("needle{i} "))
        .collect::<String>()
        .chars()
        .take(300_000)
        .collect();
    fs::write(&file, &source).expect("fixture");
    let mut query = q(&file);
    let (mut text, mut pages) = (String::new(), 0);
    loop {
        let page = fetch(&query, &paths);
        assert_eq!(page.error_code, None, "{page:?}");
        assert!(!page.out_of_range, "continuation left the view: {page:?}");
        text.push_str(&page.content.clone().unwrap_or_default());
        pages += 1;
        assert!(pages < 40, "runaway paging");
        match page.next.and_then(|next| next.continue_window) {
            Some(next) => query = next.query,
            None => break,
        }
    }
    assert!(pages >= 18, "{pages} pages");
    assert!(text == source, "pages join into the source exactly once");
}

/// A whole-file read through one oversized (minified) line byte-chunks only
/// that line: once it ends, paging resumes by lines and every later line
/// keeps its number. The walk stays lossless.
#[test]
fn full_content_resumes_numbered_lines_after_one_oversized_line() {
    let t = Fixture::new();
    let paths = Paths(t.0.clone());
    let file = t.0.join("strings.ts");
    let mut source = String::new();
    for i in 1..=1_000 {
        source.push_str(&format!("export const before_{i} = {i};\n"));
    }
    source.push_str(&format!("const table = \"{}\";\n", "x".repeat(27_500)));
    for i in 1..=300 {
        source.push_str(&format!("export const after_{i} = {i};\n"));
    }
    fs::write(&file, &source).expect("fixture");
    let mut query = q(&file);

    // The standard view: every line carries its source number (`N\t`).
    amend(
        &mut query,
        serde_json::json!({"view":Some(ReadView::Compact)}),
    );
    let mut pages = Vec::new();
    loop {
        let page = execute_local_fetch(&query, &paths, &Safe, &NeverCancel, Some(20_000), None);
        assert_eq!(page.error_code, None, "{page:?}");
        let next = page.next.clone().and_then(|next| next.continue_window);
        pages.push(page);
        match next {
            Some(next) => query = next.query,
            None => break,
        }
        assert!(pages.len() < 40, "runaway paging");
    }
    // Every page after the long line is a numbered line page.
    let long_line = 1_001;
    let mut numbered_after = Vec::new();
    let mut text = String::new();
    for page in &pages {
        let content = page.content.clone().unwrap_or_default();
        let unit = page.pagination.as_ref().map(|p| p.unit);
        if unit == Some(ReadProgressUnit::Lines) {
            for line in content.lines() {
                let (number, _) = line.split_once('\t').expect("numbered line");
                let number: usize = number.parse().expect("number");
                if number > long_line {
                    numbered_after.push(number);
                }
            }
        }
        text.push_str(&content);
    }
    assert_eq!(numbered_after, (1_002..=1_301).collect::<Vec<_>>());
    // Pages join into the numbered view exactly once: no gap, no repeat.
    let view = source
        .lines()
        .enumerate()
        .map(|(index, line)| format!("{}\t{line}\n", index + 1))
        .collect::<String>();
    // The numbered view ends without the source's final newline.
    let (text, view) = (text.trim_end_matches('\n'), view.trim_end_matches('\n'));
    let at = text
        .bytes()
        .zip(view.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(text.len().min(view.len()));
    assert!(
        text == view,
        "lossless walk diverges at byte {at} ({} vs {} bytes): got {:?} want {:?}",
        text.len(),
        view.len(),
        &text[at.saturating_sub(40)..(at + 80).min(text.len())],
        &view[at.saturating_sub(40)..(at + 80).min(view.len())]
    );
}
