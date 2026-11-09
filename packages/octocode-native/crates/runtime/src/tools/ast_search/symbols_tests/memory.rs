//! A directory outline holds at most what the process's memory budget allows:
//! past it the scan stops where it stands and `next.expandScan` reads on, so a
//! server answers with rows it can hold, and no row is lost or repeated.
use crate::tools::ast_search::scan_window::test_memory_limit;
use crate::tools::ast_search::test_rows::run;
use crate::tools::test_support::Fixture;
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};
use std::collections::BTreeMap;

const FILES: usize = 64;

fn fixture() -> Fixture {
    let root = Fixture::new();
    for index in 0..FILES {
        root.write(
            &format!("file{index:02}.ts"),
            format!("export function helper{index}() {{}}\nexport class Widget{index} {{}}\n"),
        );
    }
    root
}

fn query(root: &Fixture) -> Value {
    json!({"operation":"symbols","path":root.0})
}

fn names(out: &Value) -> Vec<String> {
    out["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|file| file["symbols"].as_array().into_iter().flatten())
        .filter_map(|symbol| symbol["symbolName"].as_str().map(str::to_owned))
        .collect()
}

fn lead(out: &Value) -> Option<Value> {
    out["next"]
        .get("nextPage")
        .or_else(|| out["next"].get("expandScan"))
        .map(|lead| lead["input"]["queries"][0].clone())
}

/// Every window and page of `start`, in order.
fn walk(root: &Fixture, start: Value) -> Vec<Value> {
    let mut pages = Vec::new();
    let mut next = Some(start);
    while let Some(row) = next {
        assert!(pages.len() < 500, "the scan never finishes: {row}");
        let out = run(&root.0, row).expect("page");
        next = lead(&out);
        pages.push(out);
    }
    pages
}

/// Each declaration once, whatever the budget.
fn every_name_once(pages: &[Value]) -> BTreeMap<String, usize> {
    let mut seen = BTreeMap::new();
    for name in pages.iter().flat_map(names) {
        *seen.entry(name).or_insert(0) += 1;
    }
    assert_eq!(seen.len(), FILES * 2, "every declaration: {seen:?}");
    assert!(seen.values().all(|count| *count == 1), "once: {seen:?}");
    seen
}

#[test]
fn a_scan_past_its_memory_budget_stops_and_every_row_arrives_once() {
    let root = fixture();
    let whole = every_name_once(&walk(&root, query(&root)));
    let _limit = test_memory_limit(1);
    let first = run(&root.0, query(&root)).expect("first window");
    assert_eq!(first["partialReasons"], json!(["scanBudget"]), "{first}");
    assert!(first.get("terminalLimit").is_none(), "{first}");
    assert!(first["next"]["expandScan"].is_object(), "{first}");
    assert!(
        first["filesScanned"]
            .as_u64()
            .is_some_and(|files| (files as usize) < FILES),
        "the budget stopped the scan early: {first}"
    );
    let walked = walk(&root, query(&root));
    assert!(walked.len() > 1, "{walked:?}");
    assert_eq!(every_name_once(&walked), whole);
    let last = walked.last().expect("a window");
    assert!(last["next"].get("expandScan").is_none(), "{last}");
    assert!(last.get("partialReasons").is_none(), "{last}");
}

/// Files just written are never stored, so every page of a window scans again: it
/// must stop at the same file, or the page is stale and a restart repeats it. Each
/// budget and page size reaches every row exactly once through `next.*` alone.
#[test]
fn every_page_of_a_stopped_window_is_reachable_with_nothing_stored() {
    for (limit, rows) in [(1, Some(1)), (5_000, Some(3)), (40_000, None)] {
        let root = fixture();
        let _limit = test_memory_limit(limit);
        let mut row = query(&root);
        if let Some(rows) = rows {
            row["limit"] = json!(rows);
        }
        let walked = walk(&root, row);
        assert!(
            walked.iter().all(|page| page.get("errorCode").is_none()),
            "{limit} {rows:?}: {:?}",
            walked.iter().find(|page| page.get("errorCode").is_some())
        );
        every_name_once(&walked);
    }
}

/// Where a window stops is a fact of the files: the same on every run.
#[test]
fn a_memory_stop_lands_on_the_same_file_every_time() {
    let root = fixture();
    let _limit = test_memory_limit(5_000);
    let stop = || {
        let out = run(&root.0, query(&root)).expect("window");
        (
            out["filesScanned"].clone(),
            out["next"]["expandScan"]["input"]["queries"][0]["scanOffset"].clone(),
            crate::tools::test_support::pinned_snapshot(&out).clone(),
        )
    };
    let first = stop();
    for _ in 0..20 {
        assert_eq!(stop(), first);
    }
}

/// A budget smaller than one file's rows still reads that file: every window
/// makes progress, however small the budget.
#[test]
fn a_single_file_past_the_budget_still_answers() {
    let root = Fixture::new();
    root.write("only.ts", "export function only() {}\n");
    let _limit = test_memory_limit(1);
    let out = run(&root.0, query(&root)).expect("window");
    assert_eq!(names(&out), ["only"], "{out}");
    assert!(out["next"].get("expandScan").is_none(), "{out}");
}

#[test]
fn a_scan_within_its_memory_budget_is_not_cut() {
    let root = fixture();
    let _limit = test_memory_limit(1 << 30);
    let pages = walk(&root, query(&root));
    every_name_once(&pages);
    assert!(
        pages
            .iter()
            .all(|page| page["next"].get("expandScan").is_none()),
        "{pages:?}"
    );
    assert!(pages[0].get("partialReasons").is_none(), "{}", pages[0]);
}

/// An edit by name.
type Edit = (&'static str, fn(&Fixture));

/// The window a memory stop ends is pinned like one a clock ends.
#[test]
fn an_edit_across_a_memory_stop_restarts_the_scan() {
    let edits: [Edit; 3] = [
        ("removed", |root| {
            std::fs::remove_file(root.0.join("file00.ts")).expect("remove");
        }),
        ("inserted", |root| {
            root.write("a.ts", "export function inserted() {}\n");
        }),
        ("edited", |root| {
            root.write("file00.ts", "export function edited() {}\n");
        }),
    ];
    for (name, edit) in edits {
        let root = fixture();
        let _limit = test_memory_limit(1);
        let first = run(&root.0, query(&root)).expect("first window");
        let next = first["next"]["expandScan"]["input"]["queries"][0].clone();
        assert!(next.is_object(), "{first}");
        std::thread::sleep(std::time::Duration::from_millis(20));
        edit(&root);
        let second = run(&root.0, next).expect("next window");
        assert_eq!(
            second["errorCode"],
            json!(ErrorCode::StaleSnapshot),
            "{name}: {second}"
        );
    }
}

/// The outline's snapshot hashes rows as they serialize, with no copy of them:
/// the same bytes as the one array value they used to be built into, so an
/// unchanged walk's snapshot does not move.
#[test]
fn a_tuple_hashes_as_the_array_of_its_fields() {
    let rows = vec![json!({"name":"a","members":[{"name":"b","line":3}],"kind":"class"})];
    let query = serde_json::from_value::<octocode_contract_types::AstSearchQuerySymbols>(
        json!({"operation":"symbols","path":"src","symbolFilter":{"terms":["a"]},"parser":"rust"}),
    )
    .expect("query");
    let (path, none, offset) = ("src", None::<String>, 3_u32);
    assert_eq!(
        crate::digest::json_sha256(&(
            path,
            &query.parser,
            &none,
            &query.symbol_filter,
            offset,
            &query.test_code,
            &rows,
            true,
        )),
        crate::digest::json_sha256(&json!([
            path,
            query.parser,
            none,
            query.symbol_filter,
            offset,
            query.test_code,
            &rows,
            true,
        ])),
    );
}
