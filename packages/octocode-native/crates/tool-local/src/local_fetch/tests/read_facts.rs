//! What a read states once: its file-wide facts on its first page, and its
//! ranges in file order, merged where they overlap.
use super::*;

/// Every page of `query` under the response `window`, each as its wire form.
fn wire_pages(query: &LocalFetchQuery, paths: &Paths, window: usize) -> Vec<serde_json::Value> {
    let mut pages = Vec::new();
    let mut query = query.clone();
    loop {
        let page = execute_local_fetch(&query, paths, &Safe, &NeverCancel, Some(window), None);
        assert_eq!(page.error_code, None, "{page:?}");
        pages.push(serde_json::to_value(&page).expect("wire form"));
        assert!(pages.len() < 500, "walk does not end");
        let Some(next) = page.next.and_then(|next| next.continue_window) else {
            return pages;
        };
        query = next.query;
    }
}

/// A read's file-wide facts (`totalLines`, `blocks`, the unclassified
/// `selectedMatchCount`) stand on its first page; a later page states its
/// own place, and its `pagination` states the total its first page's
/// `totalLines` stood for.
#[test]
fn a_continuation_page_states_read_wide_facts_once() {
    let fixture = Fixture::new();
    let source = (0..400)
        .map(|n| {
            format!(
                "/// doc {n}\npub fn item_{n}(value: usize) -> usize {{\n    value + {n}\n}}\n\n"
            )
        })
        .collect::<String>();
    let file = fixture.write("items.rs", &source);
    let paths = Paths(fixture.0.clone());
    for fields in [
        serde_json::json!({"matchString": ["value +"]}),
        serde_json::json!({"matchString": ["pub fn item_1"], "block": true}),
    ] {
        let pages = wire_pages(&qj(&file, fields.clone()), &paths, 1500);
        assert!(pages.len() > 1, "{fields}: one page");
        let first = &pages[0];
        assert!(first.get("totalLines").is_some(), "{fields}: {first}");
        assert!(
            first.get("selectedMatchCount").is_some(),
            "{fields}: {first}"
        );
        if fields.get("block").is_some() {
            assert!(first.get("blocks").is_some(), "{fields}: {first}");
        }
        for later in &pages[1..] {
            for fact in ["totalLines", "blocks", "selectedMatchCount"] {
                assert!(later.get(fact).is_none(), "{fields}: {fact} again: {later}");
            }
            assert!(
                later["pagination"].get("total").is_some(),
                "{fields}: a later page states its total: {later}"
            );
        }
    }
}

/// Ranges read in file order, each line once: an unordered list of
/// overlapping ranges reads as their sorted union, a gap marker between
/// the runs that do not touch.
#[test]
fn unordered_overlapping_ranges_read_once_in_file_order() {
    let fixture = Fixture::new();
    let source = (1..=30).map(|n| format!("line {n}\n")).collect::<String>();
    let file = fixture.write("lines.txt", &source);
    let paths = Paths(fixture.0.clone());
    let read = fetch(
        &qj(
            &file,
            serde_json::json!({"ranges": [
                {"startLine": 20, "endLine": 22},
                {"startLine": 3, "endLine": 5},
                {"startLine": 4, "endLine": 7},
                {"startLine": 8, "endLine": 9}
            ]}),
        ),
        &paths,
    );
    let shown = read
        .content
        .as_deref()
        .expect("content")
        .lines()
        .map(|line| line.split('\t').next_back().unwrap_or(line).to_owned())
        .collect::<Vec<_>>();
    let lines = |run: std::ops::RangeInclusive<usize>| run.map(|n| format!("line {n}"));
    assert_eq!(shown[..7], lines(3..=9).collect::<Vec<_>>(), "{read:?}");
    assert!(
        !shown[7].starts_with("line ") && shown[7].contains("10-19"),
        "one gap between the runs: {read:?}"
    );
    assert_eq!(shown[8..], lines(20..=22).collect::<Vec<_>>(), "{read:?}");
}
