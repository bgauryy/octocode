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
        let query = json!({
            "ecosystem": "npm",
            "keywords": ["http"],
            "page": page,
            "mainGoal": "test", "reasoning": "test",
        });
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
    let query = json!({"ecosystem":"npm","keywords":["schema","validation"],
        "page":200,"pageSize":100});
    let error = execute(typed(&query), call(dead, false))
        .await
        .expect_err("past the paging range");
    assert_eq!(error.code, ErrorCode::InvalidInput, "{error:?}");
    assert!(
        error.message.contains("page 200 starts past result 5000"),
        "{error:?}"
    );
    let next = error.next.expect("a repair lead");
    assert_eq!(next.as_object().map(|next| next.len()), Some(1), "{next}");
    let row = &next["repair"]["query"]["queries"][0];
    assert_eq!(
        row,
        &json!({"ecosystem":"npm","keywords":["schema","validation"],
            "page":51,"pageSize":100}),
        "{next}"
    );
    let mut last = typed(&query);
    artifact_search_query::set_page(&mut last, 51);
    assert!(page_out_of_range(&last).is_none());
    artifact_search_query::set_page(&mut last, 52);
    assert!(page_out_of_range(&last).is_some());
}

/// The deepest reachable page: the registry position's bound at large
/// page sizes, the contract's `page` maximum at small ones.
#[test]
fn the_last_reachable_page_follows_the_page_size() {
    for (ecosystem, page_size, last) in [
        ("npm", 100, 51),
        ("npm", 10, 501),
        ("npm", 1, 1000),
        // Central serves the rows of one start-0 answer only.
        ("maven", 100, 1),
        ("maven", 20, 5),
        ("maven", 30, 4),
        ("maven", 1, 100),
        // NuGet serves `skip <= 3000` with `take <= 1000`: rows end at 4,000.
        ("nuget", 100, 40),
        ("nuget", 30, 134),
        ("nuget", 7, 572),
        ("nuget", 1, 1000),
        // crates.io serves relevance results below 1,000.
        ("crates", 100, 10),
        ("crates", 30, 34),
        ("crates", 1, 1000),
    ] {
        let query = typed(&json!({"ecosystem":ecosystem,"keywords":["x"],
            "pageSize":page_size}));
        assert_eq!(last_reachable_page(&query), last, "{ecosystem} {page_size}");
    }
}
