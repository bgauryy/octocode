use super::super::local_search_query::{View, shows_span_columns};
use super::super::rows::project_match;
use super::costs::Owner;
use super::*;
use octocode_engine::types::{TextSearchFile, TextSearchMatch};
use std::collections::HashMap;
use std::sync::Arc;

fn hit(line: u32) -> TextSearchMatch {
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
    }
}

/// Cutting every page prices each row once and each file entry once,
/// however many pages a file rides, and every page keeps within its
/// read bound.
#[test]
fn stream_pages_price_each_row_and_file_once() {
    let files = (0..30u32)
        .map(|n| TextSearchFile {
            term_counts: Vec::new(),
            path: format!("src/f{n}.rs"),
            match_count: 40,
            matches: (1..=40).map(|line| hit(line * 3)).collect(),
            source: None,
        })
        .collect::<Vec<_>>();
    let rows = std::cell::Cell::new(0usize);
    let entries = std::cell::Cell::new(0usize);
    let pages = stream_pages(
        &files,
        10,
        false,
        (2_000, None),
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
        },
        None,
    );
    assert!(pages.len() > 1, "{}", pages.len());
    assert_eq!(rows.get(), 30 * 40);
    assert_eq!(entries.get(), 30);
    let mut seen = 0;
    for page in &pages {
        seen += page.iter().map(|(_, range)| range.len()).sum::<usize>();
    }
    assert_eq!(seen, 30 * 40, "every row on exactly one page");
}

/// A row is priced from its borrowed value exactly as the copy the page
/// shows serializes, with `column` only where the response keeps it.
#[test]
fn row_chars_price_the_row_as_shown() {
    let mut matched = hit(7);
    matched.value = "quote \" tab \t é 😀 needle".into();
    matched.column = 4;
    for span_columns in [false, true] {
        let mut row = project_match(&matched);
        if !span_columns {
            row.column = None;
        }
        assert_eq!(
            row_chars(&matched, span_columns),
            octocode_engine::json_size::utf16_units(&row).unwrap_or(0) + 1,
            "{span_columns}"
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
            term_counts: Vec::new(),
            path: format!("src/f{n:03}.rs"),
            match_count: 30,
            matches: (1..=30).map(|line| hit(line * 4)).collect(),
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
        for (count, terms) in [
            (0, vec![]),
            (9, vec![9, 0]),
            (10, vec![]),
            (4_294_967_295, vec![4_294_967_295, 0, 12]),
        ] {
            let file = TextSearchFile {
                term_counts: terms.clone(),
                path: path.to_owned(),
                match_count: count,
                matches: Vec::new(),
                source: None,
            };
            for view in [View::Files, View::CountMatches, View::CountLines] {
                let counted = matches!(view, View::CountMatches | View::CountLines);
                let rendered = SearchFile {
                    path: file.path.clone(),
                    matches: None,
                    match_keys: Vec::new(),
                    total_occurrences: (view == View::CountMatches).then_some(count),
                    total_matched_lines: (view == View::CountLines).then_some(count),
                    term_counts: (counted && !terms.is_empty()).then(|| terms.clone()),
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
