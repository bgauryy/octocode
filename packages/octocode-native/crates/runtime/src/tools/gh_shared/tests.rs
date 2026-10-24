//! Provider-backed tests of the GitHub search tools (ghSearchCode,
//! ghSearchRepo, ghStructure) against one mock server, plus the shared
//! paging helpers.
use super::*;
use crate::id::ToolId;
use octocode_contract_types::{GhSearchCodeQuery, GhSearchRepoQuery};
use serde_json::json;

#[test]
fn owner_listing_page_ceiling_keeps_rows_and_discloses_remaining_results() {
    let query: GhSearchRepoQuery =
        serde_json::from_value(json!({"operation":"list","owner":"o","page":1000})).expect("query");
    let mut page = json!({"repositories":[{"repo":"o/r"}],"pagination":{"hasMore":true}});
    add_next(&mut page, ToolId::GhSearchRepo, &query, 1000, true);
    assert_eq!(page["repositories"].as_array().expect("rows").len(), 1);
    assert!(page.get("calls").is_none(), "{page}");
    assert_eq!(page["terminalLimit"], true, "{page}");
    let mut earlier = json!({"repositories":[],"pagination":{"hasMore":true}});
    add_next(&mut earlier, ToolId::GhSearchRepo, &query, 999, true);
    assert_eq!(
        earlier["calls"]["nextPage"]["input"]["queries"][0]["page"],
        1000
    );
    assert!(earlier.get("terminalLimit").is_none());
}

#[test]
fn search_window_is_rejected_past_the_thousandth_result() {
    assert!(SearchWindow::PageStart.reject(11, 100).is_err());
    assert!(SearchWindow::PageStart.reject(10, 100).is_ok());
}

/// Repository, issue and commit search serve a page whose first result is
/// inside the cap (page 34 at 30 a page reaches result 1,020); code search
/// only a page whose last result is (page 333 at 3 a page, 999 results).
#[test]
fn each_search_window_reaches_its_own_last_page() {
    let start = SearchWindow::PageStart;
    assert_eq!(start.last_page(30), 34);
    assert!(start.reject(34, 30).is_ok());
    let past = start.reject(35, 30).expect_err("past the window");
    assert_eq!(
        past.message.as_ref(),
        "Page 35 is past GitHub's search window: at 30 per page, page 34 is the last, reaching the first 1020 results."
    );
    let reach = start.reach(12_599, 30);
    assert_eq!((reach.total, reach.pages, reach.capped), (1020, 34, true));
    let end = SearchWindow::PageEnd;
    assert_eq!(end.last_page(3), 333);
    assert!(end.reject(333, 3).is_ok());
    assert!(end.reject(334, 3).is_err());
    let reach = end.reach(1000, 3);
    assert_eq!((reach.total, reach.pages, reach.capped), (999, 333, true));
    let reach = end.reach(150, 20);
    assert_eq!((reach.total, reach.pages, reach.capped), (150, 8, false));
    assert_eq!(end.last_page(100), 10);
}

#[test]
fn incomplete_and_cap_are_losslessly_typed() {
    let query =
        serde_json::from_str::<GhSearchCodeQuery>(r#"{"owner":"o","keywords":["x"],"page":10}"#)
            .expect("GitHub search test data should be valid");
    let mut value = json!({"pagination":{"hasMore":false}});
    apply_partial(
        &mut value,
        &query,
        &SearchCompleteness {
            tool: ToolId::GhSearchCode,
            matched: 5_000,
            reachable: 1_000,
            incomplete: true,
            page: 10,
            has_more: false,
            subject: "code",
        },
    );
    assert_eq!(value["terminalLimit"], true);
    // `partialReasons` names the cap and `pagination.totalItems` its
    // size: no `providerLimit` restates them.
    assert!(value.get("providerLimit").is_none(), "{value}");
    assert_eq!(
        value["partialReasons"],
        json!(["providerResultCap", "providerIncompleteResults"])
    );
    assert_eq!(value["calls"]["retry"]["input"]["queries"][0]["page"], 10);
}

mod provider_backed;

/// Recovery leads never repeat a traversal the caller sent: `.` and `..`
/// resolve lexically and clamp at the repository root.
#[test]
fn recovery_paths_resolve_traversal_inside_the_repository() {
    assert_eq!(super::repo_path("../../../../user"), "user");
    assert_eq!(super::repo_path("src/./a/../b.rs"), "src/b.rs");
    assert_eq!(super::repo_path("../.."), ".");
    assert_eq!(super::repo_path("/docs/"), "docs");
    assert_eq!(super::parent_dir("../../../../user"), ".");
    assert_eq!(super::parent_dir("a/../b/c.md"), "b");
}

/// A listing path resolves `.`, `..` and stray slashes the way the
/// repository names it; one climbing above the root is invalid input.
#[test]
fn listing_paths_resolve_inside_the_repository_or_are_rejected() {
    for (path, listed) in [
        ("./packages/react-dom/npm", "packages/react-dom/npm"),
        ("packages//react-dom/", "packages/react-dom"),
        (
            "packages/react-dom/../react-dom/npm",
            "packages/react-dom/npm",
        ),
        ("a/../b/c", "b/c"),
        (".", ""),
        ("/", ""),
        ("a/..", ""),
    ] {
        assert_eq!(super::listing_path(path).expect(path), listed, "{path}");
    }
    for path in ["..", "../..", "../../etc", "a/../../b", "./.."] {
        let error = super::listing_path(path).expect_err(path);
        assert_eq!(
            error.kind,
            octocode_github::ProviderErrorKind::Validation,
            "{path}"
        );
    }
}
