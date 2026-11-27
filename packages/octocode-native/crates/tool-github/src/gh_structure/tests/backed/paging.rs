//! Listing pages: pinned commits, deep pages, omitted entries, the fetch cap and shared reads.
use super::*;
use crate::gh_structure;
use octocode_contract_types::GhStructureQuery;
use octocode_test_support::json_fixture;
use octocode_test_support::mock_http::mount_json;
use octocode_tools_shared::gh_shared::test_support::{default_context, one_shot_provider};
use serde_json::json;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A listing names the commit it read, and its next page reads
/// that same commit even if the branch moves.
#[tokio::test]
async fn tree_pins_its_pages_to_the_resolved_commit() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json_fixture(r#"{"default_branch": "main"}"#),
    )
    .await;
    mount_ref(&server, "HEAD").await;
    let entries = json!(
        (0..=octocode_runtime_base::contracts::query_limits::gh_structure::tree::LIMIT_DEFAULT)
            .map(|i| json!({"name":format!("f{i:03}.rs"),"path":format!("f{i:03}.rs"),"type":"file","size":1}))
            .collect::<Vec<_>>()
    );
    octocode_test_support::mock_http::mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        TREE_SHA,
        "",
        &entries,
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", TREE_SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(entries))
        .mount(&server)
        .await;
    let out = run(
        &server,
        json_fixture(r#"{"operation": "tree", "owner": "a", "repo": "b"}"#),
    )
    .await
    .expect("tree");
    assert_eq!(out.data["ref"], "main", "{}", out.data);
    assert_eq!(out.data["commitSha"], TREE_SHA, "{}", out.data);
    assert_eq!(
        out.data["next"]["nextPage"]["input"]["queries"][0]["ref"],
        TREE_SHA
    );
}

/// The listing's `summary` counts the whole listing of the pinned commit,
/// the same on every page: the first page states it, a later page does not
/// repeat it.
#[tokio::test]
async fn a_later_tree_page_does_not_repeat_the_summary() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json_fixture(r#"{"default_branch": "main"}"#),
    )
    .await;
    mount_ref(&server, "HEAD").await;
    let entries = json!(
        (0..5)
            .map(|i| json!({"name":format!("f{i}.rs"),"path":format!("f{i}.rs"),"type":"file","size":1}))
            .collect::<Vec<_>>()
    );
    octocode_test_support::mock_http::mount_folder_tree(
        &server,
        "/api/v3/repos/a/b",
        TREE_SHA,
        "",
        &entries,
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", TREE_SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(entries))
        .mount(&server)
        .await;
    let first = run(
        &server,
        json!({"operation": "tree", "owner": "a", "repo": "b", "limit": 2}),
    )
    .await
    .expect("tree");
    assert_eq!(first.data["summary"]["fileCount"], 5, "{}", first.data);
    let mut later = first.data["next"]["nextPage"]["input"]["queries"][0].clone();
    while later.is_object() {
        let page = run(&server, later).await.expect("later page");
        assert!(page.data["files"].is_array(), "{}", page.data);
        assert!(page.data.get("summary").is_none(), "{}", page.data);
        later = page.data["next"]["nextPage"]["input"]["queries"][0].clone();
    }
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
        json_fixture(
            r#"{
                "operation": "tree",
                "owner": "a",
                "repo": "b",
                "ref": "main",
                "maxDepth": 5
            }"#,
        ),
    )
    .await
    .expect("tree");
    let files = out.data["summary"]["fileCount"].as_u64().unwrap_or(0);
    assert!(files >= 5, "page 1 lists files: {}", out.data);
}

/// Nothing a commit tracks is omitted: a one-level listing names
/// `node_modules` like the committed `vendor/` and `.DS_Store` beside it.
#[tokio::test]
async fn tree_reports_omitted_entries_and_next_page_drops_completed_metadata() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    mount_listing(
        &server,
        "",
        json_fixture(
            r#"[
                {
                    "name": "node_modules",
                    "path": "node_modules",
                    "type": "dir"
                },
                {"name": "vendor", "path": "vendor", "type": "dir"},
                {
                    "name": ".DS_Store",
                    "path": ".DS_Store",
                    "type": "file",
                    "size": 1
                },
                {"name": "a.rs", "path": "a.rs", "type": "file", "size": 4},
                {"name": "b.rs", "path": "b.rs", "type": "file", "size": 4}
            ]"#,
        ),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/branches",
        200,
        json_fixture(r#"[{"name": "main"}]"#),
    )
    .await;
    let out = run(
        &server,
        json_fixture(
            r#"{
                "operation": "tree",
                "owner": "a",
                "repo": "b",
                "ref": "main"
            }"#,
        ),
    )
    .await
    .expect("tree");
    assert!(out.data.get("omitted").is_none(), "{}", out.data);
    assert_eq!(out.data["summary"]["fileCount"], 3, "{}", out.data);
    assert_eq!(out.data["summary"]["dirCount"], 2, "{}", out.data);
    assert!(out.data["next"].get("nextPage").is_none(), "{}", out.data);
    assert!(out.data.get("pagination").is_none());
}

/// The folder-by-folder walk reads every directory the listing reaches:
/// no fetch count ends it (GitHub sets no such rule; its rate limits and the
/// request deadline are the bounds, each a typed failure, never a silent
/// cut), and the walk reads every directory's own tree once.
#[tokio::test]
async fn tree_fallback_walk_reads_every_directory() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("/git/trees/"))
        .and(query_param("recursive", "1"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let dirs = (0..250)
        .map(|n| json!({"name":format!("d{n}"),"path":format!("d{n}"),"type":"dir"}))
        .collect::<Vec<_>>();
    mount_listing(&server, "", json!(dirs)).await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            "/git/trees/[0-9a-f]+(:|%3A)d[0-9]+$",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"sha":TREE_SHA,"truncated":false,"tree":[]})),
        )
        .mount(&server)
        .await;
    let out = run(
        &server,
        json_fixture(
            r#"{
                "operation": "tree",
                "owner": "a",
                "repo": "b",
                "ref": "main",
                "maxDepth": 2
            }"#,
        ),
    )
    .await
    .expect("tree");
    assert!(out.data.get("terminalLimit").is_none(), "{}", out.data);
    assert!(out.data.get("providerLimit").is_none(), "{}", out.data);
    assert_eq!(out.data["summary"]["dirCount"], 250, "{}", out.data);
    let requests = server.received_requests().await.unwrap_or_default();
    let folder_trees = requests
        .iter()
        .filter(|request| {
            request.url.path().contains("/git/trees/")
                && !request.url.query_pairs().any(|(key, _)| key == "recursive")
        })
        .count();
    assert_eq!(folder_trees, 1 + 250, "the root and each directory once");
}

/// A directory at the Contents API's entry cap whose own git tree cannot be
/// read lists only what the cap returned. The row states the cap's value
/// (`providerLimit.maxEntriesPerDirectory`; the partial reason says why) in
/// the shape the published output contract declares.
#[tokio::test]
async fn a_directory_at_the_contents_cap_states_the_cap_in_the_published_shape() {
    use octocode_tools_shared::gh_shared::CONTENTS_LIMIT;
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json_fixture(r#"{"default_branch": "main"}"#),
    )
    .await;
    mount_ref(&server, "HEAD").await;
    let entries = (0..CONTENTS_LIMIT)
        .map(
            |n| json!({"name":format!("f{n}.rs"),"path":format!("f{n}.rs"),"type":"file","size":1}),
        )
        .collect::<Vec<_>>();
    mount_json(&server, "/api/v3/repos/a/b/contents", 200, json!(entries)).await;
    let out = run(
        &server,
        json_fixture(r#"{"operation": "tree", "owner": "a", "repo": "b"}"#),
    )
    .await
    .expect("listing");
    assert_eq!(
        out.data["providerLimit"],
        json!({"maxEntriesPerDirectory": CONTENTS_LIMIT}),
        "{}",
        out.data
    );
    octocode_runtime_base::test_support::assert_row_contract(
        octocode_runtime_base::id::ToolId::GhStructure,
        &out.data,
    );
}

/// A listing without a ref names its default branch from one
/// repository lookup per repository, not one per call.
#[tokio::test]
async fn the_default_branch_is_looked_up_on_each_request() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "HEAD").await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json_fixture(
            r#"{"default_branch": "main", "full_name": "a/b"}"#,
        )))
        .expect(2)
        .mount(&server)
        .await;
    mount_listing(
        &server,
        "",
        json_fixture(
            r#"[
                {"name": "a.rs", "path": "a.rs", "type": "file", "size": 1}
            ]"#,
        ),
    )
    .await;
    let provider = one_shot_provider(&server);
    let context = default_context();
    let home_dir = octocode_runtime_base::test_support::Fixture::new();
    let home = home_dir.0.clone();
    for _ in 0..2 {
        let query = serde_json::from_value(json_fixture(
            r#"{"operation": "tree", "owner": "a", "repo": "b"}"#,
        ))
        .expect("tree query");
        let out = gh_structure::execute(&provider, &query, &context, &home, None)
            .await
            .expect("listing");
        assert_eq!(out.data["ref"], "main", "{}", out.data);
    }
}

/// Concurrent rows each read the pinned recursive tree.
#[tokio::test]
async fn concurrent_rows_on_one_commit_read_its_tree_independently() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    let tree = octocode_test_support::mock_http::Delayed::new(
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
    let provider = one_shot_provider(&server);
    let context = default_context();
    let home = octocode_runtime_base::test_support::Fixture::new();
    let row = |depth: u32| -> GhStructureQuery {
        serde_json::from_value(
            json!({"operation":"tree","owner":"a","repo":"b","ref":"main","maxDepth":depth}),
        )
        .expect("tree query")
    };
    let (a, b, c) = (row(5), row(5), row(3));
    let (a, b, c) = tokio::join!(
        gh_structure::execute(&provider, &a, &context, &home.0, None),
        gh_structure::execute(&provider, &b, &context, &home.0, None),
        gh_structure::execute(&provider, &c, &context, &home.0, None),
    );
    for out in [a, b, c] {
        let out = out.expect("listing");
        assert_eq!(out.data["commitSha"], TREE_SHA, "{}", out.data);
    }
    assert_eq!(tree.arrivals.count(), 3, "independent recursive tree GETs");
}
