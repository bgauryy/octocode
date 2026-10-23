//! What a listing walks and names: patterns, the one tree read, build-output names, symlinks.
use super::*;

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
    // without the globs shows what the tree holds. The tool's lead keeps the
    // request brief: the response stage owns it (asserted below).
    assert_eq!(
        none.data["hints"],
        json!(["broadenSearch rows: queries[0] drops include"]),
        "{}",
        none.data
    );
    assert_eq!(
        none.data["next"]["broadenSearch"]["query"]["queries"],
        json!([{"owner":"a","repo":"b","ref":TREE_SHA,"mainGoal":"test","reasoning":"test"}]),
        "{}",
        none.data
    );
    // The response stage drops the brief from every broadened row: the
    // published lead carries none.
    let mut row = none.data.clone();
    row["status"] = json!("empty");
    let published = crate::tools::test_support::published(
        crate::id::ToolId::GhStructure,
        &find("**/nope.py"),
        &row,
    );
    let shown = published.to_string();
    assert!(
        shown.contains("broadenSearch: Approximate: ghStructure"),
        "{shown}"
    );
    assert!(
        !shown.contains("mainGoal") && !shown.contains("reasoning"),
        "{shown}"
    );
    // `include` matches relative to `path`: under `starlette` a glob that
    // repeats it names a `starlette/starlette/…` path.
    let mut under = find("starlette/routing.py");
    under["path"] = json!("starlette");
    let under = run(&server, under).await.expect("path-anchored glob");
    assert_eq!(under.status, Some("empty"), "{}", under.data);
    // The broader listing first rebases that glob onto `path`.
    assert_eq!(
        under.data["next"]["broadenSearch"]["query"]["queries"],
        json!([
            {"owner":"a","repo":"b","ref":TREE_SHA,"path":"starlette","include":["routing.py"],
                "mainGoal":"test","reasoning":"test"},
            {"owner":"a","repo":"b","ref":TREE_SHA,"path":"starlette","mainGoal":"test","reasoning":"test"}
        ]),
        "{}",
        under.data
    );
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
        {"dir":".","files":["app.rs (3)","coverage/","node_modules/"]},
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
        json!([{"dir":".","files":["README.md (5)","src/"]}]),
        "{}",
        root.data
    );
    assert_eq!(root.data["omitted"], json!(["CHANGELOG.md (symlink)"]));
    let deep = run(
        &server,
        json!({"operation":"tree","mainGoal":"test","reasoning":"test","owner":"a","repo":"b",
                "ref":"main","maxDepth":2}),
    )
    .await
    .expect("two levels");
    assert_eq!(deep.data["omitted"], json!(["CHANGELOG.md (symlink)"]));
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
    assert_eq!(nested.data["omitted"], json!(["src/link.rs (symlink)"]));
}
