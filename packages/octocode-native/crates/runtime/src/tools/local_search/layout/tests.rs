use super::super::handoff::hit_windows;
use super::super::local_search_query::{View, shows_span_columns};
use super::super::rows::project_match;
use super::costs::Owner;
use super::expansion::{FileReads, RowRead};
use super::*;
use crate::contracts::query_limits::local_fetch::RANGES_MAX_ITEMS;
use octocode_engine::types::{TextSearchFile, TextSearchMatch};
use std::collections::HashMap;
use std::sync::Arc;

fn hit(line: u32, clipped: bool) -> TextSearchMatch {
    TextSearchMatch {
        deferred: None,
        line,
        last_line: None,
        column: 0,
        byte_offset: 0,
        value: format!("needle {line}"),
        value_digest: [0; 32],
        count: None,
        rank: Some(1),
        original_chars: clipped.then_some(4_000),
        original_bytes: clipped.then_some(4_000),
    }
}

/// Kept as rows join, a file's reads equal the batch count over the
/// same rows ([`hit_windows`] merged, long hits one each), and `added`
/// is exactly what `add` then changes.
#[test]
fn incremental_file_reads_equal_the_batch_count() {
    // Lines in rank order: out of line order, overlapping, adjacent,
    // bridging, repeated.
    let lines = [
        40u32, 10, 12, 60, 11, 30, 26, 34, 90, 1, 2, 70, 80, 55, 45, 50, 100, 120, 140, 160, 180,
        200, 220, 240, 260, 25, 33,
    ];
    for context in [0, 2, 4] {
        let mut reads = FileReads::default();
        let mut shown = Vec::new();
        for (n, &line) in lines.iter().enumerate() {
            let read = if n % 7 == 3 {
                RowRead::Long
            } else {
                let (start, end) = hit_windows([line], SearchContext::symmetric(context))[0];
                RowRead::Window { start, end }
            };
            let before = reads.rows();
            let added = reads.added(read);
            reads.add(read);
            assert_eq!(reads.rows(), before + added, "{context} {line}");
            shown.push((line, read));
            let long = shown
                .iter()
                .filter(|(_, read)| *read == RowRead::Long)
                .count();
            let windows = hit_windows(
                shown
                    .iter()
                    .filter(|(_, read)| *read != RowRead::Long)
                    .map(|(line, _)| *line),
                SearchContext::symmetric(context),
            );
            assert_eq!(
                reads.rows(),
                long + windows.len().div_ceil(RANGES_MAX_ITEMS),
                "{context} {line}"
            );
        }
    }
    let mut multiline = FileReads::default();
    assert_eq!(multiline.added(RowRead::Multiline), 1);
    multiline.add(RowRead::Multiline);
    assert_eq!(multiline.added(RowRead::Multiline), 0);
    assert_eq!(multiline.added(RowRead::None), 0);
}

/// Cutting every page prices each row once and each file entry once,
/// however many pages a file rides, and every page keeps within its
/// read bound.
#[test]
fn stream_pages_price_each_row_and_file_once() {
    let files = (0..30u32)
        .map(|n| TextSearchFile {
            path: format!("src/f{n}.rs"),
            match_count: 40,
            matches: (1..=40).map(|line| hit(line * 3, line % 2 == 0)).collect(),
            source: None,
        })
        .collect::<Vec<_>>();
    let rows = std::cell::Cell::new(0usize);
    let entries = std::cell::Cell::new(0usize);
    let read = |matched: &TextSearchMatch| {
        if matched.original_chars.is_some() {
            let (start, end) = hit_windows([matched.line], SearchContext::symmetric(1))[0];
            RowRead::Window { start, end }
        } else {
            RowRead::None
        }
    };
    let pages = stream_pages(
        &files,
        10,
        false,
        2_000,
        StreamCosts {
            file: &|_| {
                entries.set(entries.get() + 1);
                30
            },
            row: &|_| {
                rows.set(rows.get() + 1);
                25
            },
            name: &|_, _| RowName::None,
            read: &read,
        },
        None,
    );
    assert!(pages.len() > 1, "{}", pages.len());
    assert_eq!(rows.get(), 30 * 40);
    assert_eq!(entries.get(), 30);
    let mut seen = 0;
    for page in &pages {
        let reads = page
            .iter()
            .map(|(index, range)| {
                let mut reads = FileReads::default();
                for matched in &files[*index].matches[range.clone()] {
                    reads.add(read(matched));
                }
                reads.rows()
            })
            .sum::<usize>();
        assert!(reads <= max_expansion_rows(), "{reads}");
        seen += page.iter().map(|(_, range)| range.len()).sum::<usize>();
    }
    assert_eq!(seen, 30 * 40, "every row on exactly one page");
}

/// A row is priced from its borrowed value exactly as the copy the page
/// shows serializes, with `column` only where the response keeps it.
#[test]
fn row_chars_price_the_row_as_shown() {
    let mut matched = hit(7, false);
    matched.value = "quote \" tab \t é 😀 needle".into();
    matched.column = 4;
    for (cap, span_columns) in [(None, false), (Some(5), true), (Some(500), false)] {
        let mut row = project_match(&matched, cap);
        if !span_columns {
            row.column = None;
        }
        assert_eq!(
            row_chars(&matched, cap, span_columns),
            octocode_engine::json_size::utf16_units(&row).unwrap_or(0) + 1,
            "{cap:?} {span_columns}"
        );
    }
}

/// Page `page` of `files` cut over `root` with `known` prices: the
/// layout, every price known after the cut, and the files it outlined.
/// Each row's declaration is named on the line before it, unless the
/// files are `nameless`.
fn cut_page(
    root: &std::path::Path,
    files: &[TextSearchFile],
    page: u32,
    known: HashMap<usize, FilePrice>,
    nameless: bool,
) -> (Layout, HashMap<usize, FilePrice>, Vec<usize>) {
    let paths = crate::tools::test_support::workspace_policy(root);
    let mut parsed = octocode_engine::types::TextSearchResult {
        files: files.to_vec(),
        stats: Default::default(),
    };
    let mut outlined = Vec::new();
    let mut outline = |asked: &[(usize, &TextSearchFile)]| {
        Ok(asked
            .iter()
            .map(|&(index, file)| {
                outlined.push(index);
                let owners = file
                    .matches
                    .iter()
                    .filter(|_| !nameless)
                    .map(|m| (m.line, Owner { full: 60 }))
                    .collect();
                (index, FilePrice::Owners(Arc::new(owners)))
            })
            .collect())
    };
    let mut pricing = NamePricing::new(known, &mut outline);
    let request = query(serde_json::json!({"page": page}));
    let layout =
        Layout::cut(&request, &paths, root, &mut parsed, 6_000, &mut pricing).expect("cut");
    let priced = pricing.priced;
    outlined.sort_unstable();
    (layout, priced, outlined)
}

/// A default page cut outlines only the files that may decide its first
/// page: those of the first page a cut with free names reaches, and the
/// row that ends it; never a fixed count of files. Every call of a walk
/// cuts the same pages: fresh, or from the prices the first page stored,
/// which outlines nothing more.
#[test]
fn page_cuts_outline_only_the_first_pages_files_and_every_call_cuts_alike() {
    let root = tempfile::tempdir().expect("fixture directory");
    let files = (0..100u32)
        .map(|n| TextSearchFile {
            path: format!("src/f{n:03}.rs"),
            match_count: 30,
            matches: (1..=30).map(|line| hit(line * 4, false)).collect(),
            source: None,
        })
        .collect::<Vec<_>>();
    let files_of = |layout: &Layout| {
        layout
            .shown
            .iter()
            .map(|(index, _)| *index)
            .collect::<Vec<_>>()
    };
    let (first, stored, outlined) = cut_page(root.path(), &files, 1, HashMap::new(), false);
    let (free, _, _) = cut_page(root.path(), &files, 1, HashMap::new(), true);
    let (after_free, _, _) = cut_page(root.path(), &files, 2, HashMap::new(), true);
    let mut reach = files_of(&free);
    reach.push(after_free.shown[0].0);
    reach.dedup();
    assert_eq!(outlined, reach);
    assert!(outlined.len() < 64, "{outlined:?}");
    assert!(first.total_pages > 2, "{}", first.total_pages);
    assert!(
        files_of(&first)
            .iter()
            .all(|index| outlined.contains(index))
    );
    let mut rows = std::collections::BTreeSet::new();
    for page in 1..=first.total_pages {
        let (fresh, _, _) = cut_page(root.path(), &files, page, HashMap::new(), false);
        let (replayed, _, more) = cut_page(root.path(), &files, page, stored.clone(), false);
        assert!(more.is_empty(), "page {page} outlined {more:?}");
        assert_eq!(fresh.shown, replayed.shown, "page {page}");
        assert_eq!(fresh.total_pages, first.total_pages, "page {page}");
        assert_eq!(replayed.total_pages, first.total_pages, "page {page}");
        for (index, range) in fresh.shown {
            for row in range {
                assert!(rows.insert((index, row)), "row {index}:{row} twice");
            }
        }
    }
    assert_eq!(rows.len(), 100 * 30);
}

fn query(fields: serde_json::Value) -> LocalSearchQuery {
    let mut value = serde_json::json!({"path": ".", "matchString": ["x"]});
    for (key, field) in fields.as_object().expect("fields") {
        value[key] = field.clone();
    }
    serde_json::from_value(value).expect("query")
}

/// Span rows are priced with `column` only while the response keeps
/// it: line rows never carry one, and span rows always do, since equal
/// spans on one line differ only by column (L1-F4: core classes it
/// evidence, not an `extras` detail).
#[test]
fn span_columns_follow_what_the_response_keeps() {
    assert!(!shows_span_columns(&query(serde_json::json!({}))));
    assert!(shows_span_columns(&query(
        serde_json::json!({"view": "matchOnly"})
    )));
}

/// A listed file is priced as the entry its view renders, path escapes,
/// non-BMP text and count widths included.
#[test]
fn listed_file_chars_match_the_rendered_entry() {
    use super::super::types::SearchFile;
    for path in [
        "src/a.rs",
        "dir/\"quoted\"\\name\t.rs",
        "naïve/𝄞/日本.ts",
        "",
    ] {
        for count in [0, 9, 10, 4_294_967_295] {
            let file = TextSearchFile {
                path: path.to_owned(),
                match_count: count,
                matches: Vec::new(),
                source: None,
            };
            for view in [View::Files, View::CountMatches, View::CountLines] {
                let rendered = SearchFile {
                    path: file.path.clone(),
                    matches: None,
                    match_keys: Vec::new(),
                    total_occurrences: (view == View::CountMatches).then_some(count),
                    total_matched_lines: (view == View::CountLines).then_some(count),
                    pagination: None,
                };
                assert_eq!(
                    listed_file_chars(&file, view),
                    octocode_engine::json_size::utf16_units(&rendered).expect("size"),
                    "{path:?} {count} {view:?}"
                );
            }
        }
    }
}
