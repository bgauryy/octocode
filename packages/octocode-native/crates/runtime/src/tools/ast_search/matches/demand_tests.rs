use super::super::RowPaths;
use super::scan::{parser_extension, scan_directory};
use super::types::{Group, Scan, ScannedFile, Scope};
use super::*;
use crate::execution::cancel::NeverCancel;
use crate::tools::test_support::{Fixture, simple_policy};
use octocode_contract_types::AstSearchQuery;
use octocode_engine::structural::FileMatches;
use serde_json::{Value, json};

/// A nested Rust fixture where `$A` matches every named node.
fn nested() -> Fixture {
    let root = Fixture::new();
    for file in 0..4 {
        let body = (0..6)
            .map(|n| format!("fn f{n}() {{ let x = g(h({n}), k(m({file}))); }}\n"))
            .collect::<String>();
        root.write(&format!("src/f{file}.rs"), body);
    }
    root
}

fn typed(row: Value) -> AstSearchQuery {
    serde_json::from_value(row).expect("canonical match row")
}

fn matched(row: &AstSearchQuery) -> MatchQuery<'_> {
    match row {
        AstSearchQuery::Match0(query) => MatchQuery::Content(query),
        AstSearchQuery::MatchView(query) => MatchQuery::Summary(query),
        _ => panic!("a match row"),
    }
}

fn scan_of(q: MatchQuery<'_>, paths: &PathPolicy) -> (Scope, Scan) {
    let scope = resolve_scope(q, paths, &NeverCancel).expect("scope");
    let scan = scan_directory(q, &scope, paths, &NeverCancel).expect("scan");
    (scope, scan)
}

/// A file view's scan keeps only counts and a match view's only prices:
/// neither keeps match text, and both count every match.
#[test]
fn a_scan_keeps_only_what_its_view_shows() {
    let root = nested();
    let (paths, _) = simple_policy(&root.0);
    let mut totals = Vec::new();
    for view in ["files", "countMatches", "content"] {
        let query = typed(
            json!({"operation":"match","path":root.0,"rule":{"pattern":"$A"},"parser":"rust","view":view}),
        );
        let q = matched(&query);
        let (_, scan) = scan_of(q, &paths);
        for file in &scan.files {
            assert!(file.matches.held().is_empty(), "{view}: {}", file.path);
            let kept = match &file.matches {
                FileMatches::Counted(_) => "count",
                FileMatches::Priced(_) => "price",
                FileMatches::Held(_) => "held",
            };
            let expected = if view == "content" { "price" } else { "count" };
            assert_eq!(kept, expected, "{view}");
        }
        totals.push(
            scan.files
                .iter()
                .map(|file| file.matches.len())
                .sum::<usize>(),
        );
    }
    assert!(totals[0] > 100, "{totals:?}");
    assert!(totals.iter().all(|total| *total == totals[0]), "{totals:?}");
}

/// Every file page and match page of a priced scan carries the rows, sizes
/// and page facts the same scan holding every match would: the page rebuilt
/// from the file is the page the held matches cut.
#[test]
fn a_priced_scan_pages_exactly_as_a_held_scan() {
    let root = nested();
    let (paths, _) = simple_policy(&root.0);
    for budget in [600, 2_000, 100_000] {
        let row =
            json!({"operation":"match","path":root.0,"rule":{"pattern":"$A"},"parser":"rust"});
        let first = typed(row.clone());
        let q = matched(&first);
        let (scope, priced) = scan_of(q, &paths);
        let row_paths = RowPaths::new(&paths, &scope.canonical, scope.is_file);
        let held = priced
            .files
            .iter()
            .map(|file| {
                let (source, digest) = file.source.clone().expect("scanned source");
                ScannedFile {
                    path: file.path.clone(),
                    parser: file.parser.clone(),
                    matches: FileMatches::Held(
                        match octocode_engine::structural::file_matches(
                            &source,
                            &parser_extension(&scope, &source),
                            q.pattern_text().as_deref(),
                            None,
                            &digest,
                            super::super::source::MAX_PARSE_SOURCE_BYTES as u64,
                        )
                        .expect("rebuild")
                        {
                            octocode_engine::structural::Rebuilt::Matches(matches) => matches,
                            _ => panic!("unchanged: {}", file.path),
                        },
                    ),
                    diagnostics: file.diagnostics.clone(),
                    status: file.status.clone(),
                    recovered: file.recovered,
                    source: None,
                }
            })
            .collect::<Vec<_>>();
        let pages = file_page(
            q,
            &group_files(
                q,
                &priced.files,
                budget,
                row_paths,
                &Inapplicable::default(),
            )
            .groups,
            budget,
        );
        let file_pages = match pages.pagination["totalPages"].as_u64() {
            Some(total) => total as u32,
            None => 1,
        };
        for page in 1..=file_pages {
            for match_page in 1..=4 {
                let mut cursor = row.clone();
                cursor["page"] = json!(page);
                cursor["matchPage"] = json!(match_page);
                let query = typed(cursor);
                let q = matched(&query);
                let mut from_prices = group_files(
                    q,
                    &priced.files,
                    budget,
                    row_paths,
                    &Inapplicable::default(),
                );
                let expected = group_files(q, &held, budget, row_paths, &Inapplicable::default());
                let window = file_page(q, &from_prices.groups, budget).window;
                assert_eq!(window, file_page(q, &expected.groups, budget).window);
                assert!(
                    rebuild_page(
                        q,
                        &scope,
                        &priced.files,
                        &mut from_prices.groups[window.clone()],
                        budget,
                        row_paths,
                    )
                    .is_ok_and(|incomplete| incomplete.is_empty())
                );
                let shown = |groups: &[Group]| {
                    groups
                        .iter()
                        .map(|group| {
                            (
                                group.row.clone(),
                                group.chars,
                                group.out_of_range,
                                group.more_matches,
                                group.truncated_captures,
                                group.clipped.clone(),
                            )
                        })
                        .collect::<Vec<_>>()
                };
                assert_eq!(
                    shown(&from_prices.groups[window.clone()]),
                    shown(&expected.groups[window]),
                    "budget {budget} page {page} matchPage {match_page}"
                );
                assert_eq!(from_prices.total_matches, expected.total_matches);
            }
        }
    }
}

/// A priced file edited after its scan is never shown from its new bytes:
/// its page cannot be rebuilt.
#[test]
fn a_priced_file_edited_after_its_scan_rebuilds_no_page() {
    let root = nested();
    let (paths, _) = simple_policy(&root.0);
    let query =
        typed(json!({"operation":"match","path":root.0,"rule":{"pattern":"$A"},"parser":"rust"}));
    let q = matched(&query);
    let (scope, scan) = scan_of(q, &paths);
    let budget = 100_000;
    let row_paths = RowPaths::new(&paths, &scope.canonical, scope.is_file);
    let mut grouped = group_files(q, &scan.files, budget, row_paths, &Inapplicable::default());
    let window = file_page(q, &grouped.groups, budget).window;
    std::fs::write(root.0.join("src/f0.rs"), "fn z() {}\n").expect("edit");
    let groups = &mut grouped.groups[window];
    assert!(rebuild_page(q, &scope, &scan.files, groups, budget, row_paths).is_err());
}

/// A shown priced file whose unchanged bytes do not finish matching on its
/// page (an execution limit; here, no grammar for them under the name they
/// are read by) is shown without rows, its diagnostics returned to say why,
/// rather than reported as changed: a fresh page over an unchanged file
/// never restarts.
#[test]
fn a_priced_file_whose_match_does_not_finish_is_shown_unevaluated() {
    let root = nested();
    let (paths, _) = simple_policy(&root.0);
    let query =
        typed(json!({"operation":"match","path":root.0,"rule":{"pattern":"$A"},"parser":"rust"}));
    let q = matched(&query);
    let (scope, mut scan) = scan_of(q, &paths);
    let budget = 100_000;
    let (source, digest) = scan.files[0].source.clone().expect("scanned source");
    let renamed = source.with_extension("unparsed");
    std::fs::copy(&source, &renamed).expect("same bytes");
    scan.files[0].source = Some((renamed, digest));
    let row_paths = RowPaths::new(&paths, &scope.canonical, scope.is_file);
    let mut grouped = group_files(q, &scan.files, budget, row_paths, &Inapplicable::default());
    let window = file_page(q, &grouped.groups, budget).window;
    assert!(window.contains(&0) && window.len() > 1, "{window:?}");
    let Ok(incomplete) = rebuild_page(
        q,
        &scope,
        &scan.files,
        &mut grouped.groups[window.clone()],
        budget,
        row_paths,
    ) else {
        panic!("unchanged bytes are not a changed file");
    };
    assert_eq!(incomplete.len(), 1);
    assert_eq!(incomplete[0][0].code, "structural.language.unsupported");
    let unevaluated = &grouped.groups[0];
    assert_eq!(unevaluated.row, json!({"path":scan.files[0].path}));
    assert!(unevaluated.priced.is_none() && !unevaluated.more_matches);
    assert!(
        grouped.groups[1..window.end]
            .iter()
            .all(|group| group.priced.is_none() && group.row.get("matches").is_some())
    );
}
