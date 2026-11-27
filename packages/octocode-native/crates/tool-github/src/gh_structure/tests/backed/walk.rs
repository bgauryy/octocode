//! What a listing walks and names: patterns, the one tree read, build-output names, symlinks.
use super::*;
use octocode_github::ProviderErrorKind;
use octocode_test_support::json_fixture;
use octocode_test_support::mock_http::mount_json;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// ghStructure: `pattern` finds a file by name at any depth in one
/// call, keeping the dir/files row shape.
#[tokio::test]
async fn tree_pattern_finds_paths_by_name_at_any_depth() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    let tree = json_fixture(
        r#"[
            {"path": "starlette", "type": "tree"},
            {
                "path": "starlette/_exception_handler.py",
                "type": "blob",
                "size": 3
            },
            {"path": "starlette/routing.py", "type": "blob", "size": 3},
            {"path": "tests", "type": "tree"},
            {
                "path": "tests/test_exception_handler.py",
                "type": "blob",
                "size": 3
            },
            {"path": "docs/exceptions.md", "type": "blob", "size": 3}
        ]"#,
    );
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
        200,
        json!({"sha":TREE_SHA,"tree":tree,"truncated":false}),
    )
    .await;
    let find = |pattern: &str| {
        json!({"operation":"tree", "owner":"a","repo":"b",
                "ref":"main","include":[pattern]})
    };
    let out = run(&server, find("**/_exception_handler.py"))
        .await
        .expect("glob");
    assert_eq!(
        out.data["files"],
        json_fixture(r#"["starlette/_exception_handler.py (3)"]"#),
        "{}",
        out.data
    );
    assert_eq!(out.data["summary"]["fileCount"], 1);
    let literal = run(&server, find("Exception_Handler"))
        .await
        .expect("literal glob");
    assert_eq!(literal.data["summary"]["fileCount"], 0);
    let bare = run(&server, find("**/*exception_handler*"))
        .await
        .expect("bare word");
    assert_eq!(bare.data["summary"]["fileCount"], 2, "{}", bare.data);
    let none = run(&server, find("**/nope.py")).await.expect("no match");
    assert_eq!(none.status, Some("empty"));
    let broaden = octocode_runtime_base::test_support::checks::related_call(
        &none.data,
        "ghStructure",
        |_| true,
    );
    assert_eq!(
        broaden["input"]["queries"],
        json!([{"operation":"tree","owner":"a","repo":"b","ref":TREE_SHA}])
    );
    octocode_runtime_base::contracts::validate("ghStructure", broaden["input"].clone())
        .expect("canonical fresh listing");
    // A listing scoped to a directory reads that directory's own tree.
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(format!(
            "/git/trees/{TREE_SHA}(:|%3A)starlette$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha":"e".repeat(40),"truncated":false,"tree":[
                {"path":"_exception_handler.py","type":"blob","size":3},
                {"path":"routing.py","type":"blob","size":3}
            ]
        })))
        .mount(&server)
        .await;
    // Globs read paths below the listed path; the row stays repo-relative.
    let mut under = find("routing.py");
    under["path"] = json!("starlette");
    let under = run(&server, under)
        .await
        .expect("glob relative to the listed path");
    assert_eq!(under.data["summary"]["fileCount"], 1);
    assert_eq!(
        under.data["files"],
        json_fixture(r#"["starlette/routing.py (3)"]"#)
    );
    let invalid = run(&server, find("src/[")).await.expect_err("bad glob");
    assert_eq!(invalid.kind, ProviderErrorKind::Validation);
}

/// An `include` entry that is a bare word matches only an entry of exactly
/// that name: the empty listing names the substring form, and its one
/// related listing asks it (never the listing without `include`).
#[tokio::test]
async fn a_bare_word_include_leads_to_its_substring_glob() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
        200,
        json!({"sha":TREE_SHA,"truncated":false,"tree":[
            {"path": "starlette", "type": "tree"},
            {"path": "starlette/routing.py", "type": "blob", "size": 3},
            {"path": "docs/index.md", "type": "blob", "size": 3}
        ]}),
    )
    .await;
    let out = run(
        &server,
        json!({"operation":"tree","owner":"a","repo":"b","ref":"main",
               "include":["routing", "*.toml"]}),
    )
    .await
    .expect("empty listing");
    assert_eq!(out.status, Some("empty"), "{}", out.data);
    assert_eq!(
        out.data["hints"],
        json!([
            "include \"routing\" matches only that exact name; \"*routing*\" matches a substring."
        ]),
        "{}",
        out.data
    );
    let related = out.data["related"].as_array().expect("related");
    assert_eq!(related.len(), 1, "{}", out.data);
    let queries = related[0]["input"]["queries"].as_array().expect("queries");
    assert_eq!(queries.len(), 1, "{}", out.data);
    assert_eq!(
        queries[0]["include"],
        json!(["*routing*", "*.toml"]),
        "{}",
        out.data
    );
    let listed = run(&server, queries[0].clone())
        .await
        .expect("substring listing");
    assert_eq!(
        listed.data["files"],
        json_fixture(r#"["starlette/routing.py (3)"]"#),
        "{}",
        listed.data
    );
}

/// An `include` glob that restates the listed `path` matches nothing, since
/// globs read paths below `path`: the empty listing says so and leads first
/// with the same globs relative to `path`.
#[tokio::test]
async fn an_include_that_restates_path_leads_to_the_path_relative_globs() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(format!(
            "/git/trees/{TREE_SHA}(:|%3A)starlette$"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha":"e".repeat(40),"truncated":false,"tree":[
                {"path":"routing.py","type":"blob","size":3},
                {"path":"sub","type":"tree"},
                {"path":"sub/a.py","type":"blob","size":3}
            ]
        })))
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
                "path": "starlette",
                "include": ["starlette/routing.py", "**/nope.py", "starlette/sub/*.py"]
            }"#,
        ),
    )
    .await
    .expect("empty listing");
    assert_eq!(out.status, Some("empty"), "{}", out.data);
    let broaden =
        octocode_runtime_base::test_support::checks::related_call(&out.data, "ghStructure", |_| {
            true
        });
    assert_eq!(
        broaden["input"]["queries"][0]["include"],
        json_fixture(r#"["routing.py", "**/nope.py", "sub/*.py"]"#),
        "{}",
        out.data
    );
    octocode_runtime_base::contracts::validate("ghStructure", broaden["input"].clone())
        .expect("canonical rebased listing");
    // The repaired listing alone, with the one hint that names the fix:
    // no listing without `include` and no generic broadening note.
    assert_eq!(
        broaden["input"]["queries"].as_array().map(Vec::len),
        Some(1),
        "{}",
        out.data
    );
    assert_eq!(
        out.data["hints"],
        json!([
            "include globs are relative to path `starlette`: \"starlette/routing.py\", \"starlette/sub/*.py\" restate it, so they match nothing below it."
        ]),
        "{}",
        out.data
    );
    let hints = out.data["hints"].to_string();
    // The related request's globs, relative to path, list what they name:
    // the rows stay repo-relative.
    let mut relative = broaden["input"]["queries"][0].clone();
    relative
        .as_object_mut()
        .expect("row")
        .retain(|key, _| !matches!(key.as_str(), "mainGoal" | "reasoning"));
    let listed = run(&server, relative).await.expect("relative globs");
    let rows = listed.data["files"].to_string();
    assert!(
        rows.contains("routing.py") && rows.contains("a.py") && rows.contains("starlette"),
        "{}",
        listed.data
    );
    // The hint names the request it explains, not a position in related[].
    assert!(!hints.contains("related[] request asks"), "{}", out.data);
    // A glob written from the root with a leading `/` loses `/path/` once.
    let rooted = run(
        &server,
        json_fixture(
            r#"{
                "operation": "tree",
                "owner": "a",
                "repo": "b",
                "ref": "main",
                "path": "starlette",
                "include": ["/starlette/nope.py"]
            }"#,
        ),
    )
    .await
    .expect("empty listing");
    let broaden = octocode_runtime_base::test_support::checks::related_call(
        &rooted.data,
        "ghStructure",
        |_| true,
    );
    assert_eq!(
        broaden["input"]["queries"][0]["include"],
        json_fixture(r#"["nope.py"]"#),
        "{}",
        rooted.data
    );
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
        json_fixture(
            r#"{
                "operation": "tree",
                "owner": "a",
                "repo": "b",
                "ref": "main",
                "maxDepth": 3
            }"#,
        ),
    )
    .await
    .expect("tree");
    assert_eq!(
        out.data["files"],
        json_fixture(r#"["pinned.rs (1)"]"#),
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
        "one pinned tree request"
    );
}

/// A git tree holds only committed content, so directories a local walk
/// prunes as build output (`node_modules`, `build`, `coverage`) are listed,
/// walked, and materialized like any other: by the recursive tree and by the
/// folder-by-folder walk alike, with nothing omitted and no lead to recover
/// them.
#[tokio::test]
async fn committed_build_output_names_are_listed_and_walked() {
    let expected = json_fixture(
        r#"[
            {
                "dir": ".",
                "files": ["app.rs (3)", "coverage/", "node_modules/"]
            },
            "coverage/coverage.go (2)",
            "node_modules/kept.rs (4)"
        ]"#,
    );
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
    let home = octocode_runtime_base::test_support::Fixture::new();
    let out = run_at(
        &server,
        json_fixture(
            r#"{
                "operation": "tree",
                "owner": "a",
                "repo": "b",
                "ref": "main",
                "maxDepth": 5,
                "materialize": true
            }"#,
        ),
        &home.0,
    )
    .await
    .expect("recursive tree");
    let data = &out.data;
    assert_eq!(data["files"], expected, "{data}");
    assert!(data.get("omitted").is_none(), "{data}");
    assert!(data.pointer("/next/includeSkipped").is_none(), "{data}");
    let local = std::path::Path::new(
        data["location"]["path"]
            .as_str()
            .expect("materialized location"),
    );
    assert!(local.join("node_modules/kept.rs").exists(), "{data}");
    assert!(local.join("coverage/coverage.go").exists(), "{data}");

    // Folder-by-folder walk (the recursive tree is unavailable).
    let walked = MockServer::builder().start().await;
    mount_ref(&walked, "main").await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex("/git/trees/"))
        .and(wiremock::matchers::query_param("recursive", "1"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&walked)
        .await;
    mount_listing(
        &walked,
        "",
        json_fixture(
            r#"[
                {
                    "name": "app.rs",
                    "path": "app.rs",
                    "type": "file",
                    "size": 3
                },
                {"name": "coverage", "path": "coverage", "type": "dir"},
                {
                    "name": "node_modules",
                    "path": "node_modules",
                    "type": "dir"
                }
            ]"#,
        ),
    )
    .await;
    mount_listing(
        &walked,
        "coverage",
        json_fixture(
            r#"[
                {
                    "name": "coverage.go",
                    "path": "coverage/coverage.go",
                    "type": "file",
                    "size": 2
                }
            ]"#,
        ),
    )
    .await;
    mount_listing(
        &walked,
        "node_modules",
        json_fixture(
            r#"[
                {
                    "name": "kept.rs",
                    "path": "node_modules/kept.rs",
                    "type": "file",
                    "size": 4
                }
            ]"#,
        ),
    )
    .await;
    let out = run(
        &walked,
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
    let entries = json_fixture(
        r#"[
            {
                "path": "README.md",
                "type": "blob",
                "mode": "100644",
                "size": 5
            },
            {
                "path": "CHANGELOG.md",
                "type": "blob",
                "mode": "120000",
                "size": 19
            },
            {
                "path": ".env",
                "type": "blob",
                "mode": "120000",
                "sha": "adadadadadadadadadadadadadadadadadadadad"
            },
            {"path": "src", "type": "tree", "mode": "040000"}
        ]"#,
    );
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
        .and(wiremock::matchers::path_regex("/contents/?$"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(
            "/api/v3/repos/a/b/git/blobs/adadadadadadadadadadadadadadadadadadadad",
        ))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let root = run(
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
    .expect("root level");
    assert_eq!(
        root.data["files"],
        json_fixture(
            r#"[
                {
                    "dir": ".",
                    "files": [".env (link)", "CHANGELOG.md (link)", "README.md (5)", "src/"]
                }
            ]"#
        ),
        "{}",
        root.data
    );
    assert!(root.data.get("omitted").is_none(), "{}", root.data);
    let deep = run(
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
    .expect("two levels");
    assert!(deep.data.get("omitted").is_none(), "{}", deep.data);
    let nested = run(
        &server,
        json_fixture(
            r#"{
                "operation": "tree",
                "owner": "a",
                "repo": "b",
                "ref": "main",
                "path": "src"
            }"#,
        ),
    )
    .await
    .expect("nested level");
    assert_eq!(
        nested.data["files"],
        json_fixture(r#"[{"dir": "src", "files": ["lib.rs (7)", "link.rs (link)"]}]"#),
        "{}",
        nested.data
    );
    assert!(nested.data.get("omitted").is_none(), "{}", nested.data);
}

/// A listing that lists a symlink and names a submodule is a
/// response the published output contract accepts: the kinds are the
/// contract's own keys and each path list is its declared collection.
#[tokio::test]
async fn a_listing_naming_a_symlink_and_a_submodule_meets_the_output_contract() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}"),
        200,
        json!({"sha":"c".repeat(40),"truncated":false,"tree":[
            {"path":"README.md","type":"blob","mode":"100644","size":5},
            {"path":"docs","type":"tree","mode":"040000"},
            {"path":"docs/link.md","type":"blob","mode":"120000","size":9},
            {"path":"vendor/lib","type":"commit","mode":"160000"}
        ]}),
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
                "maxDepth": 2
            }"#,
        ),
    )
    .await
    .expect("tree");
    assert_eq!(
        out.data["omitted"],
        json_fixture(r#"{"submodule": ["vendor/lib"]}"#),
        "{}",
        out.data
    );
    octocode_runtime_base::test_support::assert_row_contract(
        octocode_runtime_base::id::ToolId::GhStructure,
        &out.data,
    );
}

/// A deep listing of a directory reads the recursive git tree of that
/// directory (`<commit>:<path>`), never the whole repository's: a large
/// repository's own tree is truncated and costs a read per subtree, where the
/// directory's is one request. Entries keep their repository-relative paths
/// and the listing's depth.
#[tokio::test]
async fn a_deep_listing_of_a_directory_reads_that_directorys_tree_once() {
    let server = MockServer::builder().start().await;
    mount_ref(&server, "main").await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{TREE_SHA}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"sha":"b".repeat(40),"truncated":true,"tree":[{"path":"root.rs","type":"blob","size":1}]}),
        ))
        .expect(0)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(format!(
            "/git/trees/{TREE_SHA}(:|%3A)src$"
        )))
        .and(wiremock::matchers::query_param("recursive", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha":"d".repeat(40),"truncated":false,"tree":[
                {"path":"lib.rs","type":"blob","mode":"100644","size":7},
                {"path":"util","type":"tree","mode":"040000"},
                {"path":"util/deep.rs","type":"blob","mode":"100644","size":3},
                {"path":"util/more","type":"tree","mode":"040000"},
                {"path":"util/more/too_deep.rs","type":"blob","mode":"100644","size":2}
            ]
        })))
        .expect(1)
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
                "path": "src",
                "maxDepth": 2
            }"#,
        ),
    )
    .await
    .expect("directory tree");
    assert_eq!(
        out.data["files"],
        json_fixture(
            r#"[
                {"dir": "src", "files": ["lib.rs (7)", "util/"]},
                {"dir": "src/util", "files": ["deep.rs (3)", "more/"]}
            ]"#
        ),
        "{}",
        out.data
    );
    let reads = server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request| !request.url.path().contains("/commits/"))
        .count();
    assert_eq!(reads, 1, "one tree read for the whole listing");
}
