//! Page facts: partial pages, hit counts, estimates and the repeats a later page drops.
use super::*;
use octocode_test_support::json_fixture;

/// A resolved page retains every hit and needs no hit-recovery continuation.
#[tokio::test]
async fn a_complete_page_keeps_every_hit_without_partial_disclosures() {
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
        octocode_test_support::mock_http::list_in_trees(&server, "a", "b", &format!("src/{name}"))
            .await;
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
        &search(json_fixture(r#"["wrap_app"]"#)),
        &context,
        &Passthrough,
        &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect("search");
    assert!(out.data.get("isPartial").is_none());
    assert!(out.data.get("partialReasons").is_none());
    assert!(out.data["next"].get("readHits").is_none());
    assert_eq!(
        out.data["files"][0]["matches"],
        json!(
            (1..=8)
                .map(|n| json!(format!("{n}\twrap_app();")))
                .collect::<Vec<_>>()
        )
    );
}

/// File pagination and full hit evidence remain independent.
#[tokio::test]
async fn a_page_counts_files_and_retains_every_hit_of_each_file() {
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
        octocode_test_support::mock_http::list_in_trees(&server, "a", "b", &format!("src/{name}"))
            .await;
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/src%2F{name}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "type":"file","encoding":"base64","content":STANDARD.encode(body)
            })))
            .mount(&server)
            .await;
    }
    let provider = one_shot_provider(&server);
    let query: GhSearchCodeQuery = serde_json::from_value(json_fixture(
        r#"{"owner": "a", "repo": "b", "keywords": ["wrap_app"]}"#,
    ))
    .expect("query");
    let context = default_context();
    let out = execute(
        &provider,
        &query,
        &context,
        &Passthrough,
        &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect("search");
    let (busy, calm) = (&out.data["files"][0], &out.data["files"][1]);
    assert_eq!(
        busy["matches"].as_array().map(Vec::len),
        Some(8),
        "{}",
        out.data
    );
    assert!(
        busy.get("matchCount").is_none(),
        "complete lines already carry the count"
    );
    assert!(calm.get("matchCount").is_none(), "{}", out.data);
    // GitHub's `total_count` is an estimate a walk need not deliver: no
    // total is stated while files remain.
    assert!(out.data["pagination"]["total"].is_null(), "{}", out.data);
    assert_eq!(out.data["pagination"]["length"], 2, "{}", out.data);
    assert_eq!(out.data["pagination"]["unit"], "files", "{}", out.data);
    assert!(out.data["next"].get("readHits").is_none());
    assert_eq!(
        out.data["next"]["nextPage"]["input"]["queries"][0]["page"],
        2
    );
    assert_eq!(
        busy["matches"],
        json!(
            (1..=8)
                .map(|n| json!(format!("{n}\twrap_app();")))
                .collect::<Vec<_>>()
        )
    );
}

/// The search's page `page` (at the default limit) lists `names` (paths in
/// `a/b`), GitHub reporting `total` matches.
async fn mount_search_page(server: &MockServer, page: usize, total: usize, names: &[&str]) {
    let limit = octocode_runtime_base::contracts::query_limits::gh_search_code::LIMIT_DEFAULT;
    mount_answer(server, page, limit, total, names).await;
}

/// Page 1 of the search, or the page `previous`'s `next.nextPage` resumes.
async fn search_page_of(
    provider: &octocode_github::GitHubProvider,
    previous: Option<&octocode_runtime_base::response::result::ToolData>,
) -> octocode_runtime_base::response::result::ToolData {
    let query: GhSearchCodeQuery = match previous {
        Some(previous) => next_page(previous),
        None => serde_json::from_value(json_fixture(
            r#"{
                "owner": "a",
                "repo": "b",
                "keywords": ["needle"],
                "matchIn": ["path"]
            }"#,
        ))
        .expect("query"),
    };
    execute(
        provider,
        &query,
        &default_context(),
        &Passthrough,
        &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
    )
    .await
    .expect("search")
}

fn warnings_of(out: &octocode_runtime_base::response::result::ToolData) -> &serde_json::Value {
    &out.data["warnings"]
}

/// Page 2 of a walk drops the file page 1 listed (the walk's continuation
/// knows it) and states the shift by its reason alone: the repeat loses no
/// evidence and the reason says results moved, so no warning restates it. A
/// clean first page says nothing.
#[tokio::test]
async fn paged_pages_state_estimates_and_the_repeats_they_dropped() {
    {
        let server = MockServer::builder().start().await;
        mount_search_page(&server, 1, 100, &["a.rs", "b.rs"]).await;
        mount_search_page(&server, 2, 100, &["b.rs", "c.rs"]).await;
        let provider = one_shot_provider(&server);
        let first = search_page_of(&provider, None).await;
        assert_eq!(paths_of(&first), ["a.rs", "b.rs"], "{}", first.data);
        assert_eq!(warnings_of(&first), &serde_json::Value::Null);
        let out = search_page_of(&provider, Some(&first)).await;
        // A walk over several provider pages states no total: GitHub's
        // 100 is an estimate, and repeats were dropped on the way.
        assert!(out.data["pagination"]["total"].is_null(), "{}", out.data);
        assert_eq!(paths_of(&out), ["c.rs"], "{}", out.data);
        assert_eq!(warnings_of(&out), &serde_json::Value::Null, "{}", out.data);
        assert_eq!(
            out.data["partialReasons"],
            json_fixture(r#"["providerResultsShifted"]"#)
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
    let provider = one_shot_provider(&server);
    let first = search_page_of(&provider, None).await;
    let second = search_page_of(&provider, Some(&first)).await;
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
    // The repeats lose no evidence: the shift is its reason alone.
    assert_eq!(
        warnings_of(&second),
        &serde_json::Value::Null,
        "{}",
        second.data
    );
    assert_eq!(
        second.data["partialReasons"],
        json_fixture(r#"["providerResultsShifted"]"#),
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
        for fields in [json!([]), json_fixture(r#"["path"]"#)] {
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
                .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
                    r#"{
                        "full_name": "a/b",
                        "default_branch": "main",
                        "archived": false
                    }"#,
                )))
                .mount(&server)
                .await;
            let mut row = json_fixture(r#"{"owner": "a", "repo": "b", "keywords": ["marker"]}"#);
            if fields != json!([]) {
                row["fields"] = fields;
            }
            let query: GhSearchCodeQuery = serde_json::from_value(row.clone()).expect("query");
            let provider = one_shot_provider(&server);
            let context = default_context();
            let out = execute(
                &provider,
                &query,
                &context,
                &Passthrough,
                &octocode_runtime_base::policy::discovery::is_sensitive_repository_file,
            )
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
                let prepared = octocode_runtime_base::contracts::validate_saved(
                    "ghSearchCode",
                    retry["input"].clone(),
                )
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
            let shaped = octocode_runtime_base::response::rows::result_row(0, out.data, out.status);
            // An empty page stays empty when GitHub's results were
            // incomplete: its only call is a retry of the same page.
            assert_eq!(shaped["status"], "empty", "{shaped}");
            octocode_runtime_base::test_support::validate_sealed(
                "ghSearchCode",
                &json!({"results": [shaped]}),
            )
            .expect("the actual normalized row satisfies the canonical output contract");
        }
    }
}
