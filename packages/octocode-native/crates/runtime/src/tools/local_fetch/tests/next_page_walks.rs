//! Following `next.nextPage` alone reaches every row of a read exactly once,
//! whatever else a page offers: a walker keeps only the latest page.
use super::*;
use octocode_contract_types::LocalFetchQuery;
use std::collections::BTreeMap;

/// Lines numbered from 1, about 85 bytes each, `hits` of them holding `needle`,
/// long enough to reach `bytes`.
fn numbered_source(bytes: usize, hits: &[usize]) -> String {
    let mut text = String::with_capacity(bytes + 100);
    let mut line = 1;
    while text.len() < bytes {
        let word = if hits.contains(&line) {
            "needle"
        } else {
            "other"
        };
        text.push_str(&format!("L{line} {:0>70} {word}\n", 0));
        line += 1;
    }
    text
}

fn streamed_size() -> usize {
    crate::tools::local_fetch::acquire::MAX_SOURCE_BYTES as usize + 1024 * 1024
}

/// Every page of `query`, each continuation taken through the input contract,
/// following nothing but `nextPage`.
fn walk(query: LocalFetchQuery, paths: &Paths, window: usize) -> Vec<LocalFetchResult> {
    let (mut pages, mut call) = (Vec::new(), Some(query));
    while let Some(query) = call {
        let row = crate::contracts::validate_built_query("localFetch", serde_json::json!(query))
            .expect("canonical input");
        let admitted = serde_json::from_value(row).expect("admitted");
        let page = execute_local_fetch(&admitted, paths, &Safe, &NeverCancel, Some(window), None);
        assert_eq!(page.error_code, None, "{:?}", page.error);
        call = page
            .next
            .as_ref()
            .and_then(|next| next.continue_window.as_ref())
            .map(|read| read.query.clone());
        pages.push(page);
        assert!(pages.len() < 2_000, "the walk ends");
    }
    pages
}

/// How many times the walk showed each numbered line.
fn shown(pages: &[LocalFetchResult]) -> BTreeMap<usize, usize> {
    let mut seen = BTreeMap::new();
    for page in pages {
        for line in page.content.as_deref().unwrap_or_default().lines() {
            if let Some(number) = line
                .strip_prefix('L')
                .and_then(|rest| rest.split(' ').next())
                .and_then(|number| number.parse().ok())
            {
                *seen.entry(number).or_insert(0) += 1;
            }
        }
    }
    seen
}

fn once(lines: impl IntoIterator<Item = usize>) -> BTreeMap<usize, usize> {
    lines.into_iter().map(|line| (line, 1)).collect()
}

/// More hit lines in one window than `limit`, then one far past it: the shape
/// that lost its last row.
#[test]
fn a_streamed_match_walk_reaches_the_hit_past_a_cut_window() {
    let fixture = Fixture::new();
    let far = 100_001;
    let hits = (1..=100).chain([far]).collect::<Vec<_>>();
    let file = fixture.write("big.log", numbered_source(streamed_size(), &hits));
    octocode_engine::stamp::settle_written_so_far();
    let pages = walk(
        qj(
            &file,
            serde_json::json!({"matchString":["needle"],"limit":50}),
        ),
        &Paths(fixture.0.clone()),
        20_000,
    );
    assert_eq!(
        shown(&pages),
        once(hits.iter().copied()),
        "{} pages",
        pages.len()
    );
    let matched = pages
        .iter()
        .flat_map(|page| page.matched_lines.iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(matched, hits);
    assert!(
        pages
            .iter()
            .all(|page| page.selected_match_count == Some(hits.len())),
        "every page states the whole count"
    );
}

/// Context lines around hits in a cut window are shown once too.
#[test]
fn a_streamed_match_walk_shows_each_context_line_once() {
    let fixture = Fixture::new();
    let hits = (10..=60)
        .step_by(2)
        .chain([5_000, 90_000])
        .collect::<Vec<_>>();
    let file = fixture.write("big.log", numbered_source(streamed_size(), &hits));
    octocode_engine::stamp::settle_written_so_far();
    let pages = walk(
        qj(
            &file,
            serde_json::json!({"matchString":["needle"],"limit":7,"contextLines":{"before":2,"after":2}}),
        ),
        &Paths(fixture.0.clone()),
        20_000,
    );
    let wanted = hits.iter().flat_map(|hit| hit - 2..=hit + 2);
    assert_eq!(shown(&pages), once(wanted), "{} pages", pages.len());
}

/// More windows than one read holds.
#[test]
fn a_streamed_match_walk_reaches_hits_past_a_full_page_of_windows() {
    let fixture = Fixture::new();
    let hits = (1..=300).map(|index| index * 100).collect::<Vec<_>>();
    let file = fixture.write("big.log", numbered_source(streamed_size(), &hits));
    octocode_engine::stamp::settle_written_so_far();
    let pages = walk(
        qj(&file, serde_json::json!({"matchString":["needle"]})),
        &Paths(fixture.0.clone()),
        3_000,
    );
    assert_eq!(
        shown(&pages),
        once(hits.iter().copied()),
        "{} pages",
        pages.len()
    );
}

/// A hit in a line longer than a page: the line pages by bytes, whole, and the
/// walk still reaches the hits after it.
#[test]
fn a_streamed_match_walk_reaches_hits_past_a_line_longer_than_a_page() {
    let fixture = Fixture::new();
    let long = format!("L2 {} needle {}", "x".repeat(40_000), "y".repeat(40_000));
    let mut source = String::from("L1 before\n");
    source.push_str(&long);
    source.push('\n');
    source.push_str(&numbered_source(streamed_size(), &[]).replace("L", "M"));
    source.push_str("L3 needle at the end\n");
    let file = fixture.write("big.log", &source);
    octocode_engine::stamp::settle_written_so_far();
    let pages = walk(
        qj(&file, serde_json::json!({"matchString":["needle"]})),
        &Paths(fixture.0.clone()),
        20_000,
    );
    assert!(pages.len() > 2, "{} pages", pages.len());
    let text = pages
        .iter()
        .map(|page| page.content.as_deref().unwrap_or_default())
        .collect::<String>();
    assert_eq!(text.matches("needle").count(), 2, "{} pages", pages.len());
    assert_eq!(text.matches('x').count(), 40_000);
    assert_eq!(text.matches('y').count(), 40_000);
    assert!(
        text.contains("L3 needle at the end"),
        "{} pages",
        pages.len()
    );
}

/// Several ranges where one does not fit a page: the later ranges follow it on
/// the same walk.
#[test]
fn a_streamed_ranges_walk_reaches_the_ranges_after_a_cut_one() {
    let fixture = Fixture::new();
    let file = fixture.write("big.log", numbered_source(streamed_size(), &[]));
    octocode_engine::stamp::settle_written_so_far();
    let pages = walk(
        qj(
            &file,
            serde_json::json!({"ranges":[
                {"startLine":100,"endLine":150},
                {"startLine":50000,"endLine":51500},
                {"startLine":90000,"endLine":90010},
                {"startLine":120000,"endLine":120400}
            ]}),
        ),
        &Paths(fixture.0.clone()),
        20_000,
    );
    let wanted = (100..=150)
        .chain(50_000..=51_500)
        .chain(90_000..=90_010)
        .chain(120_000..=120_400);
    assert_eq!(shown(&pages), once(wanted), "{} pages", pages.len());
}

/// A long line in the first range pages by bytes before the later range.
#[test]
fn a_streamed_ranges_walk_reaches_the_range_after_a_line_longer_than_a_page() {
    let fixture = Fixture::new();
    let mut source = format!("L1 {}\n", "x".repeat(80_000));
    source.push_str(&numbered_source(streamed_size(), &[]).replace("L", "M"));
    source.push_str("L9 the later range\n");
    let last = source.lines().count();
    let file = fixture.write("big.log", &source);
    octocode_engine::stamp::settle_written_so_far();
    let pages = walk(
        qj(
            &file,
            serde_json::json!({"ranges":[{"startLine":1,"endLine":1},{"startLine":last,"endLine":last}]}),
        ),
        &Paths(fixture.0.clone()),
        20_000,
    );
    let text = pages
        .iter()
        .map(|page| page.content.as_deref().unwrap_or_default())
        .collect::<String>();
    assert_eq!(text.matches('x').count(), 80_000, "{} pages", pages.len());
    assert_eq!(text.matches("L9 the later range").count(), 1);
}

/// The whole-file path holds one view, so its pages already walk it all.
#[test]
fn an_in_memory_walk_reaches_every_row() {
    let fixture = Fixture::new();
    let hits = (1..=100).chain([2_000]).collect::<Vec<_>>();
    let file = fixture.write("small.log", numbered_source(200_000, &hits));
    let paths = Paths(fixture.0.clone());
    let matches = walk(
        qj(
            &file,
            serde_json::json!({"matchString":["needle"],"limit":50}),
        ),
        &paths,
        2_000,
    );
    assert_eq!(shown(&matches), once(hits.iter().copied()));
    let ranges = walk(
        qj(
            &file,
            serde_json::json!({"ranges":[{"startLine":10,"endLine":400},{"startLine":1000,"endLine":1400},{"startLine":2000,"endLine":2001}]}),
        ),
        &paths,
        2_000,
    );
    let wanted = (10..=400).chain(1_000..=1_400).chain(2_000..=2_001);
    assert_eq!(shown(&ranges), once(wanted));
}
