//! Timings of the walks a retest measured, printed for a before/after
//! comparison (`cargo dev-test -- --ignored --nocapture walk_timings`). They
//! assert no duration: the scan counts beside them are the invariant.
use super::*;

/// Walks `start` under `root` and returns (first page, whole walk, pages,
/// rows): the time spent inside the tool, without the walker's checks.
fn timed_walk(
    root: &std::path::Path,
    start: serde_json::Value,
) -> (std::time::Duration, std::time::Duration, usize, usize) {
    let (policy, security) = policy_for(root);
    let mut spent = Vec::new();
    let walk = crate::tools::test_support::checks::walker::walk(
        "localSearch",
        serde_json::json!({"queries": [start]}),
        |call| {
            let input =
                crate::tools::test_support::opened_input(crate::id::ToolId::LocalSearch, call);
            let row = &input["queries"][0];
            let query: LocalSearchQuery =
                serde_json::from_value(row.clone()).expect("localSearch row");
            let started = std::time::Instant::now();
            let result = search_in(&query, &policy, &security, None)
                .unwrap_or_else(|error| panic!("{row}: {error:?}"));
            spent.push(started.elapsed());
            let body = result.to_row().expect("result");
            crate::tools::test_support::checks::Page {
                output: crate::tools::test_support::published(
                    crate::id::ToolId::LocalSearch,
                    &body,
                ),
                rows: row_ids(&body),
            }
        },
    );
    (spent[0], spent.iter().sum(), spent.len(), walk.rows.len())
}

/// A source of `lines` declarations, each with a quoted string and a call.
fn source(file: usize, lines: usize) -> String {
    (0..lines)
        .map(|line| {
            format!(
                "pub fn item_{file}_{line}(input: &str) -> String {{ let label = \"label of item {file} {line}\"; format!(\"{{label}}{{input}}\") }}\n    // let plain = {line};\n"
            )
        })
        .collect()
}

#[test]
#[ignore = "a timing printout, not an assertion: run with --ignored --nocapture"]
fn walk_timings() {
    // One 2.7 MB file, matchOnly spans over many pages.
    let one = tempfile::tempdir().expect("fixture");
    fs::write(one.path().join("types.rs"), source(0, 18_000)).expect("file");
    // 40 files of 40 KB, content rows over many pages.
    let many = tempfile::tempdir().expect("fixture");
    for file in 0..40 {
        fs::write(
            many.path().join(format!("f{file:03}.rs")),
            source(file, 270),
        )
        .expect("file");
    }
    settle();
    let cases = [
        (
            "matchOnly, one 2.7 MB file",
            one.path(),
            serde_json::json!({"path":one.path(),"matchString":["\"[^\"]{8,}\""],"regexEngine":"rust","view":"matchOnly","limit":2000}),
        ),
        (
            "content, 40 files",
            many.path(),
            serde_json::json!({"path":many.path(),"matchString":["fn "],"limit":2000}),
        ),
        (
            "context, 40 files",
            many.path(),
            serde_json::json!({"path":many.path(),"matchString":["let "],"limit":2000,"contextLines":{"before":1,"after":1}}),
        ),
    ];
    for (name, root, start) in cases {
        for run in ["cold", "warm"] {
            let (first, whole, pages, rows) = timed_walk(root, start.clone());
            eprintln!(
                "TIMING {name} [{run}]: first page {} ms, walk {} ms, {pages} pages, {rows} rows",
                first.as_millis(),
                whole.as_millis()
            );
        }
    }
}
