//! A page minted by another build is refused, whatever else matches.
use super::*;
use octocode_runtime_base::test_support::{Fixture, simple_policy};
use serde_json::json;

/// `row` answered as the build `build` would.
fn answer(build: &str, row: &Value, root: &std::path::Path) -> Result<Value, ToolError> {
    let (paths, _) = simple_policy(root);
    let query: crate::ast_search::AstSearchQuery =
        serde_json::from_value(row.clone()).expect("canonical match row");
    let q = match &query {
        crate::ast_search::AstSearchQuery::Match0(query) => MatchQuery::Content(query),
        crate::ast_search::AstSearchQuery::MatchView(query) => MatchQuery::Summary(query),
        other => panic!("not a match row: {other:?}"),
    };
    run_match_under(
        build,
        q,
        &paths,
        &octocode_runtime_base::execution::cancel::NeverCancel,
        Some(1),
    )
    .map(octocode_runtime_base::response::result::finished)
}

/// The same query and scan under two builds digest differently, and a page-2
/// cursor of one build is `staleSnapshot` with an executable restart under the
/// other, while the build that minted it still serves it.
#[test]
fn a_page_cursor_of_another_build_is_stale_with_a_restart_lead() {
    let root = Fixture::new();
    for name in ["a.ts", "b.ts", "c.ts"] {
        std::fs::write(root.0.join(name), "console.log(1);\nconsole.log(2);\n").expect("file");
    }
    octocode_engine::stamp::settle_written_so_far();
    let first_row = json!({"operation":"match","path":root.0,"rule":{"pattern":"console.log($A)"},"parser":"typescript"});
    let first = answer("build-a", &first_row, &root.0).expect("page 1");
    let again = answer("build-b", &first_row, &root.0).expect("page 1 elsewhere");
    assert_ne!(
        octocode_runtime_base::test_support::pinned_snapshot(&first),
        octocode_runtime_base::test_support::pinned_snapshot(&again),
        "{first}"
    );
    // Production is the same function under this build's identity.
    let produced = crate::ast_search::test_rows::run_window(&root.0, first_row.clone(), 1)
        .expect("production page 1");
    let under = answer(
        octocode_runtime_base::test_support::BUILD_IDENTITY,
        &first_row,
        &root.0,
    )
    .expect("page 1");
    assert_eq!(
        octocode_runtime_base::test_support::pinned_snapshot(&produced),
        octocode_runtime_base::test_support::pinned_snapshot(&under),
        "{produced}"
    );
    let page_two = first["next"]["nextPage"]["input"]["queries"][0].clone();
    assert!(page_two.is_object(), "{first}");
    let served = answer("build-a", &page_two, &root.0).expect("the minting build serves it");
    assert_ne!(served["errorCode"], "staleSnapshot", "{served}");
    let refused = answer("build-b", &page_two, &root.0).expect("a refusal is an answer");
    assert_eq!(refused["errorCode"], "staleSnapshot", "{refused}");
    let restart =
        octocode_runtime_base::test_support::checks::related_call(&refused, "astSearch", |_| true);
    assert_eq!(restart["tool"], "astSearch", "{refused}");
    let restarted = answer("build-b", &restart["input"]["queries"][0], &root.0).expect("restart");
    assert!(restarted.get("errorCode").is_none(), "{restarted}");
}
