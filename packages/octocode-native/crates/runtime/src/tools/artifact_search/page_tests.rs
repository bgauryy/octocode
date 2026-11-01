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
/// first rows), so 20-row pages reach page 251. `page` is private walk
/// state, so the repair is continued work: the published error row
/// carries it as `next.repair`, an opaque cursor whose saved row resumes
/// at page 251 and passes page validation into provider work.
#[tokio::test]
async fn a_page_past_the_paging_range_repairs_to_the_last_reachable_page() {
    use crate::id::ToolId;
    use crate::tools::test_support::{assert_row_contract, opened_input, published};
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
    // The error row as the runtime renders it, published as every surface
    // publishes it: continued work is sealed into a cursor.
    let calls = error.calls.map(|calls| *calls);
    let mut row = crate::response::outcome::DomainResult::failure(
        error.code,
        error.message,
        error.hints,
        calls,
    )
    .data;
    row["status"] = json!("error");
    assert_row_contract(ToolId::ArtifactSearch, &row);
    let public = published(ToolId::ArtifactSearch, &row);
    let data = &public["results"][0]["data"];
    assert_eq!(
        data["next"].as_object().map(serde_json::Map::len),
        Some(1),
        "{data}"
    );
    assert!(data.get("related").is_none(), "{data}");
    let repair = &data["next"]["repair"];
    assert_eq!(repair["kind"], "continue", "{repair}");
    assert!(repair["input"]["cursor"].is_string(), "{repair}");
    let resumed = opened_input(ToolId::ArtifactSearch, &repair["input"])["queries"][0].clone();
    assert_eq!(
        resumed["page"], 251,
        "the repair resumes at the last reachable page: {resumed}"
    );
    for field in ["operation", "ecosystem", "keywords"] {
        assert_eq!(resumed[field], query[field], "{resumed}");
    }
    // The resumed page passes page validation and reaches provider work,
    // which then fails on the dead budget.
    let resumed = execute(typed(&resumed), call(dead, false))
        .await
        .expect_err("dead budget");
    assert_ne!(resumed.code, ErrorCode::InvalidInput, "{resumed:?}");
    let mut last = typed(&query);
    artifact_search_query::set_page(&mut last, 251);
    assert!(page_out_of_range(&last).is_none());
    artifact_search_query::set_page(&mut last, 252);
    assert!(page_out_of_range(&last).is_some());
}

/// The deepest reachable page: the registry position's bound at the
/// query's page size.
#[test]
fn the_last_reachable_page_follows_the_page_size() {
    for (ecosystem, last) in [("npm", 251), ("maven", 5), ("nuget", 200), ("crates", 50)] {
        let query = typed(&json!({"operation":"search","ecosystem":ecosystem,"keywords":["x"]}));
        assert_eq!(last_reachable_page(&query), last, "{ecosystem}");
    }
    // The page size is the query's `limit`: 100-row npm pages reach page 51.
    let query =
        typed(&json!({"operation":"search","ecosystem":"npm","keywords":["x"],"limit":100}));
    assert_eq!(last_reachable_page(&query), 51);
}
