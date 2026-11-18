//! A `regex:"pcre2"` search ranks its whole scope whatever its time budget
//! does: a window the budget pauses lists no file and resumes where it
//! paused, so the pages a paused search lists are the pages an unpaused one
//! lists, in its order.
use super::*;
use crate::local_search::window::{forget_kept, test_budget};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

/// Fifteen files over nested directories, with hit counts that rank in
/// another order than their paths, three without a hit.
fn spread() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("fixture directory");
    for index in 0..15_usize {
        let dir = root.path().join(["a", "b", "c/d", "c"][index % 4]);
        fs::create_dir_all(&dir).expect("fixture dir");
        let hits = (index * 7) % 5;
        let body = format!("{}plain line\n", "foo bar\n".repeat(hits));
        fs::write(dir.join(format!("f{index:02}.txt")), body).expect("fixture");
    }
    root
}

/// Every row a search lists, window after window and page after page, in
/// walk order, each call under the next window of `budgets` (files per
/// window; `None` runs whole; the last repeats). `forget` drops what paused
/// windows kept before each call, as eviction or another process would.
/// Returns the rows and the windows that paused.
fn chain(
    root: &std::path::Path,
    start: &Value,
    budgets: &[Option<u32>],
    forget: bool,
) -> (Vec<String>, usize) {
    let (policy, security) = policy_for(root);
    let (mut calls, mut paused) = (0, 0);
    let mut start = start.clone();
    start["path"] = json!(root.to_string_lossy());
    let walk = octocode_runtime_base::test_support::checks::walker::walk(
        "localSearch",
        json!({"queries": [start]}),
        |call| {
            let budget = budgets[calls.min(budgets.len() - 1)];
            calls += 1;
            if forget {
                forget_kept(root);
            }
            let input = octocode_runtime_base::test_support::opened_input(
                octocode_runtime_base::id::ToolId::LocalSearch,
                call,
            );
            let row = &input["queries"][0];
            let query: LocalSearchQuery =
                serde_json::from_value(row.clone()).expect("localSearch row");
            let result = {
                let _window = test_budget(budget);
                search_in(&query, &policy, &security, None)
                    .unwrap_or_else(|error| panic!("{row}: {error:?}"))
            };
            let body = result.to_row().expect("result");
            if body["partialReasons"] == json_fixture(r#"["scanBudget"]"#) {
                paused += 1;
                assert!(
                    body.get("files").is_none(),
                    "a paused window lists no file: {body}"
                );
                let resume = &body["next"]["nextPage"]["input"]["queries"][0];
                assert!(resume["scanOffset"].as_u64().is_some(), "{body}");
                assert!(resume["snapshot"].is_string(), "{body}");
            } else {
                assert!(body.get("partialReasons").is_none(), "{body}");
            }
            octocode_runtime_base::test_support::checks::Page {
                output: octocode_runtime_base::test_support::published(
                    octocode_runtime_base::id::ToolId::LocalSearch,
                    &body,
                ),
                rows: row_ids(&body),
            }
        },
    );
    (walk.rows, paused)
}

#[test]
fn a_paused_pcre2_search_lists_the_same_pages_in_the_same_order_whatever_the_clock_does() {
    let root = spread();
    let schedules: [&[Option<u32>]; 5] = [
        &[Some(1)],
        &[Some(2)],
        &[Some(5)],
        &[Some(3), None],
        &[None, Some(2)],
    ];
    for start in [
        json_fixture(
            r#"{
                "matchString": ["fo(?=o)"],
                "regexEngine": "pcre2",
                "limit": 4,
                "sort": {"by": "matchCount", "order": "desc"}
            }"#,
        ),
        json_fixture(
            r#"{
                "matchString": ["fo(?=o)"],
                "regexEngine": "pcre2",
                "limit": 4
            }"#,
        ),
        json_fixture(
            r#"{
                "matchString": ["fo(?=o)"],
                "regexEngine": "pcre2",
                "view": "countMatches",
                "limit": 3,
                "sort": {"by": "path", "order": "desc"}
            }"#,
        ),
        json_fixture(
            r#"{
                "matchString": ["fo(?=o)"],
                "regexEngine": "pcre2",
                "view": "files",
                "sort": {"by": "traversal", "order": "asc"}
            }"#,
        ),
    ] {
        let (whole, unpaused) = chain(root.path(), &start, &[None], false);
        assert_eq!(unpaused, 0);
        assert!(whole.len() >= 12, "{start}: {whole:?}");
        for schedule in schedules {
            for forget in [false, true] {
                let (rows, paused) = chain(root.path(), &start, schedule, forget);
                assert_eq!(rows, whole, "{start} {schedule:?} forget={forget}");
                assert!(
                    paused > 0 || schedule[0].is_none(),
                    "{start} {schedule:?}: no window paused"
                );
            }
        }
    }
}

/// The window that ends a paused search answers as the query without its
/// resume does: the same snapshot, so its later pages are that query's.
#[test]
fn the_window_that_ends_a_paused_search_has_the_unpaused_snapshot() {
    let root = spread();
    let (policy, security) = policy_for(root.path());
    let start = ls_query(
        json!({"path": root.path().to_string_lossy(), "matchString": ["fo(?=o)"],
               "regexEngine": "pcre2", "limit": 4}),
        None,
    );
    let whole = search_in(&start, &policy, &security, None).expect("unpaused");
    let mut query = start;
    let mut windows = 0;
    let last = loop {
        windows += 1;
        assert!(windows < 50, "the search never ends");
        let result = {
            let _window = test_budget(Some(4));
            search_in(&query, &policy, &security, None).expect("window")
        };
        let body = result.to_row().expect("row");
        let Some(next) = body["next"]["nextPage"]["input"]["queries"][0]
            .as_object()
            .filter(|_| body["partialReasons"] == json_fixture(r#"["scanBudget"]"#))
        else {
            break result;
        };
        query = serde_json::from_value(Value::Object(next.clone())).expect("resume");
    };
    assert!(windows > 1, "the budget never paused the search");
    assert_eq!(last.source_snapshot, whole.source_snapshot);
    assert_eq!(values(&last), values(&whole));
    let next = last.to_row().expect("row")["next"]["nextPage"]["input"]["queries"][0].clone();
    assert!(next.get("scanOffset").is_none(), "{next}");
}
