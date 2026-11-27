//! A walk of `next.nextPage` across pages no cache keeps: the issued
//! continuation alone says which files the walk already listed.
use super::*;
use octocode_test_support::json_fixture;

/// The walk's page `page` (at its `limit` of 2) lists `names` (paths in
/// `a/b`), GitHub reporting `total` matches.
async fn mount_page(server: &MockServer, page: usize, total: usize, names: &[&str]) {
    mount_answer(server, page, 2, total, names).await;
}

/// Walk `pages` pages from page 1, each from the previous page's
/// `next.nextPage`, with no code-search cache: every earlier page has left it.
async fn walk(
    server: &MockServer,
    pages: usize,
) -> Vec<octocode_runtime_base::response::result::ToolData> {
    let provider = one_shot_provider(server);
    let mut query: GhSearchCodeQuery = serde_json::from_value(json_fixture(
        r#"{
            "owner": "a",
            "repo": "b",
            "keywords": ["needle"],
            "matchIn": ["path"],
            "limit": 2
        }"#,
    ))
    .expect("query");
    let mut outs = Vec::new();
    for _ in 0..pages {
        let out = execute(
            &provider,
            &query,
            &default_context(),
            &Passthrough,
            &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
        )
        .await
        .expect("search");
        if outs.len() + 1 < pages {
            query = next_page(&out);
        }
        outs.push(out);
    }
    outs
}

fn reasons(out: &octocode_runtime_base::response::result::ToolData) -> Vec<String> {
    out.data["partialReasons"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|reason| reason.as_str().map(str::to_owned))
        .collect()
}

/// GitHub sends `b.rs` again on page 3 after its page 1 left every cache:
/// the walk's continuation still knows it was listed, so page 3 drops it
/// and states the shift by its reason (a file may also have moved past the
/// walk unseen), with no warning restating it.
#[tokio::test]
async fn a_walk_drops_a_file_an_uncached_earlier_page_listed_and_discloses_the_shift() {
    let server = MockServer::builder().start().await;
    mount_page(&server, 1, 6, &["a.rs", "b.rs"]).await;
    mount_page(&server, 2, 6, &["c.rs", "d.rs"]).await;
    mount_page(&server, 3, 6, &["b.rs", "e.rs"]).await;
    let outs = walk(&server, 3).await;
    let listed = outs.iter().flat_map(paths_of).collect::<Vec<_>>();
    assert_eq!(listed, ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"]);
    let third = &outs[2].data;
    assert!(
        reasons(&outs[2]).contains(&"providerResultsShifted".to_owned()),
        "{third}"
    );
    assert_eq!(third["isPartial"], true, "{third}");
    assert!(third.get("warnings").is_none(), "{third}");
    for clean in &outs[..2] {
        assert!(clean.data.get("partialReasons").is_none(), "{}", clean.data);
    }
}

/// GitHub's match count changes mid-walk with no repeat: files may have
/// moved between pages, so the page that sees the change discloses it.
#[tokio::test]
async fn a_walk_discloses_a_changed_match_count_without_a_repeat() {
    let server = MockServer::builder().start().await;
    mount_page(&server, 1, 6, &["a.rs", "b.rs"]).await;
    mount_page(&server, 2, 5, &["d.rs", "e.rs"]).await;
    let outs = walk(&server, 2).await;
    let second = &outs[1].data;
    assert_eq!(paths_of(&outs[1]), ["d.rs", "e.rs"]);
    assert!(
        reasons(&outs[1]).contains(&"providerResultsShifted".to_owned()),
        "{second}"
    );
    assert_eq!(second["isPartial"], true, "{second}");
    let warnings = second["warnings"].to_string();
    assert!(warnings.contains("from 6 to 5"), "{second}");
    assert!(
        warnings.contains("cannot tell whether a file moved"),
        "{second}"
    );
    assert!(warnings.contains("re-walk"), "{second}");
}

/// Each extension's search holds `total` files of its own, `per` to each
/// of its pages, answered at any `page`/`per_page` as GitHub does.
async fn mount_extension_pages(server: &MockServer, extension: &str, total: usize, per: usize) {
    let items = (0..total)
        .map(|index| {
            let (page, file) = (index / per + 1, index % per);
            let name = format!("{extension}_p{page}_{file}.{extension}");
            json!({"name":name,"path":name,"sha":"1","html_url":"https://x",
             "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(wiremock::matchers::query_param_contains(
            "q",
            format!("extension:{extension}"),
        ))
        .respond_with(octocode_test_support::mock_http::SearchIndex(items))
        .mount(server)
        .await;
}

/// A `nextPage` walk over k extensions carries each search's match count in
/// its continuation: every page costs k code searches (never k more to
/// re-read page 1 of each), and the walk reaches every hit exactly once.
#[tokio::test]
async fn a_multi_extension_walk_costs_one_search_per_extension_each_page() {
    let server = MockServer::builder().start().await;
    let extensions = ["ts", "js", "py"];
    for extension in extensions {
        mount_extension_pages(&server, extension, 6, 2).await;
    }
    let provider = one_shot_provider(&server);
    let mut query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"a","repo":"b","keywords":["needle"],"matchIn":["path"],
        "extensions":extensions,"limit":6
    }))
    .expect("query");
    let mut listed = Vec::new();
    let mut counted = 0;
    let mut pages = 0;
    loop {
        let out = execute(
            &provider,
            &query,
            &default_context(),
            &Passthrough,
            &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
        )
        .await
        .expect("page");
        pages += 1;
        let requests = server.received_requests().await.unwrap_or_default().len();
        assert_eq!(
            requests - counted,
            extensions.len(),
            "page {pages} reads each extension's search once"
        );
        counted = requests;
        listed.extend(paths_of(&out));
        if out.data["pagination"]["hasMore"] != true {
            break;
        }
        query = next_page(&out);
    }
    assert_eq!(pages, 3, "{listed:?}");
    let mut unique = listed.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 18, "{listed:?}");
    assert_eq!(listed.len(), 18, "every hit once: {listed:?}");
}

/// Every response of a multi-`extensions` walk holds at most `limit` files,
/// whatever `limit` and the number of extensions are, and the walk's
/// continuations reach every hit exactly once: a page whose merged searches
/// list more than `limit` files shows the rest as the next part of the page.
#[tokio::test]
async fn a_multi_extension_response_holds_at_most_limit_files() {
    let extensions = ["ts", "js", "py"];
    let each = 6;
    for limit in 1..=extensions.len() * 2 + 1 {
        let server = MockServer::builder().start().await;
        for extension in extensions {
            mount_extension_pages(&server, extension, each, 1).await;
        }
        let provider = one_shot_provider(&server);
        let mut query: GhSearchCodeQuery = serde_json::from_value(json!({
            "owner":"a","repo":"b","keywords":["needle"],"matchIn":["path"],
            "extensions":extensions,"limit":limit
        }))
        .expect("query");
        let mut listed = Vec::new();
        loop {
            let out = execute(
                &provider,
                &query,
                &default_context(),
                &Passthrough,
                &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
            )
            .await
            .expect("page");
            let paths = paths_of(&out);
            assert!(
                !paths.is_empty() && paths.len() <= limit,
                "limit {limit}: {}",
                out.data
            );
            // Every response states where it stands in the walk: it starts
            // after the files the earlier responses showed, and `hasMore`
            // holds exactly while a file is left, on another search page or
            // in the rest of this one.
            let continues = out.data["next"].get("nextPage").is_some();
            let pagination = &out.data["pagination"];
            assert_eq!(
                pagination["offset"],
                listed.len(),
                "limit {limit}: {pagination}"
            );
            assert_eq!(pagination["length"], paths.len(), "limit {limit}");
            listed.extend(paths);
            assert!(listed.len() <= extensions.len() * each, "{listed:?}");
            assert_eq!(
                pagination["hasMore"], continues,
                "limit {limit}: {pagination}"
            );
            assert_eq!(
                pagination["hasMore"],
                listed.len() < extensions.len() * each,
                "limit {limit} after {} files: {pagination}",
                listed.len()
            );
            // Several provider searches: no total is stated on any page.
            assert!(pagination["total"].is_null(), "{pagination}");
            if !continues {
                break;
            }
            query = next_page(&out);
        }
        let mut unique = listed.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), extensions.len() * each, "limit {limit}");
        assert_eq!(listed.len(), unique.len(), "limit {limit}: every hit once");
    }
}

/// The same bound on a page that reads its hit lines: more extensions than
/// `limit` list one file each, the response shows `limit` of them with
/// their lines, and the page's continuation shows the rest.
#[tokio::test]
async fn a_multi_extension_line_page_holds_at_most_limit_files() {
    let server = MockServer::builder().start().await;
    let extensions = ["ts", "js", "py"];
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/commits/HEAD"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
        .mount(&server)
        .await;
    for extension in extensions {
        let name = format!("only.{extension}");
        Mock::given(method("GET"))
            .and(path("/api/v3/search/code"))
            .and(wiremock::matchers::query_param_contains(
                "q",
                format!("extension:{extension}"),
            ))
            .respond_with(octocode_test_support::mock_http::SearchIndex(vec![json!({
                "name":name,"path":name,"sha":"1","html_url":"https://x",
                "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}
            })]))
            .mount(&server)
            .await;
        octocode_test_support::mock_http::list_in_trees(&server, "a", "b", &name).await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode(format!("let {extension} = needle();\n"))
            })))
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let limit = extensions.len() - 1;
    let mut query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"a","repo":"b","keywords":["needle"],"extensions":extensions,"limit":limit
    }))
    .expect("query");
    let mut listed = Vec::new();
    loop {
        let out = execute(
            &provider,
            &query,
            &default_context(),
            &Passthrough,
            &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
        )
        .await
        .expect("page");
        let rows = out.data["files"].as_array().expect("files");
        assert!(!rows.is_empty() && rows.len() <= limit, "{}", out.data);
        for row in rows {
            assert!(
                row["matches"].is_array(),
                "a shown row lists its lines: {row}"
            );
        }
        listed.extend(paths_of(&out));
        assert!(listed.len() <= extensions.len(), "{listed:?}");
        if out.data["next"].get("nextPage").is_none() {
            break;
        }
        query = next_page(&out);
    }
    listed.sort_unstable();
    assert_eq!(listed, ["only.js", "only.py", "only.ts"]);
}
