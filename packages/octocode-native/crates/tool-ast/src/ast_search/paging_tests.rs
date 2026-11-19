//! astSearch pages by bytes: with no count asked, a page fills the response
//! window (never past it, so the response layer never splits it), a walk
//! reaches every item exactly once, and totals agree on every page. An
//! explicit count is honored exactly.
use super::{AstSearchQuery, execute_ast};
use octocode_runtime_base::execution::cancel::NeverCancel;

use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::test_support::{Fixture, checks, opened_input, simple_policy};
use serde_json::{Value, json};
use std::collections::BTreeSet;

const WINDOW: usize = 16_384;

/// Runs `row` with a response window and walks every page lead
/// (`checks::walk`: each call once, every output and lead within the
/// contract). `rows` names the items a page delivers, each exactly once
/// across the walk. Returns the raw pages in the order reached.
pub(super) fn walk(
    root: &std::path::Path,
    row: Value,
    rows: fn(&Value) -> Vec<String>,
) -> Vec<Value> {
    walk_within(root, row, rows, WINDOW)
}

/// [`walk`] under a response window of `window` characters.
pub(super) fn walk_within(
    root: &std::path::Path,
    row: Value,
    rows: fn(&Value) -> Vec<String>,
    window: usize,
) -> Vec<Value> {
    let (paths, security) = simple_policy(root);
    let mut outputs = Vec::new();
    checks::walker::walk("astSearch", json!({"queries":[row]}), |call| {
        let input = opened_input(ToolId::AstSearch, call);
        let row = &input["queries"][0];
        let query: AstSearchQuery = serde_json::from_value(row.clone()).expect("typed row");
        let out = execute_ast(
            octocode_runtime_base::test_support::BUILD_IDENTITY,
            &query,
            &paths,
            &security,
            &NeverCancel,
            Some(window),
        )
        .unwrap_or_else(|error| panic!("{row} failed: {error:?}"));
        // Every page states `pagination.hasMore`: the response layer and
        // clasify read the raw page before any shaping.
        if let Some(pagination) = out.get("pagination") {
            assert!(pagination["hasMore"].is_boolean(), "{out}");
        }
        let page = checks::Page {
            output: octocode_runtime_base::test_support::published(
                octocode_runtime_base::id::ToolId::AstSearch,
                &out,
            ),
            rows: rows(&out),
        };
        outputs.push(out);
        page
    });
    outputs
}

/// Every node row of a syntax-tree page.
fn node_rows(out: &Value) -> Vec<String> {
    out["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(Value::to_string)
        .collect()
}

/// Every match of a match page, as `path line:column`.
pub(super) fn match_rows(out: &Value) -> Vec<String> {
    match_cells(out)
        .into_iter()
        .map(|(path, line, column)| format!("{path} {line}:{column}"))
        .collect()
}

/// A single complete page carries a valid `pagination`.
#[test]
fn a_single_symbols_page_states_its_pagination() {
    let root = Fixture::new();
    root.write("a.ts", numbered_functions(3));
    let outputs = walk(
        &root.0,
        json!({"operation":"symbols","path":root.0.to_string_lossy()}),
        symbol_rows,
    );
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0]["pagination"]["hasMore"], false, "{}", outputs[0]);
}

pub(super) fn numbered_functions(count: usize) -> String {
    (0..count)
        .map(|index| format!("export function declaredName{index}(value: number): number {{ return value + {index}; }}\n"))
        .collect()
}

/// Every symbol row of a symbols page, in the shape the page carries them.
pub(super) fn symbol_rows(out: &Value) -> Vec<String> {
    let rows = |symbols: &Value| {
        symbols
            .as_array()
            .into_iter()
            .flatten()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
    };
    match out.get("files").and_then(Value::as_array) {
        Some(files) => files
            .iter()
            .flat_map(|file| {
                let path = file["path"].to_string();
                rows(&file["symbols"])
                    .into_iter()
                    .map(move |row| format!("{path} {row}"))
            })
            .collect(),
        None => rows(&out["symbols"]),
    }
}

/// A raw page as the hoist stage publishes it
/// ([`octocode_runtime_base::response::hoist::apply_on`]).
pub(super) fn stated(out: &Value) -> Value {
    let mut data = out.as_object().cloned().unwrap_or_default();
    octocode_runtime_base::response::hoist::apply_on(ToolId::AstSearch, &mut data);
    Value::Object(data)
}

#[test]
fn symbol_pages_fill_the_window_and_walk_every_symbol_once() {
    let root = Fixture::new();
    root.write("big.ts", numbered_functions(1_500));
    let outputs = walk(
        &root.0,
        json!({"operation":"symbols","path":root.0.join("big.ts")}),
        symbol_rows,
    );
    let mut all = Vec::new();
    for (index, out) in outputs.iter().enumerate() {
        // The page as the hoist stage publishes it: its outline as strings
        // (Q5), the form its rows are priced in.
        let published = stated(out);
        let chars = octocode_engine::json_size::utf16_units(&published).unwrap_or(0);
        assert!(chars <= WINDOW, "page {index} is {chars} chars");
        // Symbol evidence fills every page but the last. Measure the payload,
        // so wrapper metadata cannot disguise an underfilled page and added
        // provenance does not impose an obsolete fixed row-count assumption.
        if out["pagination"]["hasMore"] == true {
            let evidence_chars = published["symbols"]
                .as_array()
                .expect("symbol evidence")
                .iter()
                .map(|row| octocode_engine::json_size::utf16_units(row).expect("row size"))
                .sum::<usize>();
            assert!(
                evidence_chars * 10 >= WINDOW * 6,
                "page {index} holds only {evidence_chars} evidence chars"
            );
        }
        all.extend(symbol_rows(out));
    }
    let unique = all.iter().collect::<BTreeSet<_>>();
    assert_eq!(all.len(), 1_500, "{} rows", all.len());
    assert_eq!(unique.len(), 1_500, "a symbol was repeated");
    let totals = outputs
        .iter()
        .map(|out| out["pagination"]["total"].as_u64())
        .collect::<BTreeSet<_>>()
        .len();
    assert_eq!(totals, 1, "totals differ across pages");
}

/// A directory page states each file's path once, so a row is measured
/// without it: pages of many small files under a long path fill the window
/// as a single file's pages do.
#[test]
fn directory_symbol_pages_measure_rows_as_delivered() {
    // Only the scan's own budget may cut it, never a loaded machine's clock.
    let _frozen = octocode_engine::scan_budget::freeze_walk_clock();
    let root = Fixture::new();
    for index in 0..300 {
        root.write(
            &format!("packages/some-long-package-name/src/modules/file_{index:03}.ts"),
            format!("export function a{index}() {{}}\nexport function b{index}() {{}}\n"),
        );
    }
    let outputs = walk(
        &root.0,
        json!({"operation":"symbols","path":root.0.join("packages")}),
        symbol_rows,
    );
    let mut all = Vec::new();
    for (index, out) in outputs.iter().enumerate() {
        let rows = symbol_rows(out);
        assert!(!rows.is_empty(), "page {index} carries evidence");
        all.extend(rows);
    }
    assert_eq!(all.len(), 600, "{} rows", all.len());
    assert_eq!(all.iter().collect::<BTreeSet<_>>().len(), 600);
}

/// Match rows of one page: `(path, line, column)` of each.
fn match_cells(out: &Value) -> Vec<(String, u64, u64)> {
    out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| {
            let path = file["path"].as_str().unwrap_or_default().to_owned();
            file["matches"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |hit| {
                    (
                        path.clone(),
                        hit["line"].as_u64().unwrap_or(0),
                        hit["column"].as_u64().unwrap_or(0),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn a_file_of_three_thousand_matches_pages_by_bytes_and_reaches_each_once() {
    let root = Fixture::new();
    let source = (0..3_000)
        .map(|index| format!("hit({index});\n"))
        .collect::<String>();
    root.write("many.ts", source);
    let outputs = walk(
        &root.0,
        json!({"operation":"match","rule":{"pattern":"hit($A)"},"path":root.0.join("many.ts"),"parser":"typescript"}),
        match_rows,
    );
    let mut cells = Vec::new();
    for (index, out) in outputs.iter().enumerate() {
        let rows = match_cells(out);
        assert!(!rows.is_empty(), "page {index} carries evidence");
        cells.extend(rows);
    }
    assert_eq!(cells.len(), 3_000, "{} matches", cells.len());
    assert_eq!(
        cells.iter().collect::<BTreeSet<_>>().len(),
        3_000,
        "a match was repeated"
    );
    // Every row now includes its exact origin; byte filling, rather than a
    // legacy fixed row count, bounds the number of calls.
    for out in &outputs[..outputs.len() - 1] {
        let chars = octocode_engine::json_size::utf16_units(out).unwrap_or(0);
        assert!(
            chars * 10 >= WINDOW * 5,
            "a page of {chars} chars left the window half empty"
        );
    }
}

/// Pages of many one-match files fill the window, and each states the
/// scope's total, or a floor of it (`countScope: loaded`) on a window that
/// ends before the scope does or follows another.
#[test]
fn many_files_page_by_bytes_each_with_its_total_or_a_floor() {
    let root = Fixture::new();
    for index in 0..300 {
        root.write(
            &format!("file{index:03}.ts"),
            format!("export const value{index} = hit({index});\n"),
        );
    }
    let outputs = walk(
        &root.0,
        json!({"operation":"match","rule":{"pattern":"hit($A)"},"path":root.0,"parser":"typescript"}),
        match_rows,
    );
    let mut cells = Vec::new();
    for (index, out) in outputs.iter().enumerate() {
        let rows = match_cells(out);
        assert!(!rows.is_empty(), "page {index} carries evidence");
        cells.extend(rows);
    }
    assert_eq!(cells.iter().collect::<BTreeSet<_>>().len(), 300);
    assert_eq!(cells.len(), 300);
    for out in &outputs {
        let pagination = &out["pagination"];
        let total = pagination["total"].as_u64().expect("a total");
        match pagination.get("countScope") {
            Some(scope) => assert!(scope == "loaded" && total <= 300, "{pagination}"),
            None => assert_eq!(total, 300, "{pagination}"),
        }
    }
    for out in &outputs[..outputs.len() - 1] {
        let chars = octocode_engine::json_size::utf16_units(out).unwrap_or(0);
        assert!(
            chars >= WINDOW / 2,
            "a page of {chars} chars left the window half empty"
        );
    }
}

#[test]
fn a_syntax_tree_pages_by_bytes_and_reaches_every_node_once() {
    let root = Fixture::new();
    root.write("tree.ts", numbered_functions(300));
    let outputs = walk(
        &root.0,
        json!({"operation":"syntaxTree","path":root.0.join("tree.ts")}),
        node_rows,
    );
    let mut ids = Vec::new();
    for (index, out) in outputs.iter().enumerate() {
        let chars = octocode_engine::json_size::utf16_units(out).unwrap_or(0);
        assert!(chars <= WINDOW, "page {index} is {chars} chars");
        ids.extend(
            out["nodes"]
                .as_array()
                .into_iter()
                .flatten()
                .map(std::string::ToString::to_string),
        );
    }
    let total = outputs[0]["pagination"]["total"].as_u64().expect("total") as usize;
    assert_eq!(ids.len(), total);
    assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), total);
    for out in &outputs[..outputs.len() - 1] {
        let chars = octocode_engine::json_size::utf16_units(out).unwrap_or(0);
        assert!(
            chars >= WINDOW / 2,
            "a page of {chars} chars left the window half empty"
        );
    }
}

/// A later window-sized page reads on from the walk its first page kept
/// (no parse or walk from the first node again), and a page past the last
/// is the shared out-of-range outcome with the page count, never an empty
/// success.
#[test]
fn a_syntax_tree_page_reuses_its_walk_and_past_the_end_is_out_of_range() {
    let root = Fixture::new();
    let file = root.0.join("tree.ts");
    root.write("tree.ts", numbered_functions(300));
    let (paths, security) = simple_policy(&root.0);
    let run = |row: Value| {
        let query: AstSearchQuery = serde_json::from_value(row).expect("typed row");
        execute_ast(
            octocode_runtime_base::test_support::BUILD_IDENTITY,
            &query,
            &paths,
            &security,
            &NeverCancel,
            Some(WINDOW),
        )
        .expect("tree")
    };
    let first = run(json!({"operation":"syntaxTree","path":file}));
    let total = first["pagination"]["total"].as_u64().expect("total");
    let length = first["pagination"]["length"]
        .as_u64()
        .expect("length")
        .max(1);
    let pages = total.div_ceil(length);
    assert!(pages > 2, "{}", first["pagination"]);
    let next = first["next"]["nextPage"]["input"]["queries"][0].clone();
    let inspected = super::syntax::inspections();
    let second = run(next.clone());
    assert_eq!(
        super::syntax::inspections(),
        inspected,
        "page 2 re-read the tree"
    );
    assert_eq!(second["pagination"]["offset"], length, "{second}");
    let mut past = next;
    past["page"] = json!(pages + 5);
    let out = run(past);
    assert!(out.get("errorCode").is_none(), "{out}");
    assert_eq!(out["nodes"], json!([]), "{out}");
    assert_eq!(out["pagination"]["outOfRange"], true, "{out}");
    assert_eq!(out["pagination"]["total"], total, "{out}");
}

/// Every match row of a page, by file and line.
fn match_lines(out: &Value) -> Vec<String> {
    out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| {
            let path = file["path"].as_str().unwrap_or_default().to_owned();
            file["matches"]
                .as_array()
                .into_iter()
                .flatten()
                .map(move |hit| format!("{path}:{}", hit["line"]))
        })
        .collect()
}

/// A match walk with matches states no prefilter count (it decides no next
/// call); a scan with none states it once, on its first page.
#[test]
fn a_match_walk_with_matches_states_no_prefilter_count() {
    let root = Fixture::new();
    for index in 0..6 {
        root.write(
            &format!("hit{index}.rs"),
            "fn a() { hit(1); }\nfn b() { hit(2); }\n",
        );
    }
    for index in 0..3 {
        root.write(&format!("miss{index}.rs"), "fn c() {}\n");
    }
    let pages = walk(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"hit($A)"},"limit":3}),
        match_lines,
    );
    assert!(pages.len() > 1, "pages: {pages:?}");
    for page in &pages {
        let notes = page["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|diagnostic| diagnostic["code"] == "structural.prefilter.skipped")
            .count();
        assert_eq!(notes, 0, "{page}");
    }
}

/// astSearch has one engine: no match page states it.
#[test]
fn a_match_page_states_no_engine_name() {
    let root = Fixture::new();
    for index in 0..4 {
        root.write(&format!("hit{index}.rs"), "fn a() { hit(1); }\n");
    }
    let pages = walk(
        &root.0,
        json!({"operation":"match","path":root.0,"parser":"rust","rule":{"pattern":"hit($A)"},"limit":2}),
        match_lines,
    );
    assert!(pages.len() > 1, "pages: {pages:?}");
    for page in &pages {
        assert!(page.get("searchEngine").is_none(), "{page}");
    }
}
