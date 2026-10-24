//! Page facts: partial pages, hit counts, estimates and the repeats a later page drops.
use super::*;

/// A single complete page whose file lists fewer hit lines than it has
/// is partial because of those unlisted lines, and the row says so; its
/// `next.readHits` reaches them. A page with another page behind it states
/// its remainder through `pagination` instead.
#[tokio::test]
async fn a_complete_page_with_unlisted_hit_lines_names_why_it_is_partial() {
    let server = MockServer::builder().start().await;
    let item = |name: &str| {
        json!({"name":name,"path":format!("src/{name}"),"sha":"1","html_url":"https://x",
            "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":2,"incomplete_results":false,"items":[item("busy.rs"), item("calm.rs")]
        })))
        .mount(&server)
        .await;
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
    let search = |keywords: serde_json::Value| {
        serde_json::from_value::<GhSearchCodeQuery>(
            json!({"owner":"a","repo":"b","keywords":keywords}),
        )
        .expect("query")
    };
    let context = default_context();
    let out = execute(
        &provider,
        &search(json!(["wrap_app"])),
        &context,
        &Passthrough,
    )
    .await
    .expect("search");
    assert_eq!(out.data["isPartial"], true, "{}", out.data);
    assert_eq!(
        out.data["partialReasons"],
        json!(["hitLinesNotListed"]),
        "{}",
        out.data
    );
    assert!(out.data["next"]["readHits"].is_object(), "{}", out.data);
}

/// `pageSize` counts files: a page of two files states the search's file
/// total in its pagination, a file with more keyword lines than the five it
/// lists states its `matchCount`, and `next.readHits` reads the rest of
/// exactly that file; a file whose hits all show states no count.
#[tokio::test]
async fn a_page_counts_files_and_a_file_with_more_hits_says_how_many_and_leads_to_them() {
    let server = MockServer::builder().start().await;
    let item = |name: &str| {
        json!({"name":name,"path":format!("src/{name}"),"sha":"1","html_url":"https://x",
            "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
    };
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":140,"incomplete_results":false,"items":[item("busy.rs"), item("calm.rs")]
        })))
        .mount(&server)
        .await;
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
    let query: GhSearchCodeQuery =
        serde_json::from_value(json!({"owner":"a","repo":"b","keywords":["wrap_app"]}))
            .expect("query");
    let context = default_context();
    let out = execute(&provider, &query, &context, &Passthrough)
        .await
        .expect("search");
    let (busy, calm) = (&out.data["files"][0], &out.data["files"][1]);
    assert_eq!(
        busy["lines"].as_array().map(Vec::len),
        Some(5),
        "{}",
        out.data
    );
    assert_eq!(busy["matchCount"], 8, "{}", out.data);
    assert!(calm.get("matchCount").is_none(), "{}", out.data);
    // The page's total counts files (GitHub's `total_count`), not lines.
    assert_eq!(out.data["pagination"]["totalItems"], 140, "{}", out.data);
    assert_eq!(out.data["pagination"]["pageSize"], 50, "{}", out.data);
    let reads = &out.data["next"]["readHits"]["input"]["queries"];
    assert_eq!(reads.as_array().map(Vec::len), Some(1), "{}", out.data);
    assert_eq!(reads[0]["path"], "src/busy.rs", "{}", out.data);
    assert_eq!(reads[0]["ref"], SHA, "{}", out.data);
    // `ranges` are 1-based line numbers, never byte offsets: from the line
    // after the last shown hit (5) to the file's last line (8), so the read
    // returns exactly the hits the listing did not (lines 6-8).
    assert_eq!(
        reads[0]["ranges"],
        json!([{"startLine":6,"endLine":8}]),
        "{}",
        out.data
    );
    assert_eq!(reads[0]["matchString"], json!(["wrap_app"]), "{}", out.data);
}

/// A search page of `names` (paths in `a/b`), GitHub reporting `total` matches.
async fn mount_search_page(server: &MockServer, page: usize, total: usize, names: &[&str]) {
    let items = names
        .iter()
        .map(|name| {
            json!({"name":name,"path":name,"sha":"1","html_url":"https://x",
             "repository":{"full_name":"a/b","html_url":"https://x","url":"https://x"}})
        })
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/search/code"))
        .and(wiremock::matchers::query_param("page", page.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_count":total,"incomplete_results":false,"items":items
        })))
        .mount(server)
        .await;
}

async fn search_page_of(
    provider: &octocode_github::GitHubProvider,
    page: usize,
) -> crate::response::result::ToolData {
    let query: GhSearchCodeQuery = serde_json::from_value(json!({
        "owner":"a","repo":"b","keywords":["needle"],"matchIn":["path"],"page":page
    }))
    .expect("query");
    execute(provider, &query, &default_context(), &Passthrough)
        .await
        .expect("search")
}

fn warnings_of(out: &crate::response::result::ToolData) -> &serde_json::Value {
    &out.data["warnings"]
}

/// A paged search page states the repeats it found: a later page drops a
/// file an earlier cached page listed, and counts the earlier pages it could
/// check. An uncached earlier page keeps its repeats listed and adds no note
/// (a jump to a deep page dropped nothing), nor does a clean first page:
/// that totals are estimates is static and lives in the tool description.
#[tokio::test]
async fn paged_pages_state_estimates_and_the_repeats_they_dropped() {
    let checked =
        "Repeat check read 1 of 1 earlier page; pages no longer cached were not compared.";
    let dropped = "Dropped 1 file GitHub listed again (already on an earlier page or this one).";
    for (cached, page, paths, warnings) in [
        (true, 1, vec!["a.rs", "b.rs"], serde_json::Value::Null),
        (true, 2, vec!["c.rs"], json!([dropped, checked])),
        (false, 2, vec!["b.rs", "c.rs"], serde_json::Value::Null),
    ] {
        let server = MockServer::builder().start().await;
        mount_search_page(&server, 1, 100, &["a.rs", "b.rs"]).await;
        mount_search_page(&server, 2, 100, &["b.rs", "c.rs"]).await;
        let mut provider = one_shot_provider(&server);
        if cached {
            provider.transport.cache = std::sync::Arc::new(MemoryCache::default());
        }
        let mut out = search_page_of(&provider, 1).await;
        if page == 2 {
            out = search_page_of(&provider, 2).await;
        }
        assert_eq!(out.data["pagination"]["totalItems"], 100, "{}", out.data);
        assert_eq!(paths_of(&out), paths, "cached {cached}: {}", out.data);
        assert_eq!(
            warnings_of(&out),
            &warnings,
            "cached {cached}: {}",
            out.data
        );
    }
}

/// A page made only of repeats is not an empty search: it keeps its
/// pagination and `next.nextPage`, and claims no absence.
#[tokio::test]
async fn a_page_of_only_repeats_is_not_an_empty_search() {
    let server = MockServer::builder().start().await;
    mount_search_page(&server, 1, 150, &["a.rs", "b.rs"]).await;
    mount_search_page(&server, 2, 150, &["a.rs", "b.rs"]).await;
    let mut provider = one_shot_provider(&server);
    provider.transport.cache = std::sync::Arc::new(MemoryCache::default());
    search_page_of(&provider, 1).await;
    let second = search_page_of(&provider, 2).await;
    assert!(paths_of(&second).is_empty(), "{}", second.data);
    assert_ne!(second.status, Some("empty"), "{}", second.data);
    assert_eq!(
        second.data["pagination"]["hasMore"], true,
        "{}",
        second.data
    );
    assert!(
        second.data["next"]["nextPage"].is_object(),
        "{}",
        second.data
    );
    assert_eq!(
        warnings_of(&second),
        &json!([
            "Dropped 2 files GitHub listed again (already on an earlier page or this one).",
            "Repeat check read 1 of 1 earlier page; pages no longer cached were not compared."
        ]),
        "{}",
        second.data
    );
    let requests = server.received_requests().await.expect("requests");
    assert!(
        requests
            .iter()
            .all(|request| request.url.path() == "/api/v3/search/code"),
        "{:?}",
        requests.iter().map(|r| r.url.path()).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn empty_index_pages_keep_the_collection_when_partial_normalization_answers() {
    for incomplete in [false, true] {
        for fields in [json!([]), json!(["path"])] {
            let server = MockServer::builder().start().await;
            Mock::given(method("GET"))
                .and(path("/api/v3/search/code"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "total_count": 0, "incomplete_results": incomplete, "items": []
                })))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b/commits/HEAD"))
                .respond_with(ResponseTemplate::new(200).set_body_string(SHA))
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/api/v3/repos/a/b"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "full_name": "a/b", "default_branch": "main", "archived": false
                })))
                .mount(&server)
                .await;
            let mut row = json!({"owner": "a", "repo": "b", "keywords": ["marker"]});
            if fields != json!([]) {
                row["fields"] = fields;
            }
            let query: GhSearchCodeQuery = serde_json::from_value(row.clone()).expect("query");
            let provider = one_shot_provider(&server);
            let context = default_context();
            let out = execute(&provider, &query, &context, &Passthrough)
                .await
                .expect("provider search");
            if incomplete {
                assert_eq!(out.data["files"], json!([]), "{}", out.data);
                assert_eq!(out.data["isPartial"], true, "{}", out.data);
                assert!(
                    out.data["partialReasons"]
                        .as_array()
                        .expect("reasons")
                        .contains(&json!("providerIncompleteResults")),
                    "{}",
                    out.data
                );
                let retry = &out.data["next"]["retry"];
                assert_eq!(retry["tool"], "ghSearchCode", "{retry}");
                let prepared =
                    crate::contracts::validate_built("ghSearchCode", retry["input"].clone())
                        .expect("executable retry");
                let continued = &prepared["queries"][0];
                for field in ["owner", "repo", "keywords", "fields"] {
                    assert_eq!(
                        continued[field], row[field],
                        "retry changed {field}: {retry}"
                    );
                }
                assert_eq!(continued["page"], 1, "{retry}");
            } else {
                assert_eq!(out.status, Some("empty"), "{}", out.data);
                assert!(out.data["next"].get("retry").is_none(), "{}", out.data);
                assert_ne!(out.data["isPartial"], true, "{}", out.data);
            }
            let shaped = crate::response::rows::result_row(
                crate::id::ToolId::GhSearchCode,
                0,
                &row,
                out.data,
                out.status,
            );
            // An empty page stays empty when GitHub's results were
            // incomplete: its only call is a retry of the same page.
            assert_eq!(shaped["status"], "empty", "{shaped}");
            crate::tools::test_support::validate_sealed(
                "ghSearchCode",
                &json!({"results": [shaped]}),
            )
            .expect("the actual normalized row satisfies the canonical output contract");
        }
    }
}
