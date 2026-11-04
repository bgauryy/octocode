//! A listing costs the listing: no request names a date, so none is asked of
//! GitHub, and a page holds the query's whole `limit`.
use super::*;
use crate::contracts::query_limits::gh_structure as limits;

/// Each `files` entry of one page, in order.
fn entries(data: &Value) -> Vec<String> {
    data["files"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|directory| directory["files"].as_array().into_iter().flatten())
        .map(|entry| entry.as_str().expect("entry").to_owned())
        .collect()
}

/// Requests the mock server saw to `route`.
async fn requests_to(server: &MockServer, route: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| request.url.path() == route)
        .count()
}

fn root_of(count: usize) -> Vec<Value> {
    (0..count)
        .map(|index| {
            let name = format!("f{index:04}.rs");
            json!({"name":name,"path":name,"type":"file","size":1})
        })
        .collect()
}

/// A listing inside the default `limit` is one page: every entry, its size
/// and nothing a date lookup would have added, with no GraphQL request.
#[tokio::test]
async fn a_listing_makes_no_date_lookup_and_honors_its_default_limit() {
    let server = server().await;
    let total = limits::tree::LIMIT_DEFAULT - 1;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!(root_of(total)),
    )
    .await;
    let row = json!({"operation":"tree","owner":"a","repo":"b","ref":"main","maxDepth":1});
    let out = list(&server, row).await.expect("tree page");
    let data = &out.data;
    assert_eq!(requests_to(&server, "/api/graphql").await, 0, "{data}");
    let expected = (0..total)
        .map(|index| format!("f{index:04}.rs (1)"))
        .collect::<Vec<_>>();
    assert_eq!(entries(data), expected);
    assert!(data.get("pagination").is_none(), "{data}");
    assert!(data.get("warnings").is_none(), "{data}");
    assert!(data.get("commitDate").is_none(), "{data}");
    crate::tools::test_support::assert_row_contract(ToolId::GhStructure, data);
}

/// A walk past one page lists every entry exactly once at the requested
/// `limit`, pinned to the listed commit, with `page` and `totalPages` counting
/// the calls it takes and no GraphQL request on any page.
#[tokio::test]
async fn a_walk_lists_every_entry_once_at_its_limit() {
    let server = server().await;
    let limit = limits::tree::LIMIT_DEFAULT;
    let total = limit * 2 + 7;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!(root_of(total)),
    )
    .await;
    let mut row = json!({"operation":"tree","owner":"a","repo":"b","ref":"main","maxDepth":1});
    let mut listed = Vec::new();
    let mut sizes = Vec::new();
    loop {
        let out = list(&server, row).await.expect("tree page");
        assert!(out.data.get("warnings").is_none(), "{}", out.data);
        let page = entries(&out.data);
        sizes.push(page.len());
        listed.extend(page);
        let Some(next) = out.data.pointer("/next/nextPage/input/queries/0") else {
            break;
        };
        assert_eq!(out.data["pagination"]["totalItems"], total, "{}", out.data);
        assert_eq!(out.data["pagination"]["totalPages"], 3, "{}", out.data);
        assert_eq!(next["ref"], SHA);
        assert_eq!(next["maxDepth"], 1);
        assert!(sizes.len() < 4, "nonterminating tree: {}", out.data);
        row = next.clone();
    }
    assert_eq!(sizes, [limit, limit, 7]);
    let expected = (0..total)
        .map(|index| format!("f{index:04}.rs (1)"))
        .collect::<Vec<_>>();
    assert_eq!(listed, expected);
    assert_eq!(requests_to(&server, "/api/graphql").await, 0);
}
