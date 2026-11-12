//! A tree walk: only the first page lists the tree, a later page answers from
//! what its cursor owns, and no page makes a date lookup.
use super::*;

/// Walking every opaque continuation lists each entry exactly once at the
/// default `limit`, and costs the one listing however many pages it takes.
#[tokio::test]
async fn a_large_tree_lists_every_entry_once_through_opaque_continuations() {
    let names = (0..650)
        .map(|n| (format!("f{n:03}.rs"), "file"))
        .collect::<Vec<_>>();
    let server = listing_server(&names).await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let mut pending = Some(json!({"queries":[{"operation":"tree","owner":"a","repo":"b"}]}));
    let mut listed = Vec::new();
    let mut sizes = Vec::new();
    let reads = async || server.received_requests().await.unwrap().len();
    while let Some(input) = pending.take() {
        let before = reads().await;
        let outcome = call(&runtime, "ghStructure", input).await.expect("listing");
        let sent = reads().await - before;
        assert_eq!(
            sent == 0,
            !sizes.is_empty(),
            "only the first page lists the tree: {sent} requests on page {}",
            sizes.len() + 1
        );
        let data = row_data(&outcome);
        assert!(data.get("warnings").is_none(), "{data}");
        // The first response to the fresh request leaves out the fields that
        // repeat its query; a page read through a cursor keeps them, because
        // its request names none of them. Every page states the listed
        // commit as `commitSha`, and `ref` only as the branch that
        // resolved.
        for echo in ["owner", "repo"] {
            assert_eq!(
                data.get(echo).is_some(),
                !sizes.is_empty(),
                "{echo} on page {}: {data}",
                sizes.len() + 1
            );
        }
        assert_eq!(data["commitSha"], SHA, "page {}: {data}", sizes.len() + 1);
        assert!(
            data.get("ref")
                .is_none_or(|resolved| resolved == "main"),
            "page {}: {data}",
            sizes.len() + 1
        );
        let page = data["files"][0]["files"].as_array().expect("entries");
        sizes.push(page.len());
        listed.extend(page.iter().map(|entry| entry.as_str().unwrap().to_owned()));
        pending = data.pointer("/next/nextPage/input").cloned();
        if let Some(input) = &pending {
            support::assert_cursor_only(input);
        }
        assert!(sizes.len() <= 3, "bounded pages");
    }
    assert_eq!(sizes, [300, 300, 50]);
    let expected = names
        .iter()
        .map(|(name, _)| format!("{name} (10)"))
        .collect::<Vec<_>>();
    assert_eq!(listed, expected);
    assert!(graphql_requests(&server).await.is_empty());
    runtime.close().await;
}

/// A listing under `path` names each row's `dir` repo-relative, the same base
/// as `path` and `include`.
#[tokio::test]
async fn a_scoped_listing_names_repo_relative_dirs() {
    let server = listing_server(&[]).await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{SHA}:pkg")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "sha": SHA, "truncated": false, "tree": [
                {"path":"a.rs","type":"blob","size":1},
                {"path":"src","type":"tree"},
                {"path":"src/lib.rs","type":"blob","size":1}
            ]
        })))
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"operation":"tree","owner": "a", "repo": "b", "path": "pkg", "maxDepth": 2}),
    )
    .await
    .expect("listing");
    let data = row_data(&outcome);
    assert_eq!(
        data["files"],
        json!([
            {"dir": "pkg", "files": ["a.rs (1)", "src/"]},
            {"dir": "pkg/src", "files": ["lib.rs (1)"]},
        ]),
        "{data}"
    );
    assert!(graphql_requests(&server).await.is_empty());
    runtime.close().await;
}

/// A repository `a/b` at [`SHA`] whose root lists `entries` (name, type, size).
async fn sized_server(entries: &[(&str, &str, u64)]) -> MockServer {
    let server = MockServer::builder().start().await;
    mount_head_at_sha(&server).await;
    let rows = entries
        .iter()
        .map(|(name, kind, size)| json!({"name": name, "path": name, "type": kind, "size": size}))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents"))
        .and(query_param("ref", SHA))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(rows)))
        .mount(&server)
        .await;
    // Each folder is empty: a size rank opens every level.
    for (name, _, _) in entries.iter().filter(|(_, kind, _)| *kind == "dir") {
        Mock::given(method("GET"))
            .and(path(format!("/api/v3/repos/a/b/contents/{name}")))
            .and(query_param("ref", SHA))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
            .mount(&server)
            .await;
    }
    server
}

/// A size rank without `maxDepth` opens every folder, as "the largest files
/// under a folder" reads: the largest nested file leads, and the page's read
/// lead is that top row, not the listing's entry file. `maxDepth` still bounds it.
#[tokio::test]
async fn a_size_rank_opens_every_level_and_leads_to_its_top_row() {
    let server = listing_server(&[]).await;
    let top = [
        json!({"path":"index.ts","type":"blob","size":10}),
        json!({"path":"config.ts","type":"blob","size":900}),
        json!({"path":"plugins","type":"tree"}),
    ];
    let mut whole = top.to_vec();
    whole.push(json!({"path":"plugins/css.ts","type":"blob","size":1200}));
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{SHA}:node")))
        .and(query_param("recursive", "1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"sha": SHA, "truncated": false, "tree": whole})),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v3/repos/a/b/git/trees/{SHA}:node")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"sha": SHA, "truncated": false, "tree": top})),
        )
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let ranked = |depth: Option<u64>| {
        let mut row = json!({"operation":"tree","owner":"a","repo":"b","path":"node",
            "sort":{"by":"sizeBytes","order":"desc"},"limit":2});
        if let Some(depth) = depth {
            row["maxDepth"] = json!(depth);
        }
        json!({"queries":[row]})
    };
    let outcome = call(&runtime, "ghStructure", ranked(None))
        .await
        .expect("ranked");
    let data = row_data(&outcome);
    assert_eq!(
        data["files"],
        json!([
            {"dir": "node/plugins", "files": ["css.ts (1200)"]},
            {"dir": "node", "files": ["config.ts (900)"]},
        ]),
        "{data}"
    );
    let read = &crate::support::checks::related_call(data, "ghGetFileContent", |_| true)["input"]["queries"]
        [0];
    assert_eq!(read["path"], "node/plugins/css.ts", "{data}");
    // An explicit depth bounds the rank to the levels it names.
    let outcome = call(&runtime, "ghStructure", ranked(Some(1)))
        .await
        .expect("bounded");
    let data = row_data(&outcome);
    assert_eq!(data["files"][0]["files"][0], "config.ts (900)", "{data}");
    runtime.close().await;
}

/// Every entry of a listing walked through its opaque continuations, in the
/// order the pages state them.
async fn walk_sorted(
    runtime: &octocode_native::runtime::ToolRuntime,
    sort: serde_json::Value,
    limit: u64,
) -> (Vec<String>, usize) {
    let mut pending = Some(json!({"queries":[{"operation":"tree","owner":"a","repo":"b","limit":limit,"sort":sort}]}));
    let (mut listed, mut pages) = (Vec::new(), 0);
    while let Some(input) = pending.take() {
        let outcome = call(runtime, "ghStructure", input).await.expect("listing");
        let data = row_data(&outcome);
        assert!(data.get("warnings").is_none(), "{data}");
        for group in data["files"].as_array().expect("groups") {
            assert_eq!(group["dir"], ".", "{data}");
            listed.extend(
                group["files"]
                    .as_array()
                    .expect("entries")
                    .iter()
                    .map(|entry| entry.as_str().expect("entry").to_owned()),
            );
        }
        pages += 1;
        pending = data.pointer("/next/nextPage/input").cloned();
        if let Some(input) = &pending {
            support::assert_cursor_only(input);
        }
        assert!(pages <= 10, "bounded pages");
    }
    (listed, pages)
}

/// `sort` ranks the whole listing before a page is cut: each key and order
/// lists every entry once across its pages, in that order. Equal sizes keep
/// path order; a folder has no size and ranks last in either direction.
#[tokio::test]
async fn a_sorted_tree_ranks_the_whole_listing_and_pages_it_stably() {
    let entries = [
        ("a.rs", "file", 30),
        ("b.rs", "file", 500),
        ("c.rs", "file", 30),
        ("docs", "dir", 0),
        ("e.rs", "file", 7),
        ("src", "dir", 0),
    ];
    let server = sized_server(&entries).await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    for (sort, expected) in [
        (
            json!({"by":"sizeBytes","order":"desc"}),
            ["b.rs (500)", "a.rs (30)", "c.rs (30)", "e.rs (7)", "docs/", "src/"],
        ),
        (
            json!({"by":"sizeBytes","order":"asc"}),
            ["e.rs (7)", "a.rs (30)", "c.rs (30)", "b.rs (500)", "docs/", "src/"],
        ),
        (
            json!({"by":"path","order":"desc"}),
            ["src/", "e.rs (7)", "docs/", "c.rs (30)", "b.rs (500)", "a.rs (30)"],
        ),
        (
            json!({"by":"path","order":"asc"}),
            ["a.rs (30)", "b.rs (500)", "c.rs (30)", "docs/", "e.rs (7)", "src/"],
        ),
    ] {
        for limit in [1, 2, 4, 6] {
            let (listed, pages) = walk_sorted(&runtime, sort.clone(), limit).await;
            assert_eq!(listed, expected, "{sort} limit {limit}");
            assert_eq!(pages as u64, 6_u64.div_ceil(limit), "{sort} limit {limit}");
        }
    }
    // The largest file is the first row of a one-row page.
    let outcome = call(
        &runtime,
        "ghStructure",
        json!({"queries":[{"operation":"tree","owner":"a","repo":"b","limit":1,"sort":{"by":"sizeBytes","order":"desc"}}]}),
    )
    .await
    .expect("largest");
    assert_eq!(
        row_data(&outcome)["files"],
        json!([{"dir":".","files":["b.rs (500)"]}])
    );
    // A cursor is its own request: a sort beside it is refused.
    let cursor = row_data(&outcome)
        .pointer("/next/nextPage/input/cursor")
        .cloned()
        .expect("cursor");
    let error = call(
        &runtime,
        "ghStructure",
        json!({"queries":[{"cursor":cursor,"sort":{"by":"path","order":"asc"}}]}),
    )
    .await
    .expect_err("a cursor with another sort");
    assert_eq!(error.code, ErrorCode::InvalidInput);
    runtime.close().await;
}

/// A sort the schema does not offer is refused before any provider read.
#[tokio::test]
async fn an_unknown_tree_sort_key_is_refused() {
    let workspace = Workspace::new();
    let runtime = workspace.runtime(&[("GITHUB_API_URL", "http://127.0.0.1:9/api/v3".into())]);
    for sort in [
        json!({"by":"modifiedAt","order":"desc"}),
        json!({"by":"sizeBytes","order":"largest"}),
    ] {
        let error = call(
            &runtime,
            "ghStructure",
            json!({"operation":"tree","owner":"a","repo":"b","sort":sort}),
        )
        .await
        .expect_err("unsupported sort");
        assert_eq!(error.code, ErrorCode::InvalidInput);
    }
    runtime.close().await;
}

/// A rank over a walk that could not read a folder says so: the page is
/// partial, and the sort is over what was read.
#[tokio::test]
async fn a_sort_over_a_partial_walk_says_what_it_ranked() {
    let server = sized_server(&[("a.rs", "file", 30), ("b.rs", "file", 500), ("sub", "dir", 0)]).await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/sub"))
        .respond_with(ResponseTemplate::new(500))
        // Before the empty folder `sized_server` mounts.
        .with_priority(1)
        .mount(&server)
        .await;
    let workspace = Workspace::new();
    let runtime = github(&workspace, &server);
    let query = |sort: serde_json::Value| {
        json!({"queries":[{"operation":"tree","owner":"a","repo":"b","maxDepth":2,"sort":sort}]})
    };
    let ranked = call(&runtime, "ghStructure", query(json!({"by":"sizeBytes","order":"desc"})))
        .await
        .expect("ranked");
    let data = row_data(&ranked);
    assert_eq!(data["isPartial"], true, "{data}");
    let warned = |data: &serde_json::Value| {
        data["warnings"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|warning| warning.as_str().is_some_and(|text| text.starts_with("sort ranks only")))
    };
    assert!(warned(data), "{data}");
    assert_eq!(data["files"][0]["files"][0], "b.rs (500)", "{data}");
    // Path order claims no rank, so it adds no such warning.
    let listed = call(&runtime, "ghStructure", query(json!({"by":"path","order":"asc"})))
        .await
        .expect("listed");
    let data = row_data(&listed);
    assert_eq!(data["isPartial"], true, "{data}");
    assert!(!warned(data), "{data}");
    runtime.close().await;
}
