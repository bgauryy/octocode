//! Provider-backed tests of the GitHub search tools (ghSearchCode,
//! ghSearchRepo, ghStructure) against one mock server, plus the shared
//! paging helpers.
use super::*;
use crate::id::ToolId;
use octocode_contract_types::{GhSearchCodeQuery, GhSearchRepoQuery};
use serde_json::json;

/// An owner listing pages GitHub's REST list, which has no search window:
/// page 1,000 (the former schema ceiling) continues to page 1,001.
#[test]
fn owner_listing_continues_past_the_former_page_ceiling() {
    let query: GhSearchRepoQuery =
        serde_json::from_value(json!({"operation":"list","owner":"o","page":1000})).expect("query");
    let mut page = json!({"repositories":[{"repo":"o/r"}],"pagination":{"hasMore":true}});
    add_next(&mut page, ToolId::GhSearchRepo, &query, 1000, true);
    assert_eq!(page["repositories"].as_array().expect("rows").len(), 1);
    assert_eq!(
        page["calls"]["nextPage"]["input"]["queries"][0]["page"],
        1001
    );
    assert!(page.get("terminalLimit").is_none(), "{page}");
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
    // The count the page states stops at the cap, and says so.
    assert_eq!(value["pagination"]["totalItemsCapped"], true, "{value}");
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

/// A repository that did not resolve: every search tool's repair is the
/// shared name search, the owner's listing when the owner is known, and the
/// hint that labels the name search's rows as candidates under any owner.
#[test]
fn a_missing_repository_repair_labels_its_name_search() {
    let missing = || {
        GhFailure::new(
            octocode_github::ProviderError::new(
                octocode_github::ProviderErrorKind::NotFound,
                "Not Found",
            )
            .with_reason(octocode_github::ProviderErrorReason::RepositoryNotFound),
            "Not Found".to_owned(),
        )
        .hint("replaced")
    };
    let query = json!({"owner":"o","repo":"gone"});
    let owned = with_search_repair(
        missing(),
        ToolId::GhStructure,
        &query,
        1,
        Some("o"),
        Some("gone"),
    );
    let calls = owned.calls.as_deref().expect("leads");
    assert_eq!(calls, &repository_recovery(Some("o"), "gone"));
    assert_eq!(
        calls["findRepository2"]["input"]["queries"][0],
        json!({"operation":"list","owner":"o"}),
        "{calls}"
    );
    let [hint] = owned.hints.as_slice() else {
        panic!("one hint: {:?}", owned.hints);
    };
    assert!(
        hint.contains("named like `gone` under any owner")
            && hint.contains("confirm the owner")
            && hint.contains("`o` holds"),
        "{hint}"
    );
    // Without an owner there is no listing to offer, and the hint does not
    // promise one.
    let ownerless = with_search_repair(
        missing(),
        ToolId::GhSearchRepo,
        &query,
        1,
        None,
        Some("gone"),
    );
    let calls = ownerless.calls.as_deref().expect("leads");
    assert!(calls.get("findRepository2").is_none(), "{calls}");
    assert_eq!(ownerless.hints, [repository_search_hint(None, "gone")]);
    assert!(
        !ownerless.hints[0].contains("holds"),
        "{:?}",
        ownerless.hints
    );
}

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
