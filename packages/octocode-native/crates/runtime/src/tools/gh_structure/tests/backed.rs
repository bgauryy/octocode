//! ghStructure against the mock server: listings, pinned pages, materialize.
use crate::tools::gh_shared::test_support::backed::*;

/// ghStructure: `pattern` finds a file by name at any depth in one
/// call, keeping the dir/files row shape.
#[tokio::test]
async fn tree_pattern_finds_paths_by_name_at_any_depth() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    let tree = json!([
        {"path":"starlette","type":"tree"},
        {"path":"starlette/_exception_handler.py","type":"blob","size":3},
        {"path":"starlette/routing.py","type":"blob","size":3},
        {"path":"tests","type":"tree"},
        {"path":"tests/test_exception_handler.py","type":"blob","size":3},
        {"path":"docs/exceptions.md","type":"blob","size":3}
    ]);
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
        200,
        json!({"sha":TREE_SHA,"tree":tree,"truncated":false}),
    )
    .await;
    let find = |pattern: &str| {
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                "ref":"main","include":[pattern]})
    };
    let out = run(&server, find("**/_exception_handler.py"))
        .await
        .expect("glob");
    assert_eq!(
        out.data["files"],
        json!([{"dir":"starlette","files":["_exception_handler.py (3)"]}]),
        "{}",
        out.data
    );
    assert_eq!(out.data["summary"]["files"], 1);
    let bare = run(&server, find("Exception_Handler"))
        .await
        .expect("bare word");
    assert_eq!(bare.data["summary"]["files"], 2, "{}", bare.data);
    let none = run(&server, find("**/nope.py")).await.expect("no match");
    assert_eq!(none.status, Some("empty"));
    // The empty status is the fact; the broader listing of the same commit
    // without the globs shows what the tree holds.
    assert!(none.data.get("hints").is_none(), "{}", none.data);
    assert_eq!(
        none.data["next"]["broadenSearch"]["query"]["queries"],
        json!([{"owner":"a","repo":"b","ref":TREE_SHA}]),
        "{}",
        none.data
    );
    // `include` matches relative to `path`: under `starlette` a glob that
    // repeats it names a `starlette/starlette/…` path.
    let mut under = find("starlette/routing.py");
    under["path"] = json!("starlette");
    let under = run(&server, under).await.expect("path-anchored glob");
    assert_eq!(under.status, Some("empty"), "{}", under.data);
    let invalid = run(&server, find("src/[")).await.expect_err("bad glob");
    assert_eq!(invalid.kind, ProviderErrorKind::Validation);
}

/// ghStructure: a recursive listing fetches its tree once, by the
/// resolved commit SHA. GitHub's tree response carries the tree
/// object's SHA, never the commit's, so a tree fetched by ref name
/// cannot be proven to belong to the resolved commit.
#[tokio::test]
async fn deep_tree_is_fetched_once_at_the_resolved_commit() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    let tree_object = "b".repeat(40);
    Mock::given(method("GET"))
            .and(path("/api/v3/repos/a/b/git/trees/main"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"sha":tree_object,"truncated":false,"tree":[{"path":"by-ref.rs","type":"blob","size":1}]}),
            ))
            .expect(0)
            .mount(&server)
            .await;
    Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"sha":tree_object,"truncated":false,"tree":[{"path":"pinned.rs","type":"blob","size":1}]}),
            ))
            .expect(1)
            .mount(&server)
            .await;
    let out = run(
        &server,
        json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b",
                "ref":"main","maxDepth":3}),
    )
    .await
    .expect("tree");
    assert_eq!(
        out.data["files"][0]["files"],
        json!(["pinned.rs (1)"]),
        "{}",
        out.data
    );
    assert_eq!(out.data["commitSha"], TREE_SHA);
    let requests = server.received_requests().await.unwrap_or_default();
    let paths = requests
        .iter()
        .map(|request| request.url.path().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        paths,
        [
            "/api/v3/repos/a/b/commits/main".to_owned(),
            format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
        ],
        "no speculative tree request, and no dating request unless extras details asks"
    );
}

/// A git tree holds only committed content, so directories a local walk
/// prunes as build output (`node_modules`, `build`, `coverage`) are listed,
/// walked, and materialized like any other: by the recursive tree and by the
/// Contents walk alike, with nothing omitted and no lead to recover them.
#[tokio::test]
async fn committed_build_output_names_are_listed_and_walked() {
    let expected = json!([
        {"dir":".","files":["app.rs (3)"],"folders":["coverage","node_modules"]},
        {"dir":"coverage","files":["coverage.go (2)"]},
        {"dir":"node_modules","files":["kept.rs (4)"]}
    ]);
    // Recursive Git Trees listing.
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
        200,
        json!({
        "sha":"c".repeat(40),"truncated":false,"tree":[
            {"path":"app.rs","type":"blob","size":3},
            {"path":"coverage","type":"tree"},
            {"path":"coverage/coverage.go","type":"blob","size":2},
            {"path":"node_modules","type":"tree"},
            {"path":"node_modules/kept.rs","type":"blob","size":4}
        ]}),
    )
    .await;
    mount_content(&server, "app.rs", "app").await;
    mount_content(&server, "coverage/coverage.go", "go").await;
    mount_content(&server, "node_modules/kept.rs", "kept").await;
    let home = crate::tools::test_support::Fixture::new();
    let out = run_at(
        &server,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                "ref":"main","maxDepth":5,"materialize":true}),
        &home.0,
    )
    .await
    .expect("recursive tree");
    let data = &out.data;
    assert_eq!(data["files"], expected, "{data}");
    assert!(data.get("omitted").is_none(), "{data}");
    assert!(data.pointer("/next/includeSkipped").is_none(), "{data}");
    let local = std::path::Path::new(
        data["location"]["localPath"]
            .as_str()
            .expect("materialized location"),
    );
    assert!(local.join("node_modules/kept.rs").exists(), "{data}");
    assert!(local.join("coverage/coverage.go").exists(), "{data}");

    // Contents walk (the recursive tree is unavailable).
    let walked = MockServer::builder().start().await;
    mount_ref(&walked, "main").await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("/git/trees/"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&walked)
        .await;
    mount_json(
        &walked,
        "/api/v3/repos/a/b/contents",
        200,
        json!([
            {"name":"app.rs","path":"app.rs","type":"file","size":3},
            {"name":"coverage","path":"coverage","type":"dir"},
            {"name":"node_modules","path":"node_modules","type":"dir"}
        ]),
    )
    .await;
    mount_json(
        &walked,
        "/api/v3/repos/a/b/contents/coverage",
        200,
        json!([{"name":"coverage.go","path":"coverage/coverage.go","type":"file","size":2}]),
    )
    .await;
    mount_json(
        &walked,
        "/api/v3/repos/a/b/contents/node_modules",
        200,
        json!([{"name":"kept.rs","path":"node_modules/kept.rs","type":"file","size":4}]),
    )
    .await;
    let out = run(
        &walked,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                "ref":"main","maxDepth":5}),
    )
    .await
    .expect("contents walk");
    assert_eq!(out.data["files"], expected, "{}", out.data);
    assert!(out.data.get("omitted").is_none(), "{}", out.data);
}

/// A one-level listing reads the root's own git tree, so a symlink is
/// typed by its mode there as in a deep listing: named, not listed as a
/// file the Contents API would type `file`.
#[tokio::test]
async fn a_one_level_listing_types_symlinks_like_a_deep_one() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    let entries = json!([
        {"path":"README.md","type":"blob","mode":"100644","size":5},
        {"path":"CHANGELOG.md","type":"blob","mode":"120000","size":19},
        {"path":"src","type":"tree","mode":"040000"}
    ]);
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
        200,
        json!({"sha":"c".repeat(40),"truncated":false,"tree":entries}),
    )
    .await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(format!(
            "/git/trees/{TREE_SHA}(:|%3A)src$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha":"d".repeat(40),"truncated":false,"tree":[
                {"path":"lib.rs","type":"blob","mode":"100644","size":7},
                {"path":"link.rs","type":"blob","mode":"120000","size":6}
            ]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("/contents"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let root = run(
        &server,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                "ref":"main"}),
    )
    .await
    .expect("root level");
    assert_eq!(
        root.data["files"],
        json!([{"dir":".","files":["README.md (5)"],"folders":["src"]}]),
        "{}",
        root.data
    );
    assert_eq!(root.data["omitted"], json!({"CHANGELOG.md (symlink)":1}));
    let deep = run(
        &server,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                "ref":"main","maxDepth":2}),
    )
    .await
    .expect("two levels");
    assert_eq!(deep.data["omitted"], json!({"CHANGELOG.md (symlink)":1}));
    let nested = run(
        &server,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                "ref":"main","path":"src"}),
    )
    .await
    .expect("nested level");
    assert_eq!(
        nested.data["files"],
        json!([{"dir":"src","files":["lib.rs (7)"]}]),
        "{}",
        nested.data
    );
    assert_eq!(nested.data["omitted"], json!({"src/link.rs (symlink)":1}));
}

#[tokio::test]
async fn tree_missing_path_on_existing_branch_does_not_fall_back() {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/missing"))
        .and(query_param("ref", TREE_SHA))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message":"Not Found"})))
        .mount(&server)
        .await;
    // The path exists on the default branch only: the read must report
    // it missing on `dev`, not show the default-branch copy.
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/missing"))
        .and(query_param("ref", "main"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"name":"x.rs","path":"missing/x.rs","type":"file","size":1}
        ])))
        .mount(&server)
        .await;
    mount_ref(&server, "dev").await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main"}),
    )
    .await;
    let error = run(
            &server,
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"dev","path":"missing"}),
        )
        .await
        .expect_err("path missing on an existing branch");
    assert_eq!(error.kind, ProviderErrorKind::NotFound);
}

/// An explicit ref that does not exist is an error naming it,
/// never a silent listing of the default branch.
#[tokio::test]
async fn tree_missing_branch_is_an_error_not_a_default_branch_listing() {
    let server = MockServer::builder().start().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/commits/gone",
        422,
        json!({"message":"No commit found for SHA: gone"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b",
        200,
        json!({"default_branch":"main"}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!([
            {"name":"x.rs","path":"x.rs","type":"file","size":1}
        ]),
    )
    .await;
    let error = run(
            &server,
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"gone"}),
        )
        .await
        .expect_err("a missing ref is an error");
    assert_eq!(error.kind, ProviderErrorKind::NotFound);
    assert_eq!(
        error.reason,
        Some(octocode_github::ProviderErrorReason::RefNotFound)
    );
    assert!(error.message.contains("\"gone\""), "{}", error.message);
}

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

#[tokio::test]
async fn tree_on_a_file_path_is_a_clear_error() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Flib.rs",
        200,
        json!({
            "name":"lib.rs","path":"src/lib.rs","type":"file","size":3,"sha":"1"
        }),
    )
    .await;
    let error = run(
            &server,
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":"main","path":"src/lib.rs"}),
        )
        .await
        .expect_err("file path");
    assert_eq!(error.kind, ProviderErrorKind::Validation);
    assert!(error.message.contains("is a file"), "{}", error.message);
}

#[tokio::test]
async fn materialize_skips_binary_files_with_a_warning() {
    let server = MockServer::builder().start().await;
    let sha = "0123456789abcdef0123456789abcdef01234567";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!([
            {"name":"img.png","path":"img.png","type":"file","size":4},
            {"name":"ok.rs","path":"ok.rs","type":"file","size":4}
        ]),
    )
    .await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/commits/{sha}"),
        200,
        json!({"sha":sha}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/img.png",
        200,
        json!({
            "type":"file","encoding":"base64","content":STANDARD.encode([0u8, 1, 2, 3])
        }),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/ok.rs",
        200,
        json!({
            "type":"file","encoding":"base64","content":STANDARD.encode("fn x(){}")
        }),
    )
    .await;
    let home = crate::tools::test_support::Fixture::new();
    let out = run_at(
            &server,
            json!({"operation":"tree","mainGoal": "test", "reasoning":"test","owner":"a","repo":"b","ref":sha,"materialize":true}),
            &home.0,
        )
        .await
        .expect("materialize continues past binary files");
    let local = out.data["location"]["localPath"]
        .as_str()
        .expect("location");
    assert!(std::path::Path::new(local).join("ok.rs").exists());
    // The warning carries the count; `location.skipped` names the file.
    assert_eq!(
        out.data["location"]["skipped"],
        json!(["img.png"]),
        "{}",
        out.data
    );
    let warnings = out.data["warnings"].to_string();
    assert!(
        warnings.contains("Skipped 1 unreadable file(s)"),
        "{}",
        out.data
    );
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

/// A listing leads to the read a caller makes next: the package's
/// entry file (outlined) in a wide directory, at the listed commit.
#[tokio::test]
async fn a_listing_leads_to_an_outline_read_of_its_entry_file() {
    let server = MockServer::builder().start().await;
    let files = (0..12)
            .map(|n| json!({"name":format!("m{n}.py"),"path":format!("pkg/m{n}.py"),"type":"file","size":4}))
            .chain(std::iter::once(json!({"name":"__init__.py","path":"pkg/__init__.py","type":"file","size":4})))
            .collect::<Vec<_>>();
    mount_listing(&server, "pkg", json!(files)).await;
    let out = run(
        &server,
        json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"pkg"}),
    )
    .await
    .expect("listing");
    let read = &out.data["next"]["read"];
    assert_eq!(read["tool"], "ghGetFileContent", "{}", out.data);
    let row = &read["query"]["queries"][0];
    assert_eq!(row["path"], "pkg/__init__.py", "{}", out.data);
    assert_eq!(row["ref"], SHA, "{}", out.data);
    assert_eq!(row["minify"], "symbols", "{}", out.data);
}

/// A materialized listing points at the listed directory on disk.
#[tokio::test]
async fn materialize_leads_into_the_local_copy_of_the_listed_directory() {
    let server = MockServer::builder().start().await;
    mount_listing(
        &server,
        "src",
        json!([{"name":"lib.rs","path":"src/lib.rs","type":"file","size":8}]),
    )
    .await;
    mount_content(&server, "src/lib.rs", "fn x(){}").await;
    let home = crate::tools::test_support::Fixture::new();
    let out = run_at(
            &server,
            json!({"operation":"tree","owner":"a","repo":"b","ref":SHA,"path":"src","materialize":true}),
            &home.0,
        )
        .await
        .expect("materialize");
    let local = out.data["location"]["localPath"]
        .as_str()
        .expect("location")
        .to_owned();
    assert!(local.ends_with("/src"), "{}", out.data);
    assert!(std::path::Path::new(&local).join("lib.rs").exists());
    // No lead re-lists the tree this response just returned.
    assert!(
        out.data.pointer("/next/exploreClone").is_none(),
        "{}",
        out.data
    );
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
