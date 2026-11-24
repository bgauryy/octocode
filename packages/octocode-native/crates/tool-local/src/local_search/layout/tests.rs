use super::super::local_search_query::{View, shows_span_columns};
use super::super::rows::project_match;
use super::costs::Owner;
use super::*;
use octocode_engine::types::{TextSearchFile, TextSearchMatch};
use octocode_test_support::json_fixture;
use std::collections::HashMap;
use std::sync::Arc;

fn hit(line: u32) -> TextSearchMatch {
    TextSearchMatch {
        deferred: None,
        line,
        first_line: line,
        last_line: None,
        column: 0,
        byte_offset: 0,
        value: format!("needle {line}"),
        value_digest: [0; 32],
        count: None,
        rank: Some(1),
        window_start: None,
    }
}

/// Cutting every page prices each row once and each file entry once,
/// however many pages a file rides, and every page keeps within its
/// read bound.
#[test]
fn stream_pages_price_each_row_and_file_once() {
    let files = (0..30u32)
        .map(|n| TextSearchFile {
            sort_time: None,
            term_counts: Vec::new(),
            path: format!("src/f{n}.rs"),
            match_count: 40,
            counted: Default::default(),
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
        0,
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
        let row = project_match(&matched, span_columns);
        assert_eq!(row.column.is_some(), span_columns);
        assert_eq!(
            row_chars(&matched, span_columns),
            octocode_engine::json_size::utf16_units(&row).unwrap_or(0) + 1,
            "{span_columns}"
        );
    }
    // A window of a long line states its column in a view of any shape.
    matched.window_start = Some(2);
    for span_columns in [false, true] {
        let row = project_match(&matched, span_columns);
        assert_eq!(row.column, Some(5));
        assert_eq!(
            row_chars(&matched, span_columns),
            octocode_engine::json_size::utf16_units(&row).unwrap_or(0) + 1,
            "{span_columns}"
        );
    }
}

/// A page cut charges a file's names what the reply delivers for them: each
/// declaration as the outline string the response stage lists it by, with
/// its comma, and each row's `enclosing` or `declaration` key. The charge is
/// read from the delivered row, so it follows the form the stage sends.
#[test]
fn names_are_charged_as_the_declarations_the_reply_delivers() {
    use super::super::enclosing::{Outline, annotate_enclosing};
    // Six functions, a non-ASCII name among them: the first three are hit
    // in their bodies (each row names its own declaration around it), the
    // last three on their names (each row names the one it declares).
    let names = [
        "alpha",
        "beta",
        "größe_λ",
        "delta",
        "epsilon_with_a_long_name",
        "zeta",
    ];
    let source = names
        .iter()
        .map(|name| format!("pub fn {name}() {{\n    needle();\n}}\n"))
        .collect::<String>();
    let mut matches = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let first = u32::try_from(index * 3 + 1).expect("line");
        let (line, text) = if index < 3 {
            (first + 1, "    needle();".to_owned())
        } else {
            (first, format!("pub fn {name}() {{"))
        };
        let at = if index < 3 { "needle" } else { name };
        let byte = u32::try_from(text.find(at).expect("hit")).expect("byte");
        matches.push(TextSearchMatch {
            byte_offset: byte,
            value: text,
            ..hit(line)
        });
    }
    let file = TextSearchFile {
        sort_time: None,
        term_counts: Vec::new(),
        path: "src/names.rs".into(),
        match_count: 6,
        counted: Default::default(),
        matches,
        source: None,
    };
    let outline = Outline::of(&source, "price-test/names.rs").expect("rust outline");
    let FilePrice::Owners(owners) = FilePrice::of(&file, Some(&outline), None) else {
        panic!("an outlined file is priced by its owners");
    };
    let charged = owners.values().map(|owner| owner.full).sum::<usize>();

    // The same rows as a page shows them, through the response stage.
    let root = std::path::Path::new("/price-test");
    let mut shown = vec![super::super::types::SearchFile {
        path: file.path.clone(),
        sorted_at: None,
        match_keys: Vec::new(),
        declarations: Vec::new(),
        matches: Some(
            file.matches
                .iter()
                .map(|matched| project_match(matched, false))
                .collect(),
        ),
        total_occurrences: None,
        total_matched_lines: None,
        term_counts: None,
        pagination: None,
        next: None,
        lines: None,
    }];
    let note = annotate_enclosing(
        &mut shown,
        None,
        root,
        &|_| super::super::sources::Expected::Fresh,
        HashMap::from([(root.join(&file.path), Some(outline))]),
        &octocode_runtime_base::security::ContentSecurity::new(),
    );
    assert_eq!(note, None);
    let mut data = serde_json::json!({"files": shown})
        .as_object()
        .expect("data")
        .clone();
    octocode_runtime_base::response::hoist::apply_on(
        octocode_runtime_base::id::ToolId::LocalSearch,
        &mut data,
    );
    let delivered = &data["files"][0];
    let listed = delivered["declarations"].as_array().expect("declarations");
    assert_eq!(listed.len(), names.len(), "{delivered}");
    assert!(
        listed.iter().all(serde_json::Value::is_string),
        "{delivered}"
    );
    let rows = delivered["matches"].as_array().expect("rows");
    let keys = rows
        .iter()
        .flat_map(|row| {
            ["enclosing", "declaration"]
                .into_iter()
                .filter(|key| row.get(*key).is_some())
        })
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), names.len(), "{delivered}");
    let sent = listed
        .iter()
        .map(|entry| octocode_engine::json_size::utf16_units(entry).expect("size") + 1)
        .sum::<usize>()
        + keys
            .iter()
            .map(|key| format!(",\"{key}\":").len())
            .sum::<usize>();
    assert_eq!(charged, sent, "{delivered}");
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
    let paths = octocode_runtime_base::test_support::workspace_policy(root);
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
                    .map(|m| ((m.line, m.byte_offset), Owner { full: 60 }))
                    .collect();
                (index, FilePrice::Owners(Arc::new(owners)))
            })
            .collect())
    };
    let mut pricing = NamePricing::new(known, &mut outline);
    let request = query(serde_json::json!({"page": page}));
    let layout = Layout::cut(
        &request,
        &paths,
        root,
        &mut parsed,
        6_000,
        (&mut pricing, None, None),
    )
    .expect("cut");
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
            sort_time: None,
            term_counts: Vec::new(),
            path: format!("src/f{n:03}.rs"),
            match_count: 30,
            counted: Default::default(),
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
    let mut value = json_fixture(r#"{"path": ".", "matchString": ["x"]}"#);
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
    assert!(shows_span_columns(&query(json_fixture(
        r#"{"view": "matchOnly"}"#
    ))));
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
                sort_time: None,
                term_counts: terms.clone(),
                path: path.to_owned(),
                match_count: count,
                counted: Default::default(),
                matches: Vec::new(),
                source: None,
            };
            for view in [View::Files, View::CountMatches, View::CountLines] {
                let counted = matches!(view, View::CountMatches | View::CountLines);
                let rendered = SearchFile {
                    sorted_at: None,
                    path: file.path.clone(),
                    declarations: Vec::new(),
                    matches: None,
                    match_keys: Vec::new(),
                    total_occurrences: (view == View::CountMatches).then_some(count),
                    total_matched_lines: (view == View::CountLines).then_some(count),
                    term_counts: (counted && !terms.is_empty()).then(|| terms.clone()),
                    pagination: None,
                    next: None,
                    lines: None,
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

/// A page that shows everything still leaves out the first file's rows an
/// earlier page showed.
#[test]
fn a_whole_page_skips_the_rows_an_earlier_page_showed() {
    let files = (0..3u32)
        .map(|n| TextSearchFile {
            sort_time: None,
            term_counts: Vec::new(),
            path: format!("src/f{n}.rs"),
            match_count: 5,
            counted: Default::default(),
            matches: (1..=5).map(|line| hit(line * 3)).collect(),
            source: None,
        })
        .collect::<Vec<_>>();
    let pages = stream_pages(
        &files,
        u32::MAX as usize,
        true,
        (usize::MAX, None),
        StreamCosts {
            file: &|_| 1,
            row: &|_| 1,
            name: &|_, _| RowName::None,
        },
        None,
        3,
    );
    assert_eq!(pages, vec![vec![(0, 3..5), (1, 0..5), (2, 0..5)]]);
}
