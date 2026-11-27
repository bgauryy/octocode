//! A cut row and its `readHits` read list every line of the file's listing
//! exactly once, whatever the run, the context and the hit spacing.
use super::*;
use crate::gh_search_code::leads::unlisted_read;
use crate::gh_search_code::lines::scan;
use octocode_contract_types::{GhGetFileContentQuery, GhSearchCodeQuery};
use octocode_runtime_base::execution::cancel::NeverCancel;
use octocode_runtime_base::security::scan::Passthrough;
use octocode_tools_shared::source_view::local_query::local_query;
use octocode_tools_shared::source_view::{SourceFacts, process_fetched_content};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::Path;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// A file holding `keyword` on the lines `hits` names, filler elsewhere.
fn body(lines: u32, hits: &[u32], keyword: &str) -> String {
    (1..=lines)
        .map(|line| {
            if hits.contains(&line) {
                format!("let at_{line} = {keyword}({line});\n")
            } else {
                format!("let other_{line} = {line};\n")
            }
        })
        .collect()
}

/// The source lines a numbered listing shows.
fn numbers(lines: &[String]) -> BTreeSet<u32> {
    lines.iter().map(|line| number(line)).collect()
}

/// The lines `read` (a ghGetFileContent continuation) lists of `text`, as
/// the shared source view serves it, and the hit lines among them.
fn read_lines(read: &serde_json::Value, text: &str) -> (BTreeSet<u32>, BTreeSet<u32>) {
    let row = read["input"]["queries"][0].clone();
    let query: GhGetFileContentQuery = serde_json::from_value(row).expect("an executable read");
    let page = process_fetched_content(
        &local_query(&query).expect("local query"),
        text.as_bytes(),
        Path::new("src/hits.rs"),
        &SourceFacts {
            window: Some(1 << 20),
            beside: 0,
            interior: false,
            byte_origin: None,
            file_lines: None,
            local_file: false,
        },
        &Passthrough,
        &NeverCancel,
    );
    assert!(page.error_code.is_none(), "{:?}: {read}", page.error);
    assert!(
        page.next
            .as_ref()
            .is_none_or(|next| next.continue_window.is_none()),
        "one page: {read}"
    );
    let shown = page
        .source_line_ranges
        .iter()
        .flat_map(|range| range.start..=range.end)
        .map(|line| u32::try_from(line).expect("line"))
        .collect();
    let hits = page
        .matched_lines
        .iter()
        .map(|&line| u32::try_from(line).expect("line"))
        .collect();
    (shown, hits)
}

#[test]
fn every_run_and_its_read_list_each_line_once() {
    let line_count = 60;
    let layouts: [&[u32]; 4] = [
        &[3, 6, 9, 12, 30, 31, 32, 50, 58],
        &[1, 2, 3, 4, 5, 6],
        &[10, 20, 30, 40, 50, 60],
        &[2, 4, 5, 9, 15, 16, 22, 59, 60],
    ];
    let keyword = "Needle";
    let row = json!({"owner": "a", "repo": "b", "path": "src/hits.rs"});
    let row = row.as_object().expect("row");
    for hits in layouts {
        let text = body(line_count, hits, keyword);
        for (before, after) in [(0, 0), (1, 1), (2, 0), (0, 3), (5, 5)] {
            let window = Window { before, after };
            let query: GhSearchCodeQuery = serde_json::from_value(json!({
                "owner": "a", "repo": "b", "keywords": [keyword],
                "contextLines": {"before": before, "after": after}
            }))
            .expect("query");
            let whole = scan(
                text.as_bytes(),
                "src/hits.rs",
                &[keyword.to_owned()],
                "",
                window,
                &Passthrough,
            );
            let FileHits::Lines { lines, matched, .. } = &whole else {
                panic!("hits: {whole:?}");
            };
            assert_eq!(matched, hits);
            let listing = numbers(lines);
            for start in 0..hits.len() {
                for end in start + 1..=hits.len() {
                    let cut = Cut { start, end };
                    let label = format!("{hits:?} {window:?} {start}..{end}");
                    let Some((listed, unlisted)) = cut.apply(&whole, window) else {
                        assert!(cut.whole(matched), "{label}: only a whole run lists all");
                        continue;
                    };
                    let FileHits::Lines {
                        lines: shown,
                        matched: shown_hits,
                        ..
                    } = &listed
                    else {
                        panic!("{label}: a cut lists lines");
                    };
                    assert_eq!(shown_hits.as_slice(), &hits[start..end], "{label}");
                    let read = unlisted_read(&query, row, SHA, &unlisted).expect("read");
                    assert_eq!(read["input"]["queries"][0]["ref"], SHA, "{label}");
                    let (rest, rest_hits) = read_lines(&read, &text);
                    let shown = numbers(shown);
                    assert!(shown.is_disjoint(&rest), "{label}: {shown:?} and {rest:?}");
                    assert_eq!(
                        shown.union(&rest).copied().collect::<BTreeSet<_>>(),
                        listing,
                        "{label}: every listed line, once"
                    );
                    let unlisted_hits = hits
                        .iter()
                        .filter(|hit| !shown_hits.contains(hit))
                        .copied()
                        .collect::<BTreeSet<_>>();
                    assert_eq!(rest_hits, unlisted_hits, "{label}: the read's hits");
                }
            }
        }
    }
}

/// A run keeps the anchor hit and holds the hits it was asked for.
#[test]
fn a_run_holds_its_count_and_the_anchor() {
    let matched = [2, 5, 9, 14, 20, 27, 35];
    for best in matched {
        for count in 1..=matched.len() {
            let cut = Cut::around(&matched, best, count);
            assert_eq!(cut.end - cut.start, count, "{best} {count}");
            assert!(
                matched[cut.start..cut.end].contains(&best),
                "{best} {count}: {cut:?}"
            );
            // A run that holds the anchor from the start lists the start.
            let anchor = matched.iter().position(|&hit| hit == best).unwrap_or(0);
            if anchor < count {
                assert_eq!(cut.start, 0, "{best} {count}");
            }
        }
    }
}
