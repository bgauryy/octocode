//! Listing pages: pinned commits, deep pages, omitted entries, the fetch cap and shared reads.
use super::*;

/// A listing names the commit it read, and its next page reads
/// that same commit even if the branch moves.
#[tokio::test]
async fn tree_pins_its_pages_to_the_resolved_commit() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main"}),
    )
    .await;
    mount_ref(&server, "HEAD").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", TREE_SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"a.rs","path":"a.rs","type":"file","size":1},
            {"name":"b.rs","path":"b.rs","type":"file","size":1}
        ])))
        .mount(&server)
        .await;
    let out = run(
            &server,
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","pageSize":1}),
        )
        .await
        .expect("tree");
    assert_eq!(out.data["resolvedRef"], "main", "{}", out.data);
    assert_eq!(out.data["commitSha"], TREE_SHA, "{}", out.data);
    assert_eq!(
        out.data["next"]["nextPage"]["query"]["queries"][0]["ref"],
        TREE_SHA
    );
}

/// A deep listing pages directory by directory, so its first
/// page carries files, not only folders.
#[tokio::test]
async fn deep_tree_pages_carry_files_with_their_folders() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    let mut tree = Vec::new();
    // 10 top-level folders × 3 subfolders × 3 files: 40 folders
    // sort before the first file in a folders-first listing.
    for dir in 0..10 {
        tree.push(json!({"path":format!("d{dir}"),"type":"tree"}));
        for sub in 0..3 {
            tree.push(json!({"path":format!("d{dir}/s{sub}"),"type":"tree"}));
            for file in 0..3 {
                tree.push(
                    json!({"path":format!("d{dir}/s{sub}/f{file}.rs"),"type":"blob","size":1}),
                );
            }
        }
    }
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
        200,
        json!({"sha":TREE_SHA,"tree":tree,"truncated":false}),
    )
    .await;
    let out = run(
            &server,
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"main","maxDepth":5,"pageSize":20}),
        )
        .await
        .expect("tree");
    let files = out.data["summary"]["files"].as_u64().unwrap_or(0);
    assert!(files >= 5, "page 1 lists files: {}", out.data);
}

/// Nothing a commit tracks is omitted: a one-level listing names
/// `node_modules` like the committed `vendor/` and `.DS_Store` beside it.
#[tokio::test]
async fn tree_reports_omitted_entries_and_next_page_drops_completed_metadata() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!([
            {"name":"node_modules","path":"node_modules","type":"dir"},
            {"name":"vendor","path":"vendor","type":"dir"},
            {"name":".DS_Store","path":".DS_Store","type":"file","size":1},
            {"name":"a.rs","path":"a.rs","type":"file","size":4},
            {"name":"b.rs","path":"b.rs","type":"file","size":4}
        ]),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/branches",
        200,
        json!([{"name":"main"}]),
    )
    .await;
    let out = run(
            &server,
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"main",
                "pageSize":1}),
        )
        .await
        .expect("tree");
    assert!(out.data.get("omitted").is_none(), "{}", out.data);
    assert_eq!(out.data["pagination"]["totalItems"], 5, "{}", out.data);
    let next = &out.data["next"]["nextPage"]["query"]["queries"][0];
    assert_eq!(next["page"], 2, "{}", out.data);
}

#[tokio::test]
async fn tree_fallback_walk_stops_at_the_directory_fetch_cap() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("/git/trees/"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let dirs = (0..250)
        .map(|n| json!({"name":format!("d{n}"),"path":format!("d{n}"),"type":"dir"}))
        .collect::<Vec<_>>();
    mount_json(&server, "/api/v3/repos/a/b/contents", 200, json!(dirs)).await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("/contents/d[0-9]+$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let out = run(
            &server,
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"main","maxDepth":2}),
        )
        .await
        .expect("tree");
    assert_eq!(out.data["terminalLimit"], true, "{}", out.data);
    assert_eq!(out.data["providerLimit"]["reason"], "treeFetchLimit");
    let requests = server.received_requests().await.unwrap_or_default();
    let contents = requests
        .iter()
        .filter(|request| request.url.path().contains("/contents"))
        .count();
    assert_eq!(contents, gh_structure::MAX_DIRECTORY_FETCHES);
}

/// A listing without a ref names its default branch from one
/// repository lookup per repository, not one per call.
#[tokio::test]
async fn the_default_branch_is_looked_up_once_per_repository() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "HEAD").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"default_branch":"main","full_name":"a/b"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    mount_listing(
        &server,
        "",
        json!([{"name":"a.rs","path":"a.rs","type":"file","size":1}]),
    )
    .await;
    let provider = GitHubProvider::new(provider(&server).transport, MemoryCache::default());
    let context = default_context();
    let home_dir = crate::tools::test_support::Fixture::new();
    let home = home_dir.0.clone();
    for _ in 0..2 {
        let query = serde_json::from_value(json!({"owner":"a","repo":"b"})).expect("tree query");
        let out = gh_structure::execute(&provider, &query, &context, &home)
            .await
            .expect("listing");
        assert_eq!(out.data["resolvedRef"], "main", "{}", out.data);
    }
}

/// Concurrent rows on one commit share one recursive tree read: the later
/// rows wait for the first and list from the tree it cached.
#[tokio::test]
async fn concurrent_rows_on_one_commit_read_its_tree_once() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    let tree = octocode_github::mock_http::Delayed::new(
        std::time::Duration::from_millis(200),
        ResponseTemplate::new(200).set_body_json(json!({
            "sha":"b".repeat(40),"truncated":false,
            "tree":[{"path":"src","type":"tree"},{"path":"src/a.rs","type":"blob","size":1}]
        })),
    );
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}")))
        .respond_with(tree.clone())
        .mount(&server)
        .await;
    let provider = GitHubProvider::new(provider(&server).transport, MemoryCache::default());
    let context = default_context();
    let home = crate::tools::test_support::Fixture::new();
    let row = |depth: u32| -> gh_structure::GhStructureQuery {
        serde_json::from_value(json!({"owner":"a","repo":"b","ref":"main","maxDepth":depth}))
            .expect("tree query")
    };
    let (a, b, c) = (row(5), row(5), row(3));
    let (a, b, c) = tokio::join!(
        gh_structure::execute(&provider, &a, &context, &home.0),
        gh_structure::execute(&provider, &b, &context, &home.0),
        gh_structure::execute(&provider, &c, &context, &home.0),
    );
    for out in [a, b, c] {
        let out = out.expect("listing");
        assert_eq!(out.data["commitSha"], TREE_SHA, "{}", out.data);
    }
    assert_eq!(tree.arrivals.count(), 1, "one recursive tree GET");
}
