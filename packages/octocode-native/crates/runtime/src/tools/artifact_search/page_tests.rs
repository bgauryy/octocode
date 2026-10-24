//! Page validation against the paging range and the last reachable page.
use super::*;
use crate::providers::artifact::page_out_of_range;
use std::time::Duration;

/// An expired deadline separates page validation (runs first) from
/// provider execution: a page past the paging range fails as
/// `invalid_query` before any request budget is consulted, while an
/// in-range page reaches the provider and fails on the dead budget.
#[tokio::test]
async fn a_page_past_the_paging_range_is_rejected_before_provider_work() {
    let dead = Instant::now() - Duration::from_secs(1);
    let run = |page: u64| {
        let query = json!({"operation":"search",
            "ecosystem": "npm",
            "keywords": ["http"],
            "page": page});
        async move {
            execute(typed(&query), call(dead, false))
                .await
                .expect_err("dead budget or rejection")
        }
    };
    assert_eq!(run(5_000).await.code, ErrorCode::InvalidInput);
    assert_ne!(run(2).await.code, ErrorCode::InvalidInput);
}

/// A page past the paging range names the bound and repairs to the
/// last page its page size reaches, which the provider then serves:
/// npm search serves `from` up to 5,000 (a deeper one restarts at its
/// first rows), so 100-row pages reach page 51.
#[tokio::test]
async fn a_page_past_the_paging_range_repairs_to_the_last_reachable_page() {
    let dead = Instant::now() - Duration::from_secs(1);
    let query = json!({"operation":"search","ecosystem":"npm","keywords":["schema","validation"],
        "page":300});
    let error = execute(typed(&query), call(dead, false))
        .await
        .expect_err("past the paging range");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
    assert!(
        error.message.contains("page 300 starts past result 5000"),
        "{error:?}"
    );
    let next = error.calls.expect("a repair lead");
    assert_eq!(
        next.as_object().map(serde_json::Map::len),
        Some(1),
        "{next}"
    );
    let row = &next["repair"]["input"]["queries"][0];
    assert_eq!(
        row,
        &json!({"operation":"search","ecosystem":"npm","keywords":["schema","validation"],
            "page":251}),
        "{next}"
    );
    let mut last = typed(&query);
    artifact_search_query::set_page(&mut last, 251);
    assert!(page_out_of_range(&last).is_none());
    artifact_search_query::set_page(&mut last, 252);
    assert!(page_out_of_range(&last).is_some());
}

/// The deepest reachable page: the registry position's bound at large
/// page sizes, the contract's `page` maximum at small ones.
#[test]
fn the_last_reachable_page_follows_the_page_size() {
    for (ecosystem, last) in [("npm", 251), ("maven", 5), ("nuget", 200), ("crates", 50)] {
        let query = typed(&json!({"operation":"search","ecosystem":ecosystem,"keywords":["x"]}));
        assert_eq!(last_reachable_page(&query), last, "{ecosystem}");
    }
}
