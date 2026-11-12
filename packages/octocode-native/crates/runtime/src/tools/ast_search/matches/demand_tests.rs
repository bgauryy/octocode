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
    let scan = scan_directory(q, &scope, paths, &NeverCancel, None).expect("scan");
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

/// Every page of a priced scan carries the rows, sizes and page cut the same
/// scan holding every match would: the page rebuilt from the file is the page
/// the held matches cut.
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
                            (
                                crate::security::SourceLimits::REAL.whole_source as u64,
                                Some(&super::super::source::judge()),
                            ),
                            &|| false,
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
                    recovered_bodies: file.recovered_bodies.clone(),
                    source: None,
                }
            })
            .collect::<Vec<_>>();
        let pages = stream::stream_page(
            q,
            &group_files(q, &priced.files, row_paths, &NeverCancel)
                .expect("grouped")
                .groups,
            budget,
        )
        .pages;
        assert!(budget > 2_000 || pages > 1, "budget {budget} cuts pages");
        for page in 1..=pages {
            let mut cursor = row.clone();
            cursor["page"] = json!(page);
            let query = typed(cursor);
            let q = matched(&query);
            let mut from_prices =
                group_files(q, &priced.files, row_paths, &NeverCancel).expect("grouped");
            let mut expected = group_files(q, &held, row_paths, &NeverCancel).expect("grouped");
            let cut = stream::stream_page(q, &from_prices.groups, budget);
            let held_cut = stream::stream_page(q, &expected.groups, budget);
            assert_eq!(
                (&cut.window, &cut.slices, &cut.pagination),
                (&held_cut.window, &held_cut.slices, &held_cut.pagination)
            );
            for (groups, files) in [
                (&mut from_prices.groups, &priced.files),
                (&mut expected.groups, &held),
            ] {
                assert!(
                    fill_page(
                        q,
                        &scope,
                        files,
                        (&mut groups[cut.window.clone()], &cut.slices),
                        row_paths,
                        None,
                        &NeverCancel,
                    )
                    .is_ok_and(|incomplete| incomplete.is_empty())
                );
            }
            let shown = |groups: &[Group]| {
                groups
                    .iter()
                    .map(|group| {
                        (
                            group.row.clone(),
                            group.chars,
                            group.sizes.clone(),
                            group.truncated_captures,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                shown(&from_prices.groups[cut.window.clone()]),
                shown(&expected.groups[cut.window]),
                "budget {budget} page {page}"
            );
            assert_eq!(from_prices.total_matches, expected.total_matches);
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
    let mut grouped = group_files(q, &scan.files, row_paths, &NeverCancel).expect("grouped");
    let cut = stream::stream_page(q, &grouped.groups, budget);
    std::fs::write(root.0.join("src/f0.rs"), "fn z() {}\n").expect("edit");
    let groups = &mut grouped.groups[cut.window];
    assert!(
        fill_page(
            q,
            &scope,
            &scan.files,
            (groups, &cut.slices),
            row_paths,
            None,
            &NeverCancel
        )
        .is_err()
    );
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
    // Fit two complete files including their source correspondence; this test
    // exercises rebuilding an unevaluated file beside a healthy sibling.
    let budget = scan
        .files
        .iter()
        .take(2)
        .map(|file| match &file.matches {
            octocode_engine::structural::FileMatches::Priced(prices) => {
                prices.iter().map(|price| *price as usize).sum::<usize>() + 4096
            }
            _ => panic!("fixture must use priced matching"),
        })
        .sum();
    let (source, digest) = scan.files[0].source.clone().expect("scanned source");
    let renamed = source.with_extension("unparsed");
    std::fs::copy(&source, &renamed).expect("same bytes");
    scan.files[0].source = Some((renamed, digest));
    let row_paths = RowPaths::new(&paths, &scope.canonical, scope.is_file);
    let mut grouped = group_files(q, &scan.files, row_paths, &NeverCancel).expect("grouped");
    let cut = stream::stream_page(q, &grouped.groups, budget);
    let window = cut.window.clone();
    assert!(window.contains(&0) && window.len() > 1, "{window:?}");
    let Ok(incomplete) = fill_page(
        q,
        &scope,
        &scan.files,
        (&mut grouped.groups[window.clone()], &cut.slices),
        row_paths,
        None,
        &NeverCancel,
    ) else {
        panic!("unchanged bytes are not a changed file");
    };
    assert_eq!(incomplete.len(), 1);
    assert_eq!(incomplete[0][0].code, "structural.language.unsupported");
    let unevaluated = &grouped.groups[0];
    assert_eq!(unevaluated.row, json!({"path":scan.files[0].path}));
    assert!(unevaluated.priced.is_none() && !unevaluated.sizes.is_empty());
    assert!(
        grouped.groups[1..window.end]
            .iter()
            .all(|group| group.priced.is_none() && group.row.get("matches").is_some())
    );
}
