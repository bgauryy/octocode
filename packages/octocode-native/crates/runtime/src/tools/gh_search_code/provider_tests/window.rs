//! The search window: its last whole page, per-extension windows and the merged multi-extension walk.
use super::*;

/// A search of `pages` total results at 20 a page: GitHub answers every
/// page with the same `total_count`.
async fn mount_counted_search(server: &MockServer, total: usize) {
    let item = |name: &str| {
        json!({"name":name,"path":format!("src/{name}"),"sha":"1","html_url":"https://x",
            "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":total,"incomplete_results":false,"items":[item("busy.rs"), item("calm.rs")]
        })))
        .mount(server)
        .await;
}

/// A page past GitHub's 1,000-result window repairs to the search's last
/// real page, not to the window's last page (an empty page past the end).
#[tokio::test]
async fn a_page_past_the_window_repairs_to_the_last_real_page() {
    let server = MockServer::builder().start().await;
    mount_counted_search(&server, 150).await;
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(
        json!({"owner":"a","repo":"b","keywords":["needle"],"page":60,"fields":["path"]}),
    )
    .expect("query");
    let context = default_context();
    let Err(failure) = super::super::run(&provider, &query, Ok(&context), &Passthrough).await
    else {
        panic!("a page past the window is a failure");
    };
    let next = failure.calls.expect("repair");
    assert_eq!(next["repair"]["input"]["queries"][0]["page"], 3, "{next}");
}

/// Code search serves a page only while its last result lies inside the
/// 1,000-result window: at 3 a page the window ends at page 333 (999
/// results), which is the terminal page, and page 334 repairs to it.
#[tokio::test]
async fn the_code_search_window_ends_at_its_last_whole_page() {
    let server = MockServer::builder().start().await;
    mount_counted_search(&server, 2236).await;
    let provider = one_shot_provider(&server);
    let at = |page: usize| -> GhSearchCodeQuery {
        serde_json::from_value(json!({
            "owner":"a","repo":"b","keywords":["needle"],"page":page,"fields":["path"]
        }))
        .expect("query")
    };
    let context = default_context();
    let first = execute(&provider, &at(1), &context, &Passthrough)
        .await
        .expect("first page");
    assert_eq!(
        first.data["pagination"]["totalItems"], 1000,
        "{}",
        first.data
    );
    assert_eq!(first.data["pagination"]["totalPages"], 20, "{}", first.data);
    let last = execute(&provider, &at(20), &context, &Passthrough)
        .await
        .expect("last page");
    assert!(last.data["next"]["nextPage"].is_null(), "{}", last.data);
    assert_eq!(last.data["terminalLimit"], true, "{}", last.data);
    let Err(failure) = super::super::run(&provider, &at(21), Ok(&context), &Passthrough).await
    else {
        panic!("a page past the window is a failure");
    };
    let next = failure.calls.expect("repair");
    assert_eq!(next["repair"]["input"]["queries"][0]["page"], 20, "{next}");
}

/// A merged multi-`extensions` page is page N of each extension's own
/// search of `ceil(pageSize / k)` files, so the window is that search's: at
/// 50 files over 3 extensions (17 each) page 58 is the last, and page 59
/// repairs to it. The error states the caller's page size and the merged
/// reachable total (3 × 986), as page 1 does, never the per-search size.
#[tokio::test]
async fn a_multi_extension_window_is_each_searchs_own() {
    let server = MockServer::builder().start().await;
    mount_counted_search(&server, 5000).await;
    let provider = one_shot_provider(&server);
    let at = |page: usize| -> GhSearchCodeQuery {
        serde_json::from_value(json!({
            "owner":"a","repo":"b","keywords":["needle"],"extensions":["rs","ts","go"],
            "page":page,"fields":["path"]
        }))
        .expect("query")
    };
    let context = default_context();
    execute(&provider, &at(58), &context, &Passthrough)
        .await
        .expect("the last page");
    let Err(failure) = super::super::run(&provider, &at(59), Ok(&context), &Passthrough).await
    else {
        panic!("a page past the window is a failure");
    };
    assert_eq!(
        failure.message,
        "Page 59 is past GitHub's search window: at pageSize 50, page 58 is the last, reaching the first 2958 results."
    );
    let next = failure.calls.expect("repair");
    assert_eq!(next["repair"]["input"]["queries"][0]["page"], 58, "{next}");
}

/// `fields:["path","matchCount"]` states each file's hit count without
/// listing its lines, and no warning says the count is missing.
#[tokio::test]
async fn a_match_count_pick_counts_every_file_without_lines() {
    let server = MockServer::builder().start().await;
    mount_counted_search(&server, 2).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    for (name, body) in [
        ("busy.rs", "wrap_app();\n".repeat(8)),
        ("calm.rs", "fn wrap_app() {}\n".to_owned()),
    ] {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/src%2F{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode(body)
            })))
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"a","repo":"b","keywords":["wrap_app"],"fields":["path","matchCount"]
    }))
    .expect("query");
    let context = default_context();
    let out = execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("search");
    let files = &out.data["files"];
    assert_eq!(files[0]["path"], "src/busy.rs", "{}", out.data);
    assert_eq!(files[0]["matchCount"], 8, "{}", out.data);
    assert_eq!(files[1]["matchCount"], 1, "{}", out.data);
    assert!(files[0].get("lines").is_none(), "{}", out.data);
    assert!(out.data.get("warnings").is_none(), "{}", out.data);
}

/// Every layout of merged searches lists each GitHub page once: page 1 of
/// each search, then the later pages `k` to a merged page, every merged
/// page full but the last.
#[test]
fn merged_pages_list_every_github_page_once_and_fill_each_page() {
    for pages in [
        vec![4, 0],
        vec![0, 4],
        vec![1, 5, 2],
        vec![3, 3, 3],
        vec![58, 58, 58],
        vec![0, 0],
        vec![7],
        vec![2, 0, 0, 9, 1],
    ] {
        let k = pages.len();
        let total = super::super::search_page::merged_pages(&pages);
        let mut listed: Vec<(usize, usize)> = (0..k).map(|search| (search, 1)).collect();
        listed.retain(|&(search, _)| pages[search] > 0);
        for current in 2..=total {
            let units = super::super::search_page::later_pages(&pages, current);
            assert!(!units.is_empty(), "{pages:?} page {current}");
            if current < total {
                assert_eq!(units.len(), k, "{pages:?} page {current} is full");
            }
            listed.extend(units);
        }
        assert!(
            super::super::search_page::later_pages(&pages, total.max(1) + 1).is_empty(),
            "{pages:?}"
        );
        let mut expected: Vec<(usize, usize)> = pages
            .iter()
            .enumerate()
            .flat_map(|(search, &count)| (1..=count).map(move |page| (search, page)))
            .collect();
        let mut sorted = listed.clone();
        sorted.sort_unstable();
        expected.sort_unstable();
        assert_eq!(sorted, expected, "{pages:?}: every page once");
    }
}

/// A multi-extension walk never asks an exhausted extension again and
/// fills its pages from the live one: 12 `.ts` files and no `.rs` at
/// pageSize 6 list 3 + 6 + 3 files over 3 pages, every file once, in 5
/// searches (4 for `.ts`, 1 for `.rs`) instead of 8.
#[tokio::test]
async fn a_multi_extension_walk_skips_exhausted_searches_and_fills_its_pages() {
    let server = MockServer::builder().start().await;
    for page in 1..=4 {
        let items = (0..25)
            .map(|file| {
                let name = format!("p{page}_{file}.ts");
                json!({"name":name,"path":name,"sha":"1","html_url":"https://x",
                 "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
            })
            .collect::<Vec<_>>();
        Mock::given(method("GET"))
            .and(path("/api/v3/search/code"))
            .and(wiremock::matchers::query_param_contains(
                "q",
                "extension:ts",
            ))
            .and(wiremock::matchers::query_param("page", page.to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_count":100,"incomplete_results":false,"items":items
            })))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(wiremock::matchers::query_param_contains(
            "q",
            "extension:rs",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"total_count":0,"incomplete_results":false,"items":[]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let mut provider = one_shot_provider(&server);
    provider.transport.cache = std::sync::Arc::new(MemoryCache::default());
    let mut sizes = Vec::new();
    let mut listed = Vec::new();
    for page in 1.. {
        let query: GhSearchCodeQuery = serde_json::from_value(json!({
            "owner":"a","repo":"b","keywords":["needle"],"matchIn":["path"],
            "extensions":["ts","rs"],"page":page
        }))
        .expect("query");
        let out = execute(&provider, &query, &default_context(), &Passthrough)
            .await
            .expect("page");
        assert_eq!(out.data["pagination"]["totalPages"], 3, "{}", out.data);
        let paths = paths_of(&out);
        sizes.push(paths.len());
        listed.extend(paths);
        if out.data["pagination"]["hasMore"] != true {
            break;
        }
    }
    assert_eq!(sizes, [25, 50, 25]);
    let mut unique = listed.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 100, "{listed:?}");
    assert_eq!(listed.len(), 100, "{listed:?}");
    let searches = server.received_requests().await.unwrap_or_default().len();
    assert_eq!(searches, 5, "4 .ts pages and one .rs page");
}
