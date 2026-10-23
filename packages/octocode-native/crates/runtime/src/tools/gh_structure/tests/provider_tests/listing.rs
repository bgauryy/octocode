//! Tree listings: normalized paths, row anchors, truncated trees, the contents cap and non-directory paths.
use super::*;

/// A listing path is read as the repository names it (`./`, `//`
/// and `a/../` resolved), and its leads name that path; a path that
/// climbs above the root is invalid input, before any request.
#[tokio::test]
async fn listing_paths_are_normalized_and_escapes_rejected() {
    let server = server().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Flib",
        200,
        json!([{"name":"a.rs","path":"src/lib/a.rs","type":"file","size":3}]),
    )
    .await;
    let home = tempfile::tempdir().expect("home");
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    for requested in ["./src/lib", "src//lib/", "src/x/../lib"] {
        let out = run(
            &provider,
            &query(json!({"owner":"a","repo":"b","path":requested})),
            Ok(&context),
            home.path(),
        )
        .await
        .unwrap_or_else(|failure| panic!("{requested}: {}", failure.message));
        assert_eq!(
            out.data["files"][0]["dir"], "src/lib",
            "{requested}: {}",
            out.data
        );
    }
    let before = server.received_requests().await.unwrap_or_default().len();
    for escape in ["../..", "../../etc", "src/../../x"] {
        let failure = run(
            &provider,
            &query(json!({"owner":"a","repo":"b","path":escape})),
            Ok(&context),
            home.path(),
        )
        .await
        .expect_err(escape);
        assert_eq!(
            failure.error.kind,
            ProviderErrorKind::Validation,
            "{escape}"
        );
    }
    let after = server.received_requests().await.unwrap_or_default().len();
    assert_eq!(before, after, "an escape sends no request");
}

/// The happy-path anchors: each `dir` with its entry names (the
/// next read's path), the resolved branch and its `commitSha` (the
/// next read's ref); no query field is echoed.
#[tokio::test]
async fn a_listing_row_carries_dir_entries_commit_and_resolved_ref() {
    let server = server().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!([
            {"name":"lib.rs","path":"lib.rs","type":"file","size":3},
            {"name":"src","path":"src","type":"dir"}
        ]),
    )
    .await;
    let out = list(&server, json!({"owner":"a","repo":"b"}))
        .await
        .expect("listing");
    let data = &out.data;
    assert_eq!(
        data["files"],
        json!([{"dir":".","files":["lib.rs (3)","src/"]}]),
        "{data}"
    );
    assert_eq!(data["resolvedRef"], "main", "{data}");
    assert_eq!(data["commitSha"], SHA, "{data}");
    for echo in ["owner", "repo", "path", "operation", "ref"] {
        assert!(data.get(echo).is_none(), "{echo} echoed: {data}");
    }
    let read = &data["next"]["read"]["query"]["queries"][0];
    assert_eq!(read["path"], "lib.rs", "{data}");
    assert_eq!(read["ref"], SHA, "{data}");
}

/// A recursive tree past GitHub's limit is read subtree by subtree:
/// a folder's whole subtree comes from one recursive read by its
/// tree SHA (no per-folder Contents walk), so a bare-word `include`
/// finds names at any depth. Pages are disjoint, and each page's
/// continuation is the whole query pinned to the listed commit.
#[tokio::test]
async fn a_truncated_tree_is_read_by_subtree_and_pages_without_overlap() {
    let server = server().await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{SHA}"),
        200,
        json!({"sha":SHA,"truncated":true,"tree":[]}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!([
            {"name":"README.md","path":"README.md","type":"file","size":1},
            {"name":"src","path":"src","type":"dir","sha":"d".repeat(40)}
        ]),
    )
    .await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{}", "d".repeat(40)),
        200,
        json!({"sha":"d".repeat(40),"truncated":false,"tree":[
            {"path":"a","type":"tree"},
            {"path":"a/b","type":"tree"},
            {"path":"a/b/handler.rs","type":"blob","size":5},
            {"path":"a/b/other.rs","type":"blob","size":5},
            {"path":"a/handler_test.rs","type":"blob","size":5}
        ]}),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/a/b/contents/src"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(0)
        .mount(&server)
        .await;
    let base = json!({"owner":"a","repo":"b","ref":"main","include":["handler"],"pageSize":1});
    let first = list(&server, base.clone()).await.expect("page 1");
    let next = &first.data["next"]["nextPage"]["query"]["queries"][0];
    assert_eq!(next["include"], json!(["handler"]), "{}", first.data);
    assert_eq!(next["ref"], SHA, "{}", first.data);
    assert_eq!(next["pageSize"], 1, "{}", first.data);
    assert_eq!(next["page"], 2, "{}", first.data);
    assert!(first.data.get("terminalLimit").is_none(), "{}", first.data);
    let second = list(&server, next.clone()).await.expect("page 2");
    assert!(
        second.data["next"].get("nextPage").is_none(),
        "{}",
        second.data
    );
    let rows = [&first.data, &second.data].map(|data| data["files"].clone());
    assert_eq!(
        json!(rows),
        json!([
            [{"dir":"src/a","files":["handler_test.rs (5)"]}],
            [{"dir":"src/a/b","files":["handler.rs (5)"]}]
        ]),
        "{} {}",
        first.data,
        second.data
    );
}

/// A directory past the Contents API's 1,000-entry cap is listed
/// whole from its own git tree (the root's is the commit's; a scoped
/// directory's SHA comes from its parent listing) and paged like any
/// listing: every entry once, no provider limit, no clone lead.
#[tokio::test]
async fn a_directory_past_the_contents_cap_is_listed_whole_and_paged() {
    let names = |count: usize| (0..count).map(|n| format!("f{n:04}.rs"));
    let contents: Vec<Value> = names(1000)
        .map(|name| json!({"name":name,"path":name,"type":"file","size":1}))
        .collect();
    let tree: Vec<Value> = names(1200)
        .map(|name| json!({"path":name,"type":"blob","size":1}))
        .collect();
    let server = server().await;
    mount_json(&server, "/api/v3/repos/a/b/contents", 200, json!(contents)).await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{SHA}"),
        200,
        json!({"sha":SHA,"truncated":false,"tree":tree}),
    )
    .await;
    let mut listed = Vec::new();
    let mut row = json!({"owner":"a","repo":"b","pageSize":500});
    for page in 1..=3 {
        let out = list(&server, row.clone()).await.expect("listing");
        let data = &out.data;
        assert!(data.get("terminalLimit").is_none(), "page {page}: {data}");
        assert!(data.get("providerLimit").is_none(), "page {page}: {data}");
        assert!(
            data["next"].get("cloneRepo").is_none(),
            "page {page}: {data}"
        );
        for name in data["files"][0]["files"].as_array().expect("files") {
            listed.push(name.as_str().unwrap_or_default().to_owned());
        }
        match data["next"].get("nextPage") {
            Some(next) => row = next["query"]["queries"][0].clone(),
            None => assert_eq!(page, 3, "{data}"),
        }
    }
    let expected: Vec<String> = names(1200).map(|name| format!("{name} (1)")).collect();
    assert_eq!(listed, expected);

    // A scoped directory: its tree SHA is read from its parent.
    let dir = "e".repeat(40);
    let scoped: Vec<Value> = names(1000)
        .map(|name| json!({"name":name,"path":format!("big/{name}"),"type":"file","size":1}))
        .collect();
    let server = self::server().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents",
        200,
        json!([{"name":"big","path":"big","type":"dir","sha":dir}]),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/big",
        200,
        json!(scoped),
    )
    .await;
    mount_json(
        &server,
        format!("/api/v3/repos/a/b/git/trees/{dir}"),
        200,
        json!({"sha":dir,"truncated":false,"tree":tree}),
    )
    .await;
    let out = list(
        &server,
        json!({"owner":"a","repo":"b","path":"big","pageSize":500,"page":3}),
    )
    .await
    .expect("scoped listing");
    assert!(out.data.get("providerLimit").is_none(), "{}", out.data);
    assert_eq!(out.data["files"][0]["dir"], "big", "{}", out.data);
    assert_eq!(
        out.data["files"][0]["files"].as_array().map(Vec::len),
        Some(200),
        "{}",
        out.data
    );
}

/// A listing of a file is typed; its one repair is that file's read
/// at the commit the listing resolved (not the movable ref), with no
/// prose beside it.
#[tokio::test]
async fn a_listing_of_a_file_leads_to_its_read() {
    let server = server().await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/src%2Flib.rs",
        200,
        json!({"name":"lib.rs","path":"src/lib.rs","type":"file","size":3}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    let home = tempfile::tempdir().expect("home");
    let failure = run(
        &provider,
        &query(json!({"owner":"a","repo":"b","path":"src/lib.rs","ref":"main"})),
        Ok(&context),
        home.path(),
    )
    .await
    .expect_err("a file is not a tree");
    assert_eq!(failure.error.reason, Some(ProviderErrorReason::PathIsFile));
    assert_eq!(
        failure.message,
        "Path \"src/lib.rs\" is a file, not a directory"
    );
    assert!(failure.hints.is_empty(), "{:?}", failure.hints);
    let next = failure.next.expect("read lead");
    assert_eq!(next["read"]["tool"], "ghGetFileContent", "{next}");
    assert_eq!(
        next["read"]["query"]["queries"][0],
        json!({"owner":"a","repo":"b","path":"src/lib.rs","ref":SHA}),
        "{next}"
    );
}

/// A listing of a submodule leads to the submodule's repository at
/// the commit the parent pins, as a read of it does; a symlink is
/// read (the read follows it to its file). Neither gets the generic
/// invalid-field tip.
#[tokio::test]
async fn a_listing_of_a_submodule_or_symlink_leads_to_its_content() {
    let server = server().await;
    let pinned = "89abcdef0123456789abcdef0123456789abcdef";
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/vendor%2Flib",
        200,
        json!({"type":"submodule","name":"lib","path":"vendor/lib","sha":pinned,
            "submodule_git_url":format!("{}/o2/r2.git", server.uri())}),
    )
    .await;
    mount_json(
        &server,
        "/api/v3/repos/a/b/contents/docs%2Flink",
        200,
        json!({"type":"symlink","name":"link","path":"docs/link","target":"../README.md"}),
    )
    .await;
    let provider = mock_provider(&server, RetryPolicy::default());
    let context = default_context();
    let home = tempfile::tempdir().expect("home");
    let listing = |path: &str| query(json!({"owner":"a","repo":"b","path":path,"ref":"main"}));
    let submodule = run(&provider, &listing("vendor/lib"), Ok(&context), home.path())
        .await
        .expect_err("a submodule is not a tree");
    assert_eq!(submodule.error.reason, Some(ProviderErrorReason::Submodule));
    assert!(submodule.hints.is_empty(), "{:?}", submodule.hints);
    let next = submodule.next.expect("submodule lead");
    let row = &next["viewTree"]["query"]["queries"][0];
    assert_eq!(next["viewTree"]["tool"], "ghStructure", "{next}");
    assert_eq!(
        (&row["owner"], &row["repo"]),
        (&json!("o2"), &json!("r2")),
        "{next}"
    );
    assert_eq!(row["ref"], pinned, "{next}");
    let symlink = run(&provider, &listing("docs/link"), Ok(&context), home.path())
        .await
        .expect_err("a symlink is not a tree");
    assert_eq!(
        symlink.message,
        "Path \"docs/link\" is a symlink, not a directory"
    );
    let next = symlink.next.expect("read lead");
    assert_eq!(
        next["read"]["query"]["queries"][0],
        json!({"owner":"a","repo":"b","path":"docs/link","ref":SHA}),
        "{next}"
    );
}
